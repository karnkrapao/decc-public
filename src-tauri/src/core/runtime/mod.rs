use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::Value;

use crate::models::{
    runtime::{
        ComponentRuntimeInspection, RuntimeDiagnosticStatus, ToolRuntimeDiagnostic,
        WorkspaceRuntimeInspection,
    },
    workspace::{PersistedComponent, PersistedWorkspace},
};

#[derive(Debug, Clone)]
struct Requirement {
    display: String,
    source: String,
    constraint: VersionConstraint,
    authoritative: bool,
}

#[derive(Debug, Clone)]
enum VersionConstraint {
    Minimum(Vec<u64>),
    Exact(Vec<u64>),
    Prefix(Vec<u64>),
    CompatibleMajor(Vec<u64>),
    Caret(Vec<u64>),
    Tilde(Vec<u64>),
    Unsupported,
}

#[derive(Debug, Clone)]
struct ToolSpec {
    key: String,
    label: String,
    program: String,
    version_args: Vec<String>,
    probe_version: bool,
    requirement: Option<Requirement>,
    search_path: Option<OsString>,
}

#[derive(Debug, Clone)]
struct NodeCandidate {
    executable: PathBuf,
    version: String,
    manager: String,
    from_current_path: bool,
}

#[derive(Debug, Clone)]
struct NodeResolution {
    executable: PathBuf,
    version: String,
    manager: String,
    bin_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct NodeResolutionError {
    status: RuntimeDiagnosticStatus,
    detail: String,
}

#[derive(Debug, Clone)]
pub struct ComponentExecution {
    pub program: PathBuf,
    pub path: Option<OsString>,
}

pub fn inspect(workspaces: &[PersistedWorkspace]) -> Vec<WorkspaceRuntimeInspection> {
    workspaces
        .iter()
        .map(|workspace| WorkspaceRuntimeInspection {
            workspace_id: workspace.id.clone(),
            components: workspace
                .components
                .iter()
                .map(|component| inspect_component(workspace, component))
                .collect(),
        })
        .collect()
}

pub fn inspect_component(
    workspace: &PersistedWorkspace,
    component: &PersistedComponent,
) -> ComponentRuntimeInspection {
    let working_directory = component_working_directory(workspace, component);
    let diagnostics = match working_directory {
        Ok(directory) => {
            let requirements = derive_requirements(workspace, component, &directory);
            let node_resolution = if component.runtime == "Node.js" {
                Some(resolve_node_runtime(&directory, requirements.get("node")))
            } else {
                None
            };
            let node_search_path = node_resolution
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .and_then(|resolution| prepend_path(&resolution.bin_dir));

            let mut diagnostics = Vec::new();
            if let Some(resolution) = node_resolution.as_ref() {
                diagnostics.push(inspect_node_runtime(requirements.get("node"), resolution));
            }
            diagnostics.extend(
                tool_specs(component, &requirements, node_search_path)
                    .into_iter()
                    .map(inspect_tool),
            );
            if let Some(diagnostic) = inspect_make_command_shape(component, &directory) {
                diagnostics.push(diagnostic);
            }

            if let Some(program) = component.program.as_deref() {
                if known_tool(program).is_none() {
                    diagnostics.push(ToolRuntimeDiagnostic {
                        key: "custom".to_string(),
                        label: format!("Custom tool ({program})"),
                        program: program.to_string(),
                        status: RuntimeDiagnosticStatus::Unknown,
                        installed_version: None,
                        required_version: None,
                        requirement_source: None,
                        detail: format!(
                            "DECC does not execute version probes for unrecognized program `{program}`."
                        ),
                        blocking: false,
                    });
                }
            }
            diagnostics
        }
        Err(error) => vec![ToolRuntimeDiagnostic {
            key: "workspace-path".to_string(),
            label: "Workspace path".to_string(),
            program: String::new(),
            status: RuntimeDiagnosticStatus::Unknown,
            installed_version: None,
            required_version: None,
            requirement_source: None,
            detail: error,
            blocking: true,
        }],
    };
    let blocker = diagnostics.iter().find(|diagnostic| diagnostic.blocking);

    ComponentRuntimeInspection {
        component_id: component.id.clone(),
        ready: blocker.is_none(),
        blocking_message: blocker.map(|diagnostic| diagnostic.detail.clone()),
        diagnostics,
    }
}

fn inspect_make_command_shape(
    component: &PersistedComponent,
    directory: &Path,
) -> Option<ToolRuntimeDiagnostic> {
    if component.program.as_deref() != Some("make")
        || component.args.as_slice() != ["run".to_string()]
    {
        return None;
    }

    let text = fs::read_to_string(directory.join("Makefile")).ok()?;
    let apps = make_declared_apps(&text);
    let run_requires_app = text.lines().any(|line| {
        let line = line.trim();
        line.starts_with("run:")
            && (line.contains("require-app") || text.contains("APP_REQUIRED_MSG"))
    });
    if !run_requires_app || apps.is_empty() {
        return None;
    }

    Some(ToolRuntimeDiagnostic {
        key: "make-command-shape".to_string(),
        label: "Make run target".to_string(),
        program: "make".to_string(),
        status: RuntimeDiagnosticStatus::Unknown,
        installed_version: None,
        required_version: None,
        requirement_source: Some("Makefile".to_string()),
        detail: format!(
            "This saved component uses `make run`, but the Makefile requires an app selection ({}). Rescan the workspace so DECC can persist explicit commands such as `make run {}` instead of guessing.",
            apps.join(", "),
            apps[0]
        ),
        blocking: true,
    })
}

fn make_declared_apps(text: &str) -> Vec<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        for key in ["VALID_APPS", "APPS"] {
            let Some(rest) = line.strip_prefix(key) else {
                continue;
            };
            let Some((operator, value)) = rest.trim_start().split_once('=') else {
                continue;
            };
            if !operator.trim().is_empty() && !matches!(operator.trim(), ":" | "?" | "+") {
                continue;
            }
            let values = value
                .split_whitespace()
                .filter(|value| {
                    !value.is_empty()
                        && value.chars().all(|character| {
                            character.is_ascii_alphanumeric() || "-_.".contains(character)
                        })
                })
                .map(str::to_string)
                .collect::<Vec<_>>();
            if !values.is_empty() {
                return values;
            }
        }
    }
    Vec::new()
}
fn inspect_node_runtime(
    requirement: Option<&Requirement>,
    resolution: &Result<NodeResolution, NodeResolutionError>,
) -> ToolRuntimeDiagnostic {
    let required_version = requirement.map(|item| item.display.clone());
    let requirement_source = requirement.map(|item| item.source.clone());

    let resolution = match resolution {
        Ok(resolution) => resolution,
        Err(error) => {
            return ToolRuntimeDiagnostic {
                key: "node".to_string(),
                label: "Node.js".to_string(),
                program: "node".to_string(),
                status: error.status.clone(),
                installed_version: None,
                required_version,
                requirement_source,
                detail: error.detail.clone(),
                blocking: true,
            };
        }
    };

    let status = requirement
        .map(|requirement| requirement_matches(&resolution.version, &requirement.constraint))
        .unwrap_or(Some(true));
    let (status, blocking) = match status {
        Some(true) => (RuntimeDiagnosticStatus::Ready, false),
        Some(false) => (RuntimeDiagnosticStatus::Incompatible, true),
        None => (RuntimeDiagnosticStatus::Unknown, false),
    };

    let detail = match requirement {
        Some(requirement) => format!(
            "Node.js {} resolved from {} at {} for {} from {}.",
            resolution.version,
            resolution.manager,
            resolution.executable.display(),
            requirement.display,
            requirement.source
        ),
        None => format!(
            "Node.js {} resolved from {} at {}.",
            resolution.version,
            resolution.manager,
            resolution.executable.display()
        ),
    };

    ToolRuntimeDiagnostic {
        key: "node".to_string(),
        label: if resolution.manager == "PATH" {
            "Node.js".to_string()
        } else {
            format!("Node.js · {}", resolution.manager)
        },
        program: "node".to_string(),
        status,
        installed_version: Some(resolution.version.clone()),
        required_version,
        requirement_source,
        detail,
        blocking,
    }
}

fn inspect_tool(spec: ToolSpec) -> ToolRuntimeDiagnostic {
    let requirement_display = spec.requirement.as_ref().map(|item| item.display.clone());
    let requirement_source = spec.requirement.as_ref().map(|item| item.source.clone());

    let executable = spec
        .search_path
        .as_deref()
        .and_then(|path| find_executable_with_path(&spec.program, Some(path)))
        .or_else(|| find_executable(&spec.program));
    let Some(executable) = executable else {
        return ToolRuntimeDiagnostic {
            key: spec.key,
            label: spec.label.clone(),
            program: spec.program.clone(),
            status: RuntimeDiagnosticStatus::Missing,
            installed_version: None,
            required_version: requirement_display,
            requirement_source,
            detail: format!(
                "{} (`{}`) was not found on DECC's inherited PATH or known local installation locations. Install it or make the existing installation discoverable to DECC.",
                spec.label, spec.program
            ),
            blocking: true,
        };
    };

    if !spec.probe_version {
        let (status, detail) = if let Some(requirement) = spec.requirement.as_ref() {
            (
                RuntimeDiagnosticStatus::Unknown,
                format!(
                    "{} is available to DECC, but its version is not executed during diagnostics to avoid activating package/tool-manager wrappers. Requirement: {} from {}.",
                    spec.label, requirement.display, requirement.source
                ),
            )
        } else {
            (
                RuntimeDiagnosticStatus::Ready,
                format!("{} is available to DECC.", spec.label),
            )
        };
        return ToolRuntimeDiagnostic {
            key: spec.key,
            label: spec.label,
            program: spec.program,
            status,
            installed_version: None,
            required_version: requirement_display,
            requirement_source,
            detail,
            blocking: false,
        };
    }

    let mut command = Command::new(&executable);
    command
        .args(&spec.version_args)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null());
    if spec.key == "go" {
        command.env("GOTOOLCHAIN", "local");
    }
    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            return ToolRuntimeDiagnostic {
                key: spec.key,
                label: spec.label.clone(),
                program: spec.program.clone(),
                status: RuntimeDiagnosticStatus::Unknown,
                installed_version: None,
                required_version: requirement_display,
                requirement_source,
                detail: format!(
                    "{} is present at {}, but DECC could not run its safe version probe: {error}",
                    spec.label,
                    executable.display()
                ),
                blocking: false,
            };
        }
    };

    let combined = version_output_text(&output.stdout, &output.stderr);
    let installed_version = extract_numeric_version(&combined);

    if !output.status.success() {
        let detail = if combined.is_empty() {
            format!(
                "{} is available, but its version probe exited with status {}.",
                spec.label, output.status
            )
        } else {
            format!(
                "{} is available, but its version probe failed: {}",
                spec.label,
                single_line(&combined)
            )
        };
        return ToolRuntimeDiagnostic {
            key: spec.key,
            label: spec.label,
            program: spec.program,
            status: RuntimeDiagnosticStatus::Unknown,
            installed_version,
            required_version: requirement_display,
            requirement_source,
            detail,
            blocking: false,
        };
    }

    let Some(requirement) = spec.requirement else {
        let detail = match &installed_version {
            Some(version) => format!("{} {version} is available to DECC.", spec.label),
            None => format!("{} is available to DECC.", spec.label),
        };
        return ToolRuntimeDiagnostic {
            key: spec.key,
            label: spec.label,
            program: spec.program,
            status: RuntimeDiagnosticStatus::Ready,
            installed_version,
            required_version: None,
            requirement_source: None,
            detail,
            blocking: false,
        };
    };

    let Some(installed) = installed_version.as_deref() else {
        return ToolRuntimeDiagnostic {
            key: spec.key,
            label: spec.label.clone(),
            program: spec.program,
            status: RuntimeDiagnosticStatus::Unknown,
            installed_version: None,
            required_version: Some(requirement.display.clone()),
            requirement_source: Some(requirement.source.clone()),
            detail: format!(
                "{} is available, but DECC could not parse its version to verify {} from {}.",
                spec.label, requirement.display, requirement.source
            ),
            blocking: false,
        };
    };

    match requirement_matches(installed, &requirement.constraint) {
        Some(true) => ToolRuntimeDiagnostic {
            key: spec.key,
            label: spec.label.clone(),
            program: spec.program,
            status: RuntimeDiagnosticStatus::Ready,
            installed_version: Some(installed.to_string()),
            required_version: Some(requirement.display.clone()),
            requirement_source: Some(requirement.source.clone()),
            detail: format!(
                "{} {installed} satisfies {} from {}.",
                spec.label, requirement.display, requirement.source
            ),
            blocking: false,
        },
        Some(false) => ToolRuntimeDiagnostic {
            key: spec.key,
            label: spec.label.clone(),
            program: spec.program,
            status: RuntimeDiagnosticStatus::Incompatible,
            installed_version: Some(installed.to_string()),
            required_version: Some(requirement.display.clone()),
            requirement_source: Some(requirement.source.clone()),
            detail: format!(
                "{} {installed} does not satisfy {} from {}. Install/use a compatible version and relaunch DECC.",
                spec.label, requirement.display, requirement.source
            ),
            blocking: true,
        },
        None => ToolRuntimeDiagnostic {
            key: spec.key,
            label: spec.label.clone(),
            program: spec.program,
            status: RuntimeDiagnosticStatus::Unknown,
            installed_version: Some(installed.to_string()),
            required_version: Some(requirement.display.clone()),
            requirement_source: Some(requirement.source.clone()),
            detail: format!(
                "{} {installed} is available. DECC does not yet safely evaluate the version range {} from {}.",
                spec.label, requirement.display, requirement.source
            ),
            blocking: false,
        },
    }
}

fn tool_specs(
    component: &PersistedComponent,
    requirements: &BTreeMap<String, Requirement>,
    node_search_path: Option<OsString>,
) -> Vec<ToolSpec> {
    let mut tools = BTreeMap::<String, ToolSpec>::new();

    if let Some(program) = component.program.as_deref() {
        if let Some((key, label, version_args, probe_version)) = known_tool(program) {
            tools.insert(
                key.to_string(),
                ToolSpec {
                    key: key.to_string(),
                    label: label.to_string(),
                    program: program.to_string(),
                    version_args: version_args.iter().map(|value| value.to_string()).collect(),
                    probe_version,
                    requirement: requirements.get(key).cloned(),
                    search_path: if component.runtime == "Node.js" {
                        node_search_path.clone()
                    } else {
                        None
                    },
                },
            );
        }
    }

    if directory_uses_go(component, requirements) {
        insert_known_tool(&mut tools, "go", requirements.get("go").cloned(), None);
    }

    if component.runtime == "Cargo" || component.technology == "Rust" {
        insert_known_tool(
            &mut tools,
            "rustc",
            requirements.get("rustc").cloned(),
            None,
        );
    }

    if component.program.as_deref() == Some("make") && requirements.contains_key("go") {
        insert_known_tool(
            &mut tools,
            "go",
            requirements.get("go").cloned(),
            augmented_execution_path(),
        );
    }

    tools.into_values().collect()
}

fn directory_uses_go(
    component: &PersistedComponent,
    requirements: &BTreeMap<String, Requirement>,
) -> bool {
    requirements.contains_key("go")
        && matches!(
            component.program.as_deref(),
            Some("go" | "make" | "task" | "just")
        )
}

fn insert_known_tool(
    tools: &mut BTreeMap<String, ToolSpec>,
    program: &str,
    requirement: Option<Requirement>,
    search_path: Option<OsString>,
) {
    let Some((key, label, version_args, probe_version)) = known_tool(program) else {
        return;
    };
    tools.entry(key.to_string()).or_insert_with(|| ToolSpec {
        key: key.to_string(),
        label: label.to_string(),
        program: program.to_string(),
        version_args: version_args.iter().map(|value| value.to_string()).collect(),
        probe_version,
        requirement,
        search_path,
    });
}

fn known_tool(
    program: &str,
) -> Option<(&'static str, &'static str, &'static [&'static str], bool)> {
    match program {
        "node" => Some(("node", "Node.js", &["--version"], true)),
        "npm" => Some(("npm", "npm", &["--version"], false)),
        "pnpm" => Some(("pnpm", "pnpm", &["--version"], false)),
        "yarn" => Some(("yarn", "Yarn", &["--version"], false)),
        "bun" => Some(("bun", "Bun", &["--version"], true)),
        "go" => Some(("go", "Go", &["version"], true)),
        "cargo" => Some(("cargo", "Cargo", &["--version"], false)),
        "rustc" => Some(("rustc", "Rust", &["--version"], false)),
        "dotnet" => Some(("dotnet", ".NET SDK", &["--version"], true)),
        "docker" => Some(("docker", "Docker CLI", &["--version"], true)),
        "make" => Some(("make", "Make", &["--version"], true)),
        "task" => Some(("task", "Task", &["--version"], true)),
        "just" => Some(("just", "just", &["--version"], true)),
        _ => None,
    }
}

fn derive_requirements(
    workspace: &PersistedWorkspace,
    component: &PersistedComponent,
    directory: &Path,
) -> BTreeMap<String, Requirement> {
    let mut requirements = BTreeMap::new();

    if directory.join("package.json").is_file() {
        derive_javascript_requirements(directory, &mut requirements);
    }
    if component.runtime == "Node.js" {
        derive_node_project_requirement(workspace, directory, &mut requirements);
        if !requirements.contains_key("node") {
            derive_node_dockerfile_requirement(workspace, directory, &mut requirements);
        }
    }
    if directory.join("go.mod").is_file() {
        derive_go_requirement(directory, &mut requirements);
    }
    if directory.join("Cargo.toml").is_file() {
        derive_rust_requirement(directory, &mut requirements);
    }
    if component.program.as_deref() == Some("dotnet") {
        derive_dotnet_requirement(component, directory, &mut requirements);
    }

    requirements
}

fn derive_javascript_requirements(
    directory: &Path,
    requirements: &mut BTreeMap<String, Requirement>,
) {
    let Ok(text) = fs::read_to_string(directory.join("package.json")) else {
        return;
    };
    let Ok(package) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    let Some(engines) = package.get("engines").and_then(Value::as_object) else {
        return;
    };

    for (key, label) in [
        ("node", "Node.js"),
        ("npm", "npm"),
        ("pnpm", "pnpm"),
        ("yarn", "Yarn"),
        ("bun", "Bun"),
    ] {
        let Some(display) = engines.get(key).and_then(Value::as_str) else {
            continue;
        };
        requirements.insert(
            key.to_string(),
            Requirement {
                display: display.to_string(),
                source: format!("package.json engines.{key}"),
                constraint: parse_requirement(display),
                authoritative: false,
            },
        );
        let _ = label;
    }
}

fn derive_node_project_requirement(
    workspace: &PersistedWorkspace,
    directory: &Path,
    requirements: &mut BTreeMap<String, Requirement>,
) {
    let Ok(root) = fs::canonicalize(&workspace.path) else {
        return;
    };
    let mut current = Some(directory);

    while let Some(path) = current {
        if let Some((value, source)) = node_pin_from_directory(&root, path) {
            requirements.insert(
                "node".to_string(),
                Requirement {
                    display: value.clone(),
                    source,
                    constraint: parse_node_pin(&value),
                    authoritative: true,
                },
            );
            return;
        }

        if path == root {
            break;
        }
        current = path.parent().filter(|parent| parent.starts_with(&root));
    }
}

fn node_pin_from_directory(root: &Path, directory: &Path) -> Option<(String, String)> {
    if let Some(value) = read_node_version_file(&directory.join(".nvmrc")) {
        return Some((value, relative_source(root, &directory.join(".nvmrc"))));
    }
    if let Some(value) = read_node_version_file(&directory.join(".node-version")) {
        return Some((
            value,
            relative_source(root, &directory.join(".node-version")),
        ));
    }

    let package_path = directory.join("package.json");
    if let Ok(text) = fs::read_to_string(&package_path) {
        if let Ok(package) = serde_json::from_str::<Value>(&text) {
            if let Some(value) = package
                .get("volta")
                .and_then(Value::as_object)
                .and_then(|volta| volta.get("node"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                return Some((
                    value.to_string(),
                    format!("{} volta.node", relative_source(root, &package_path)),
                ));
            }
        }
    }

    for name in ["mise.toml", ".mise.toml"] {
        let mise_file = directory.join(name);
        if let Ok(text) = fs::read_to_string(&mise_file) {
            if let Some(value) = mise_node_version(&text) {
                return Some((value, relative_source(root, &mise_file)));
            }
        }
    }

    let tool_versions = directory.join(".tool-versions");
    if let Ok(text) = fs::read_to_string(&tool_versions) {
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(tool) = parts.next() else {
                continue;
            };
            if !matches!(tool, "node" | "nodejs") {
                continue;
            }
            let Some(value) = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            return Some((value.to_string(), relative_source(root, &tool_versions)));
        }
    }

    None
}

fn mise_node_version(text: &str) -> Option<String> {
    let mut in_tools = false;
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_tools = line == "[tools]";
            continue;
        }
        if !in_tools {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !matches!(key.trim(), "node" | "nodejs") {
            continue;
        }
        let value = value.trim().trim_matches(['"', '\'']).trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn read_node_version_file(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
}

fn relative_source(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
}

fn parse_node_pin(value: &str) -> VersionConstraint {
    let normalized = value.trim().trim_start_matches('v');
    if !starts_with_digit(normalized) {
        return VersionConstraint::Unsupported;
    }
    let numbers = parse_version_numbers(normalized);
    if numbers.is_empty() {
        VersionConstraint::Unsupported
    } else {
        VersionConstraint::CompatibleMajor(numbers)
    }
}

fn derive_node_dockerfile_requirement(
    workspace: &PersistedWorkspace,
    directory: &Path,
    requirements: &mut BTreeMap<String, Requirement>,
) {
    let Ok(root) = fs::canonicalize(&workspace.path) else {
        return;
    };
    let mut current = Some(directory);

    while let Some(path) = current {
        let hints = dockerfile_node_hints(&root, path);
        if !hints.is_empty() {
            let majors = hints
                .iter()
                .filter_map(|hint| hint.0.first().copied())
                .collect::<BTreeSet<_>>();

            let selected_major = if majors.len() == 1 {
                majors.iter().next().copied()
            } else {
                let specificity_by_major =
                    hints
                        .iter()
                        .fold(BTreeMap::<u64, usize>::new(), |mut values, (version, _)| {
                            if let Some(major) = version.first().copied() {
                                values
                                    .entry(major)
                                    .and_modify(|specificity| {
                                        *specificity = (*specificity).max(version.len())
                                    })
                                    .or_insert(version.len());
                            }
                            values
                        });
                let strongest = specificity_by_major
                    .values()
                    .copied()
                    .max()
                    .unwrap_or_default();
                let strongest_majors = specificity_by_major
                    .iter()
                    .filter_map(|(major, specificity)| {
                        (*specificity == strongest).then_some(*major)
                    })
                    .collect::<Vec<_>>();
                (strongest_majors.len() == 1).then_some(strongest_majors[0])
            };

            if let Some(major) = selected_major {
                let selected_hints = hints
                    .iter()
                    .filter(|(version, _)| version.first().copied() == Some(major))
                    .collect::<Vec<_>>();
                let minimum = selected_hints
                    .iter()
                    .map(|(version, _)| version.clone())
                    .max_by(|left, right| compare_versions(left, right).cmp(&0))
                    .unwrap_or_else(|| vec![major]);
                let display_version = minimum
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                let display = if minimum.len() == 1 {
                    format!("{major}.x")
                } else {
                    format!(">= {display_version} < {}", major + 1)
                };
                let mut source = selected_hints
                    .iter()
                    .map(|(_, source)| source.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ");
                if majors.len() > 1 {
                    source.push_str(" (preferred because this Dockerfile pin is more specific than the conflicting major-only hint)");
                }
                requirements.insert(
                    "node".to_string(),
                    Requirement {
                        display,
                        source,
                        constraint: VersionConstraint::CompatibleMajor(minimum),
                        authoritative: false,
                    },
                );
            } else {
                requirements.insert(
                    "node".to_string(),
                    Requirement {
                        display: format!(
                            "conflicting Dockerfile Node majors ({})",
                            majors
                                .iter()
                                .map(u64::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        source: "Dockerfile variants".to_string(),
                        constraint: VersionConstraint::Unsupported,
                        authoritative: true,
                    },
                );
            }
            return;
        }

        if path == root {
            break;
        }
        current = path.parent().filter(|parent| parent.starts_with(&root));
    }
}

fn dockerfile_node_hints(root: &Path, directory: &Path) -> Vec<(Vec<u64>, String)> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut hints = Vec::new();

    for entry in entries.flatten().take(64) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != "Dockerfile" && !name.starts_with("Dockerfile.") {
            continue;
        }
        let path = entry.path();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > 512 * 1024 {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };

        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(keyword) = parts.next() else {
                continue;
            };
            if !keyword.eq_ignore_ascii_case("FROM") {
                continue;
            }
            let image = parts.find(|part| !part.starts_with("--"));
            let Some(image) = image else {
                continue;
            };
            let Some((version, tag)) = docker_node_image_version(image) else {
                continue;
            };
            hints.push((
                version,
                format!("{} node:{tag}", relative_source(root, &path)),
            ));
        }
    }

    hints
}

fn docker_node_image_version(image: &str) -> Option<(Vec<u64>, String)> {
    let image = image.split('@').next().unwrap_or(image);
    let image_name = image.rsplit('/').next()?;
    let (repository, tag) = image_name.split_once(':')?;
    if repository != "node" {
        return None;
    }
    let numeric = tag
        .trim_start_matches('v')
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect::<String>();
    let version = parse_version_numbers(&numeric);
    let major = version.first().copied()?;
    (major > 0).then_some((version, tag.to_string()))
}

fn derive_go_requirement(directory: &Path, requirements: &mut BTreeMap<String, Requirement>) {
    let Ok(text) = fs::read_to_string(directory.join("go.mod")) else {
        return;
    };

    let toolchain = text.lines().map(str::trim).find_map(|line| {
        line.strip_prefix("toolchain ")
            .map(str::trim)
            .and_then(|value| value.strip_prefix("go").or(Some(value)))
            .filter(|value| starts_with_digit(value))
            .map(str::to_string)
    });
    let language = text.lines().map(str::trim).find_map(|line| {
        line.strip_prefix("go ")
            .map(str::trim)
            .filter(|value| starts_with_digit(value))
            .map(str::to_string)
    });
    let version = toolchain.or(language);
    if let Some(version) = version {
        requirements.insert(
            "go".to_string(),
            Requirement {
                display: format!(">= {version}"),
                source: "go.mod".to_string(),
                constraint: VersionConstraint::Minimum(parse_version_numbers(&version)),
                authoritative: false,
            },
        );
    }
}

fn derive_rust_requirement(directory: &Path, requirements: &mut BTreeMap<String, Requirement>) {
    let Ok(text) = fs::read_to_string(directory.join("Cargo.toml")) else {
        return;
    };
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "rust-version" {
            continue;
        }
        let version = value.trim().trim_matches(['"', '\'']);
        if version.is_empty() || !starts_with_digit(version) {
            return;
        }
        requirements.insert(
            "rustc".to_string(),
            Requirement {
                display: format!(">= {version}"),
                source: "Cargo.toml rust-version".to_string(),
                constraint: VersionConstraint::Minimum(parse_version_numbers(version)),
                authoritative: false,
            },
        );
        return;
    }
}

fn derive_dotnet_requirement(
    component: &PersistedComponent,
    directory: &Path,
    requirements: &mut BTreeMap<String, Requirement>,
) {
    let project = component
        .args
        .windows(2)
        .find(|pair| pair[0] == "--project")
        .map(|pair| pair[1].as_str());
    let Some(project) = project else {
        return;
    };
    let Ok(text) = fs::read_to_string(directory.join(project)) else {
        return;
    };
    let target = xml_value(&text, "TargetFramework").or_else(|| {
        xml_value(&text, "TargetFrameworks")
            .and_then(|value| value.split(';').next().map(str::to_string))
    });
    let Some(target) = target else {
        return;
    };
    let Some(version) = dotnet_target_version(&target) else {
        return;
    };
    requirements.insert(
        "dotnet".to_string(),
        Requirement {
            display: format!(">= {version}"),
            source: format!("{project} TargetFramework"),
            constraint: VersionConstraint::Minimum(parse_version_numbers(&version)),
            authoritative: false,
        },
    );
}

fn xml_value(text: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{tag}>");
    let end_tag = format!("</{tag}>");
    let start = text.find(&start_tag)? + start_tag.len();
    let end = text[start..].find(&end_tag)? + start;
    let value = text[start..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn dotnet_target_version(target: &str) -> Option<String> {
    let target = target.trim();
    let value = if let Some(value) = target.strip_prefix("netcoreapp") {
        value
    } else {
        let value = target.strip_prefix("net")?;
        if value.starts_with("standard") {
            return None;
        }
        value
    };

    let numeric = value
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect::<String>();
    if !numeric.contains('.') {
        return None;
    }

    let major = numeric.split('.').next()?.parse::<u64>().ok()?;
    (major >= 3).then_some(numeric)
}

pub fn resolve_component_execution(
    workspace: &PersistedWorkspace,
    component: &PersistedComponent,
) -> Result<ComponentExecution, String> {
    let program = component.program.as_deref().ok_or_else(|| {
        "This component predates the native execution contract. Rescan and add the workspace again."
            .to_string()
    })?;
    let working_directory = component_working_directory(workspace, component)?;

    if component.runtime != "Node.js" {
        let path = augmented_execution_path();
        let executable = if known_tool(program).is_some() {
            find_executable_with_path(program, path.as_deref()).ok_or_else(|| {
                format!(
                    "{program} is not available on the DECC process PATH or in its known installation locations."
                )
            })?
        } else {
            PathBuf::from(program)
        };
        return Ok(ComponentExecution {
            program: executable,
            path,
        });
    }

    let requirements = derive_requirements(workspace, component, &working_directory);
    let node = resolve_node_runtime(&working_directory, requirements.get("node"))
        .map_err(|error| error.detail)?;
    let path = prepend_path(&node.bin_dir)
        .ok_or_else(|| "Could not construct the Node.js execution PATH.".to_string())?;
    let executable = if program == "node" {
        node.executable
    } else {
        find_executable_with_path(program, Some(path.as_os_str())).ok_or_else(|| {
            format!(
                "{program} is not available with the Node.js {} environment resolved from {}.",
                node.version, node.manager
            )
        })?
    };

    Ok(ComponentExecution {
        program: executable,
        path: Some(path),
    })
}

fn resolve_node_runtime(
    _working_directory: &Path,
    requirement: Option<&Requirement>,
) -> Result<NodeResolution, NodeResolutionError> {
    select_node_candidate(collect_node_candidates(), requirement)
}

fn select_node_candidate(
    mut candidates: Vec<NodeCandidate>,
    requirement: Option<&Requirement>,
) -> Result<NodeResolution, NodeResolutionError> {
    if candidates.is_empty() {
        let detail = match requirement {
            Some(requirement) => format!(
                "Node.js {} is required by {}, but DECC could not find a usable Node installation on PATH or in fnm, nvm, Volta, asdf, or mise.",
                requirement.display, requirement.source
            ),
            None => "Node.js is not available on PATH and DECC could not find an installed Node version in fnm, nvm, Volta, asdf, or mise.".to_string(),
        };
        return Err(NodeResolutionError {
            status: RuntimeDiagnosticStatus::Missing,
            detail,
        });
    }

    if let Some(requirement) = requirement {
        if requirement.authoritative
            && matches!(requirement.constraint, VersionConstraint::Unsupported)
        {
            let detail = if requirement
                .display
                .starts_with("conflicting Dockerfile Node majors")
            {
                format!(
                    "DECC found {} in {}. Installed Node versions cannot tell DECC which Dockerfile represents local development intent. Add an explicit project selector such as .nvmrc, .node-version, package.json volta.node, or .tool-versions, or align the Dockerfile Node majors.",
                    requirement.display, requirement.source
                )
            } else {
                format!(
                    "DECC found the Node selection '{}' in {}, but it is not a numeric version DECC can resolve safely without invoking a version-manager shell hook. Use a numeric project version or make the intended Node executable available directly.",
                    requirement.display, requirement.source
                )
            };
            return Err(NodeResolutionError {
                status: RuntimeDiagnosticStatus::Unknown,
                detail,
            });
        }

        if !matches!(requirement.constraint, VersionConstraint::Unsupported) {
            let matching = candidates
                .iter()
                .filter(|candidate| {
                    requirement_matches(&candidate.version, &requirement.constraint) == Some(true)
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Some(candidate) =
                preferred_node_candidate_for_requirement(matching, &requirement.constraint)
            {
                return Ok(candidate.into());
            }

            let detected = describe_node_candidates(&candidates);
            return Err(NodeResolutionError {
                status: RuntimeDiagnosticStatus::Incompatible,
                detail: format!(
                    "Node.js {} is required by {}, but no installed Node version satisfies it. Detected: {detected}. DECC will not install or switch runtimes automatically.",
                    requirement.display, requirement.source
                ),
            });
        }
    }

    if let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.from_current_path)
        .cloned()
    {
        return Ok(candidate.into());
    }

    let unique_versions = candidates
        .iter()
        .map(|candidate| candidate.version.clone())
        .collect::<BTreeSet<_>>();
    if unique_versions.len() == 1 {
        candidates.sort_by(compare_node_candidates);
        return Ok(candidates.remove(0).into());
    }

    Err(NodeResolutionError {
        status: RuntimeDiagnosticStatus::Unknown,
        detail: format!(
            "DECC found multiple installed Node.js versions ({}) but this project does not select one and the app PATH does not expose a direct Node executable. Add a numeric .nvmrc/.node-version, package.json volta.node, or .tool-versions entry so DECC can choose deterministically.",
            describe_node_candidates(&candidates)
        ),
    })
}

impl From<NodeCandidate> for NodeResolution {
    fn from(candidate: NodeCandidate) -> Self {
        let bin_dir = candidate
            .executable
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        Self {
            executable: candidate.executable,
            version: candidate.version,
            manager: candidate.manager,
            bin_dir,
        }
    }
}

fn preferred_node_candidate_for_requirement(
    mut candidates: Vec<NodeCandidate>,
    requirement: &VersionConstraint,
) -> Option<NodeCandidate> {
    if let VersionConstraint::CompatibleMajor(minimum) = requirement {
        if minimum.len() <= 1 {
            candidates.sort_by(compare_node_candidates);
        } else {
            candidates.sort_by(|left, right| {
                parse_version_numbers(&left.version)
                    .cmp(&parse_version_numbers(&right.version))
                    .then_with(|| right.from_current_path.cmp(&left.from_current_path))
                    .then_with(|| left.manager.cmp(&right.manager))
            });
        }
        return candidates.into_iter().next();
    }

    preferred_node_candidate(candidates)
}

fn preferred_node_candidate(mut candidates: Vec<NodeCandidate>) -> Option<NodeCandidate> {
    if let Some(index) = candidates
        .iter()
        .position(|candidate| candidate.from_current_path)
    {
        return Some(candidates.remove(index));
    }
    candidates.sort_by(compare_node_candidates);
    candidates.into_iter().next()
}

fn compare_node_candidates(left: &NodeCandidate, right: &NodeCandidate) -> std::cmp::Ordering {
    parse_version_numbers(&right.version)
        .cmp(&parse_version_numbers(&left.version))
        .then_with(|| left.manager.cmp(&right.manager))
}

fn describe_node_candidates(candidates: &[NodeCandidate]) -> String {
    let values = candidates
        .iter()
        .map(|candidate| format!("{} ({})", candidate.version, candidate.manager))
        .collect::<BTreeSet<_>>();
    if values.is_empty() {
        "none".to_string()
    } else {
        values.into_iter().collect::<Vec<_>>().join(", ")
    }
}

fn collect_node_candidates() -> Vec<NodeCandidate> {
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::<PathBuf>::new();

    if let Some(path) = find_executable("node") {
        if !is_manager_dispatcher(&path) {
            let canonical = fs::canonicalize(&path).unwrap_or(path);
            if let Some(version) = probe_node_version(&canonical) {
                seen.insert(canonical.clone());
                candidates.push(NodeCandidate {
                    manager: manager_label_for_path(&canonical),
                    executable: canonical,
                    version,
                    from_current_path: true,
                });
            }
        }
    }

    for base in fnm_data_dirs() {
        scan_node_version_root(
            &base.join("node-versions"),
            "fnm",
            &[
                PathBuf::from("installation")
                    .join("bin")
                    .join(node_binary_name()),
                PathBuf::from("installation").join(node_binary_name()),
            ],
            &mut seen,
            &mut candidates,
        );
    }

    for base in nvm_data_dirs() {
        scan_node_version_root(
            &base.join("versions").join("node"),
            "nvm",
            &[
                PathBuf::from("bin").join(node_binary_name()),
                PathBuf::from(node_binary_name()),
            ],
            &mut seen,
            &mut candidates,
        );
        scan_node_version_root(
            &base,
            "nvm",
            &[
                PathBuf::from(node_binary_name()),
                PathBuf::from("bin").join(node_binary_name()),
            ],
            &mut seen,
            &mut candidates,
        );
    }

    for base in volta_data_dirs() {
        scan_node_version_root(
            &base.join("tools").join("image").join("node"),
            "Volta",
            &[
                PathBuf::from("bin").join(node_binary_name()),
                PathBuf::from(node_binary_name()),
            ],
            &mut seen,
            &mut candidates,
        );
    }

    for base in asdf_data_dirs() {
        scan_node_version_root(
            &base.join("installs").join("nodejs"),
            "asdf",
            &[
                PathBuf::from("bin").join(node_binary_name()),
                PathBuf::from(node_binary_name()),
            ],
            &mut seen,
            &mut candidates,
        );
    }

    for base in mise_data_dirs() {
        for tool in ["node", "nodejs"] {
            scan_node_version_root(
                &base.join("installs").join(tool),
                "mise",
                &[
                    PathBuf::from("bin").join(node_binary_name()),
                    PathBuf::from(node_binary_name()),
                ],
                &mut seen,
                &mut candidates,
            );
        }
    }

    candidates
}

fn scan_node_version_root(
    root: &Path,
    manager: &str,
    binary_relatives: &[PathBuf],
    seen: &mut BTreeSet<PathBuf>,
    candidates: &mut Vec<NodeCandidate>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(version) = normalize_node_version_label(&entry.file_name().to_string_lossy())
        else {
            continue;
        };

        for relative in binary_relatives {
            let executable = entry.path().join(relative);
            if !is_executable_file(&executable) {
                continue;
            }
            let canonical = fs::canonicalize(&executable).unwrap_or(executable);
            if !seen.insert(canonical.clone()) {
                break;
            }
            candidates.push(NodeCandidate {
                executable: canonical,
                version: version.clone(),
                manager: manager.to_string(),
                from_current_path: false,
            });
            break;
        }
    }
}

fn normalize_node_version_label(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches('v');
    if !starts_with_digit(value) {
        return None;
    }
    let numeric = value
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect::<String>();
    (!numeric.is_empty() && parse_version_numbers(&numeric).len() >= 2).then_some(numeric)
}

fn probe_node_version(executable: &Path) -> Option<String> {
    let output = Command::new(executable)
        .arg("--version")
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    extract_numeric_version(&version_output_text(&output.stdout, &output.stderr))
}

fn manager_label_for_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    if normalized.contains("/fnm/") || normalized.contains("fnm_multishell") {
        "fnm".to_string()
    } else if normalized.contains("/.nvm/") || normalized.contains("/nvm/versions/") {
        "nvm".to_string()
    } else if normalized.contains("/.volta/") || normalized.contains("/volta/tools/") {
        "Volta".to_string()
    } else if normalized.contains("/.asdf/") || normalized.contains("/asdf/installs/") {
        "asdf".to_string()
    } else if normalized.contains("/mise/") {
        "mise".to_string()
    } else {
        "PATH".to_string()
    }
}

fn is_manager_dispatcher(path: &Path) -> bool {
    manager_shim_dirs()
        .iter()
        .any(|directory| path.parent().is_some_and(|parent| parent == directory))
}

fn manager_shim_dirs() -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for base in volta_data_dirs() {
        paths.insert(base.join("bin"));
    }
    for base in asdf_data_dirs() {
        paths.insert(base.join("shims"));
    }
    for base in mise_data_dirs() {
        paths.insert(base.join("shims"));
    }
    paths.into_iter().collect()
}

fn fnm_data_dirs() -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    push_env_path(&mut paths, "FNM_DIR");
    if let Some(xdg) = env_path("XDG_DATA_HOME") {
        paths.insert(xdg.join("fnm"));
    }
    if let Some(home) = home_dir() {
        paths.insert(home.join(".local").join("share").join("fnm"));
        paths.insert(home.join("Library").join("Application Support").join("fnm"));
    }
    if let Some(app_data) = env_path("APPDATA") {
        paths.insert(app_data.join("fnm"));
    }
    if let Some(local_app_data) = env_path("LOCALAPPDATA") {
        paths.insert(local_app_data.join("fnm"));
    }
    paths.into_iter().collect()
}

fn nvm_data_dirs() -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    push_env_path(&mut paths, "NVM_DIR");
    push_env_path(&mut paths, "NVM_HOME");
    if let Some(xdg) = env_path("XDG_CONFIG_HOME") {
        paths.insert(xdg.join("nvm"));
    }
    if let Some(home) = home_dir() {
        paths.insert(home.join(".nvm"));
    }
    paths.into_iter().collect()
}

fn volta_data_dirs() -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    push_env_path(&mut paths, "VOLTA_HOME");
    if let Some(home) = home_dir() {
        paths.insert(home.join(".volta"));
    }
    paths.into_iter().collect()
}

fn asdf_data_dirs() -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    push_env_path(&mut paths, "ASDF_DATA_DIR");
    if let Some(home) = home_dir() {
        paths.insert(home.join(".asdf"));
    }
    paths.into_iter().collect()
}

fn mise_data_dirs() -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    push_env_path(&mut paths, "MISE_DATA_DIR");
    if let Some(xdg) = env_path("XDG_DATA_HOME") {
        paths.insert(xdg.join("mise"));
    }
    if let Some(home) = home_dir() {
        paths.insert(home.join(".local").join("share").join("mise"));
    }
    if let Some(local_app_data) = env_path("LOCALAPPDATA") {
        paths.insert(local_app_data.join("mise"));
    }
    paths.into_iter().collect()
}

fn push_env_path(paths: &mut BTreeSet<PathBuf>, key: &str) {
    if let Some(path) = env_path(key) {
        paths.insert(path);
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn home_dir() -> Option<PathBuf> {
    env_path("HOME")
        .or_else(|| env_path("USERPROFILE"))
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            let mut home = PathBuf::from(drive);
            home.push(path);
            Some(home)
        })
}

fn node_binary_name() -> &'static str {
    if cfg!(windows) {
        "node.exe"
    } else {
        "node"
    }
}

fn prepend_path(directory: &Path) -> Option<OsString> {
    let mut entries = vec![directory.to_path_buf()];
    if let Some(path) = augmented_execution_path() {
        for candidate in std::env::split_paths(&path) {
            if candidate != directory && !entries.contains(&candidate) {
                entries.push(candidate);
            }
        }
    }
    std::env::join_paths(entries).ok()
}

fn augmented_execution_path() -> Option<OsString> {
    let mut entries = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();

    for program in [
        "bun", "dotnet", "go", "docker", "make", "task", "just", "cargo", "rustc",
    ] {
        for directory in common_tool_directories(program) {
            if !entries.contains(&directory) {
                entries.push(directory);
            }
        }
    }

    std::env::join_paths(entries).ok()
}

fn find_executable(program: &str) -> Option<PathBuf> {
    find_executable_with_path(program, std::env::var_os("PATH").as_deref())
}

fn find_executable_with_path(program: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let direct = Path::new(program);
    if direct.is_absolute() || direct.components().count() > 1 {
        return is_executable_file(direct).then(|| direct.to_path_buf());
    }

    let mut directories = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    for directory in common_tool_directories(program) {
        if !directories.contains(&directory) {
            directories.push(directory);
        }
    }

    for directory in directories {
        let candidate = directory.join(program);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }

        #[cfg(windows)]
        if candidate.extension().is_none() {
            let extensions = std::env::var_os("PATHEXT")
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
            for extension in extensions.split(';').filter(|value| !value.is_empty()) {
                let extension = extension.trim_start_matches('.');
                let candidate = directory.join(format!("{program}.{extension}"));
                if is_executable_file(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn common_tool_directories(program: &str) -> Vec<PathBuf> {
    let mut directories = Vec::new();

    if let Some(home) = home_dir() {
        directories.push(home.join(".local").join("bin"));
        directories.push(home.join(".cargo").join("bin"));
        if matches!(program, "bun") {
            directories.push(home.join(".bun").join("bin"));
        }
        if matches!(program, "dotnet") {
            directories.push(home.join(".dotnet"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        directories.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ]);
        if program == "go" {
            directories.push(PathBuf::from("/usr/local/go/bin"));
        }
        if program == "dotnet" {
            directories.extend([
                PathBuf::from("/usr/local/share/dotnet"),
                PathBuf::from("/opt/homebrew/share/dotnet"),
            ]);
        }
        if program == "docker" {
            directories.push(PathBuf::from(
                "/Applications/Docker.app/Contents/Resources/bin",
            ));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        directories.extend([
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            PathBuf::from("/snap/bin"),
        ]);
        if program == "go" {
            directories.push(PathBuf::from("/usr/local/go/bin"));
        }
        if program == "dotnet" {
            directories.extend([
                PathBuf::from("/usr/share/dotnet"),
                PathBuf::from("/usr/local/share/dotnet"),
            ]);
        }
    }

    #[cfg(windows)]
    {
        if let Some(program_files) = env_path("ProgramFiles") {
            if program == "dotnet" {
                directories.push(program_files.join("dotnet"));
            }
            if program == "go" {
                directories.push(program_files.join("Go").join("bin"));
            }
            if program == "docker" {
                directories.push(
                    program_files
                        .join("Docker")
                        .join("Docker")
                        .join("resources")
                        .join("bin"),
                );
            }
        }
        if let Some(local_app_data) = env_path("LOCALAPPDATA") {
            directories.push(
                local_app_data
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links"),
            );
        }
        if let Some(home) = home_dir() {
            directories.push(home.join("scoop").join("shims"));
        }
        if let Some(chocolatey) = env_path("ChocolateyInstall") {
            directories.push(chocolatey.join("bin"));
        }
    }

    directories
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn component_working_directory(
    workspace: &PersistedWorkspace,
    component: &PersistedComponent,
) -> Result<PathBuf, String> {
    let root = fs::canonicalize(&workspace.path)
        .map_err(|error| format!("Workspace folder is unavailable: {error}"))?;
    let candidate = if component.relative_path == "." {
        root.clone()
    } else {
        root.join(&component.relative_path)
    };
    let working = fs::canonicalize(candidate)
        .map_err(|error| format!("Component working directory is unavailable: {error}"))?;
    if !working.starts_with(&root) {
        return Err("Component working directory resolves outside the workspace.".to_string());
    }
    Ok(working)
}

fn parse_requirement(value: &str) -> VersionConstraint {
    let value = value.trim();
    if value.is_empty() || value == "*" {
        return VersionConstraint::Unsupported;
    }
    if value.contains("||")
        || value.contains('<')
        || (value.contains(' ') && !value.starts_with(">="))
        || value.contains(" - ")
    {
        return VersionConstraint::Unsupported;
    }

    if let Some(version) = value.strip_prefix(">=") {
        let numbers = parse_version_numbers(version.trim());
        return if numbers.is_empty() {
            VersionConstraint::Unsupported
        } else {
            VersionConstraint::Minimum(numbers)
        };
    }
    if let Some(version) = value.strip_prefix('^') {
        let numbers = parse_version_numbers(version.trim());
        return if numbers.is_empty() {
            VersionConstraint::Unsupported
        } else {
            VersionConstraint::Caret(numbers)
        };
    }
    if let Some(version) = value.strip_prefix('~') {
        let numbers = parse_version_numbers(version.trim());
        return if numbers.is_empty() {
            VersionConstraint::Unsupported
        } else {
            VersionConstraint::Tilde(numbers)
        };
    }

    let wildcard = value
        .split('.')
        .position(|part| matches!(part.trim(), "x" | "X" | "*"));
    if let Some(index) = wildcard {
        let prefix = value
            .split('.')
            .take(index)
            .filter_map(|part| part.trim().parse::<u64>().ok())
            .collect::<Vec<_>>();
        return if prefix.is_empty() {
            VersionConstraint::Unsupported
        } else {
            VersionConstraint::Prefix(prefix)
        };
    }

    let numbers = parse_version_numbers(value);
    if numbers.is_empty() {
        VersionConstraint::Unsupported
    } else if numbers.len() < 3 {
        VersionConstraint::Prefix(numbers)
    } else {
        VersionConstraint::Exact(numbers)
    }
}

fn requirement_matches(installed: &str, requirement: &VersionConstraint) -> Option<bool> {
    let installed = parse_version_numbers(installed);
    if installed.is_empty() {
        return None;
    }

    match requirement {
        VersionConstraint::Minimum(minimum) => Some(compare_versions(&installed, minimum) >= 0),
        VersionConstraint::Exact(exact) => Some(compare_versions(&installed, exact) == 0),
        VersionConstraint::Prefix(prefix) => Some(version_starts_with(&installed, prefix)),
        VersionConstraint::CompatibleMajor(minimum) => {
            let major = *minimum.first()?;
            Some(
                installed.first().copied() == Some(major)
                    && compare_versions(&installed, minimum) >= 0,
            )
        }
        VersionConstraint::Caret(minimum) => {
            let major = *minimum.first()?;
            let lower_ok = compare_versions(&installed, minimum) >= 0;
            let upper_ok = if major > 0 {
                installed.first().copied() == Some(major)
            } else {
                let minor = minimum.get(1).copied().unwrap_or(0);
                if minor > 0 {
                    installed.first().copied() == Some(0)
                        && installed.get(1).copied() == Some(minor)
                } else {
                    installed.first().copied() == Some(0)
                        && installed.get(1).copied().unwrap_or(0) == 0
                        && installed.get(2).copied().unwrap_or(0)
                            == minimum.get(2).copied().unwrap_or(0)
                }
            };
            Some(lower_ok && upper_ok)
        }
        VersionConstraint::Tilde(minimum) => {
            let major = *minimum.first()?;
            let minor = minimum.get(1).copied();
            Some(
                compare_versions(&installed, minimum) >= 0
                    && installed.first().copied() == Some(major)
                    && minor.is_none_or(|minor| installed.get(1).copied() == Some(minor)),
            )
        }
        VersionConstraint::Unsupported => None,
    }
}

fn compare_versions(left: &[u64], right: &[u64]) -> i8 {
    let length = left.len().max(right.len());
    for index in 0..length {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        if left < right {
            return -1;
        }
        if left > right {
            return 1;
        }
    }
    0
}

fn version_starts_with(version: &[u64], prefix: &[u64]) -> bool {
    prefix
        .iter()
        .enumerate()
        .all(|(index, value)| version.get(index) == Some(value))
}

fn extract_numeric_version(text: &str) -> Option<String> {
    for (index, character) in text.char_indices() {
        if !character.is_ascii_digit() {
            continue;
        }
        let candidate = text[index..]
            .chars()
            .take_while(|character| character.is_ascii_digit() || *character == '.')
            .collect::<String>();
        let candidate = candidate.trim_end_matches('.');
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn parse_version_numbers(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches('v')
        .trim_start_matches("go")
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .take_while(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn version_output_text(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => String::new(),
    }
}

fn single_line(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    const LIMIT: usize = 240;
    if value.chars().count() <= LIMIT {
        value
    } else {
        format!("{}…", value.chars().take(LIMIT).collect::<String>())
    }
}

fn starts_with_digit(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::models::{
        detection::DetectionConfidence,
        runtime::RuntimeDiagnosticStatus,
        workspace::{PersistedComponent, PersistedWorkspace},
    };

    use super::{
        derive_requirements, dotnet_target_version, extract_numeric_version, inspect_component,
        inspect_tool, node_binary_name, node_pin_from_directory, parse_node_pin, parse_requirement,
        requirement_matches, scan_node_version_root, select_node_candidate, NodeCandidate,
        Requirement, ToolSpec,
    };

    fn fixture_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-runtime-{name}-{unique}"));
        fs::create_dir_all(&root).expect("root");
        root
    }

    fn workspace(root: &Path, program: &str, runtime: &str) -> PersistedWorkspace {
        PersistedWorkspace {
            id: "workspace".to_string(),
            name: "Workspace".to_string(),
            path: root.to_string_lossy().into_owned(),
            trusted: true,
            components: vec![PersistedComponent {
                id: "component".to_string(),
                name: "Component".to_string(),
                relative_path: ".".to_string(),
                technology: runtime.to_string(),
                runtime: runtime.to_string(),
                command: format!("{program} run"),
                program: Some(program.to_string()),
                args: vec![],
                expected_ports: vec![],
                include_in_run_all: true,
                detection_confidence: DetectionConfidence::High,
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn parses_common_version_requirements_conservatively() {
        assert_eq!(
            requirement_matches("24.1.0", &parse_requirement(">=22")),
            Some(true)
        );
        assert_eq!(
            requirement_matches("20.5.0", &parse_requirement(">=22")),
            Some(false)
        );
        assert_eq!(
            requirement_matches("22.5.0", &parse_requirement("^22.1")),
            Some(true)
        );
        assert_eq!(
            requirement_matches("23.0.0", &parse_requirement("^22.1")),
            Some(false)
        );
        assert_eq!(
            requirement_matches("22.1.9", &parse_requirement("22.1.x")),
            Some(true)
        );
        assert_eq!(
            requirement_matches("22.2.0", &parse_requirement("22.1.x")),
            Some(false)
        );
        assert_eq!(
            requirement_matches("22.1.0", &parse_requirement(">=20 <23")),
            None
        );
        assert_eq!(
            requirement_matches("22.9.0", &parse_requirement("22")),
            Some(true)
        );
        assert_eq!(
            requirement_matches("23.0.0", &parse_requirement("22")),
            Some(false)
        );
    }

    #[test]
    fn derives_manifest_requirements_without_executing_project_commands() {
        let root = fixture_root("requirements");
        fs::write(
            root.join("package.json"),
            r#"{"engines":{"node":">=22","pnpm":"^9.0.0"}}"#,
        )
        .expect("package");
        let workspace = workspace(&root, "pnpm", "Node.js");
        let requirements = derive_requirements(&workspace, &workspace.components[0], &root);
        assert_eq!(
            requirements.get("node").map(|item| item.display.as_str()),
            Some(">=22")
        );
        assert_eq!(
            requirements.get("pnpm").map(|item| item.display.as_str()),
            Some("^9.0.0")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dockerfile_node_image_selects_major_when_no_stronger_project_hint_exists() {
        let root = fixture_root("docker-node");
        fs::write(
            root.join("package.json"),
            r#"{"scripts":{"dev":"next dev"}}"#,
        )
        .expect("package");
        fs::write(
            root.join("Dockerfile"),
            "FROM node:16.15.1 AS base\nRUN echo build\n",
        )
        .expect("dockerfile");
        fs::write(
            root.join("Dockerfile.onprem"),
            "FROM node:16.15.1-alpine AS builder\n",
        )
        .expect("dockerfile onprem");

        let workspace = workspace(&root, "yarn", "Node.js");
        let requirements = derive_requirements(&workspace, &workspace.components[0], &root);
        let node = requirements.get("node").expect("docker node requirement");
        assert_eq!(node.display, ">= 16.15.1 < 17");
        assert!(node.source.contains("Dockerfile"));
        assert_eq!(requirement_matches("16.20.2", &node.constraint), Some(true));
        assert_eq!(requirement_matches("16.21.0", &node.constraint), Some(true));
        assert_eq!(
            requirement_matches("16.14.9", &node.constraint),
            Some(false)
        );
        assert_eq!(
            requirement_matches("24.21.0", &node.constraint),
            Some(false)
        );

        let selected = select_node_candidate(
            vec![
                NodeCandidate {
                    executable: PathBuf::from("/fnm/node-24"),
                    version: "24.21.0".to_string(),
                    manager: "fnm".to_string(),
                    from_current_path: true,
                },
                NodeCandidate {
                    executable: PathBuf::from("/fnm/node-16"),
                    version: "16.20.2".to_string(),
                    manager: "fnm".to_string(),
                    from_current_path: false,
                },
            ],
            Some(node),
        )
        .expect("node 16 selection");
        assert_eq!(selected.version, "16.20.2");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn conflicting_dockerfile_node_majors_do_not_fall_back_to_machine_default() {
        let root = fixture_root("docker-node-conflict");
        fs::write(
            root.join("package.json"),
            r#"{"scripts":{"dev":"next dev"}}"#,
        )
        .expect("package");
        fs::write(root.join("Dockerfile"), "FROM node:16-alpine\n").expect("dockerfile");
        fs::write(root.join("Dockerfile.dev"), "FROM node:18-alpine\n").expect("dockerfile dev");

        let workspace = workspace(&root, "npm", "Node.js");
        let requirements = derive_requirements(&workspace, &workspace.components[0], &root);
        let node = requirements
            .get("node")
            .expect("conflicting node requirement");
        assert!(node.authoritative);
        assert!(matches!(
            node.constraint,
            super::VersionConstraint::Unsupported
        ));

        let result = select_node_candidate(
            vec![NodeCandidate {
                executable: PathBuf::from("/system/node-24"),
                version: "24.21.0".to_string(),
                manager: "PATH".to_string(),
                from_current_path: true,
            }],
            Some(node),
        );
        assert!(result.is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn more_specific_dockerfile_node_pin_wins_over_conflicting_major_only_hint() {
        let root = fixture_root("docker-node-specificity");
        fs::write(
            root.join("package.json"),
            r#"{"scripts":{"dev":"next dev"}}"#,
        )
        .expect("package");
        fs::write(root.join("Dockerfile"), "FROM node:24-alpine\n").expect("dockerfile");
        fs::write(
            root.join("Dockerfile.onprem"),
            "FROM node:16.15.1 as builder\nFROM node:16.15.1\n",
        )
        .expect("dockerfile onprem");

        let workspace = workspace(&root, "yarn", "Node.js");
        let requirements = derive_requirements(&workspace, &workspace.components[0], &root);
        let node = requirements.get("node").expect("node requirement");
        assert!(!node.authoritative);
        assert!(matches!(
            node.constraint,
            super::VersionConstraint::CompatibleMajor(_)
        ));
        assert_eq!(node.display, ">= 16.15.1 < 17");
        assert!(node.source.contains("Dockerfile.onprem"));

        let selected = select_node_candidate(
            vec![
                NodeCandidate {
                    executable: PathBuf::from("/fnm/node-24"),
                    version: "24.21.0".to_string(),
                    manager: "fnm".to_string(),
                    from_current_path: true,
                },
                NodeCandidate {
                    executable: PathBuf::from("/fnm/node-16"),
                    version: "16.20.2".to_string(),
                    manager: "fnm".to_string(),
                    from_current_path: false,
                },
            ],
            Some(node),
        )
        .expect("node 16 selection");
        assert_eq!(selected.version, "16.20.2");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn node_project_pin_overrides_engine_range() {
        let root = fixture_root("node-pin");
        fs::write(root.join("package.json"), r#"{"engines":{"node":">=18"}}"#).expect("package");
        fs::write(root.join(".nvmrc"), "v20.11.1\n").expect("nvmrc");

        let workspace = workspace(&root, "pnpm", "Node.js");
        let requirements = derive_requirements(&workspace, &workspace.components[0], &root);
        let node = requirements.get("node").expect("node requirement");
        assert_eq!(node.display, "v20.11.1");
        assert_eq!(node.source, ".nvmrc");
        assert!(node.authoritative);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recognizes_node_version_volta_and_tool_versions_pins() {
        let root = fixture_root("node-pin-files");

        let node_version = root.join("node-version");
        fs::create_dir_all(&node_version).expect("node-version dir");
        fs::write(node_version.join(".node-version"), "22.4.0\n").expect("node-version");
        assert_eq!(
            node_pin_from_directory(&root, &node_version)
                .map(|item| item.0)
                .as_deref(),
            Some("22.4.0")
        );

        let volta = root.join("volta");
        fs::create_dir_all(&volta).expect("volta dir");
        fs::write(volta.join("package.json"), r#"{"volta":{"node":"21.7.3"}}"#)
            .expect("volta package");
        assert_eq!(
            node_pin_from_directory(&root, &volta)
                .map(|item| item.0)
                .as_deref(),
            Some("21.7.3")
        );

        let mise = root.join("mise");
        fs::create_dir_all(&mise).expect("mise dir");
        fs::write(mise.join("mise.toml"), "[tools]\nnode = \"23.6.1\"\n").expect("mise config");
        assert_eq!(
            node_pin_from_directory(&root, &mise)
                .map(|item| item.0)
                .as_deref(),
            Some("23.6.1")
        );

        let tool_versions = root.join("tool-versions");
        fs::create_dir_all(&tool_versions).expect("tool-versions dir");
        fs::write(
            tool_versions.join(".tool-versions"),
            "python 3.13.1\nnodejs 24.2.0\n",
        )
        .expect("tool versions");
        assert_eq!(
            node_pin_from_directory(&root, &tool_versions)
                .map(|item| item.0)
                .as_deref(),
            Some("24.2.0")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn node_selection_prefers_project_compatible_manager_over_default_path_node() {
        let requirement = Requirement {
            display: "20".to_string(),
            source: ".nvmrc".to_string(),
            constraint: parse_node_pin("20"),
            authoritative: true,
        };
        let candidates = vec![
            NodeCandidate {
                executable: PathBuf::from("/system/node-24"),
                version: "24.1.0".to_string(),
                manager: "PATH".to_string(),
                from_current_path: true,
            },
            NodeCandidate {
                executable: PathBuf::from("/fnm/node-20"),
                version: "20.18.1".to_string(),
                manager: "fnm".to_string(),
                from_current_path: false,
            },
        ];

        let selected = select_node_candidate(candidates, Some(&requirement)).expect("selection");
        assert_eq!(selected.version, "20.18.1");
        assert_eq!(selected.manager, "fnm");
    }

    #[test]
    fn numeric_node_pin_accepts_newer_compatible_minor_and_prefers_closest() {
        let requirement = Requirement {
            display: "16.20".to_string(),
            source: ".nvmrc".to_string(),
            constraint: parse_node_pin("16.20"),
            authoritative: true,
        };
        let candidates = vec![
            NodeCandidate {
                executable: PathBuf::from("/fnm/node-16.19"),
                version: "16.19.1".to_string(),
                manager: "fnm".to_string(),
                from_current_path: false,
            },
            NodeCandidate {
                executable: PathBuf::from("/fnm/node-16.20"),
                version: "16.20.2".to_string(),
                manager: "fnm".to_string(),
                from_current_path: false,
            },
            NodeCandidate {
                executable: PathBuf::from("/system/node-16.21"),
                version: "16.21.0".to_string(),
                manager: "PATH".to_string(),
                from_current_path: true,
            },
            NodeCandidate {
                executable: PathBuf::from("/fnm/node-17"),
                version: "17.0.0".to_string(),
                manager: "fnm".to_string(),
                from_current_path: false,
            },
        ];

        let selected = select_node_candidate(candidates, Some(&requirement)).expect("selection");
        assert_eq!(selected.version, "16.20.2");
        assert_eq!(
            requirement_matches("16.21.0", &requirement.constraint),
            Some(true)
        );
        assert_eq!(
            requirement_matches("16.19.1", &requirement.constraint),
            Some(false)
        );
        assert_eq!(
            requirement_matches("17.0.0", &requirement.constraint),
            Some(false)
        );
    }

    #[test]
    fn scans_versioned_node_install_directory() {
        let root = fixture_root("node-manager-layout");
        let version_root = root.join("node-versions");
        let executable = version_root
            .join("v22.14.0")
            .join("installation")
            .join("bin")
            .join(node_binary_name());
        fs::create_dir_all(executable.parent().expect("parent")).expect("bin dir");
        fs::write(&executable, b"").expect("binary placeholder");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&executable, permissions).expect("permissions");
        }

        let mut seen = std::collections::BTreeSet::new();
        let mut candidates = Vec::new();
        scan_node_version_root(
            &version_root,
            "fnm",
            &[PathBuf::from("installation")
                .join("bin")
                .join(node_binary_name())],
            &mut seen,
            &mut candidates,
        );

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].version, "22.14.0");
        assert_eq!(candidates[0].manager, "fnm");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_make_run_requires_rescan_when_makefile_declares_apps() {
        let root = fixture_root("make-shape");
        fs::write(
            root.join("Makefile"),
            "VALID_APPS := api worker\nrun: require-app\n\t@echo $(APP)\n",
        )
        .expect("makefile");
        let mut workspace = workspace(&root, "make", "Project command");
        workspace.components[0].args = vec!["run".to_string()];

        let diagnostic = super::inspect_make_command_shape(&workspace.components[0], &root)
            .expect("shape diagnostic");
        assert!(diagnostic.blocking);
        assert!(diagnostic.detail.contains("api, worker"));
        assert!(diagnostic.detail.contains("Rescan"));

        workspace.components[0].args = vec!["run".to_string(), "api".to_string()];
        assert!(super::inspect_make_command_shape(&workspace.components[0], &root).is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_workspace_path_blocks_runtime_readiness() {
        let root = fixture_root("missing-workspace");
        let workspace = workspace(&root, "node", "Node.js");
        fs::remove_dir_all(&root).expect("remove workspace");

        let inspection = inspect_component(&workspace, &workspace.components[0]);

        assert!(!inspection.ready);
        assert!(inspection
            .blocking_message
            .as_deref()
            .is_some_and(|message| message.contains("Workspace folder is unavailable")));
        assert!(inspection
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.key == "workspace-path" && diagnostic.blocking }));
    }

    #[test]
    fn missing_tool_is_blocking_without_executing_it() {
        let root = fixture_root("missing");
        let diagnostic = inspect_tool(ToolSpec {
            key: "missing".to_string(),
            label: "Missing Tool".to_string(),
            program: "decc-definitely-missing-tool-9f0c".to_string(),
            version_args: vec!["--version".to_string()],
            probe_version: true,
            requirement: None,
            search_path: None,
        });

        assert_eq!(diagnostic.status, RuntimeDiagnosticStatus::Missing);
        assert!(diagnostic.blocking);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_component_program_is_never_version_probed() {
        let root = fixture_root("unknown");
        let workspace = workspace(&root, "decc-definitely-missing-tool", "Fixture");
        let inspection = inspect_component(&workspace, &workspace.components[0]);
        assert!(inspection.ready);
        assert!(inspection
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.status == RuntimeDiagnosticStatus::Unknown));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_versions_and_dotnet_targets() {
        assert_eq!(
            extract_numeric_version("go version go1.24.3 darwin/arm64").as_deref(),
            Some("1.24.3")
        );
        assert_eq!(
            extract_numeric_version("Docker version 27.2.0, build abc").as_deref(),
            Some("27.2.0")
        );
        assert_eq!(dotnet_target_version("net8.0").as_deref(), Some("8.0"));
        assert_eq!(
            dotnet_target_version("netcoreapp3.1").as_deref(),
            Some("3.1")
        );
        assert_eq!(dotnet_target_version("net48"), None);
    }
}
