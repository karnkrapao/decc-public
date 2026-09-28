use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::models::{
    environment::{
        EnvironmentFileKind, EnvironmentFileMetadata, EnvironmentVariableMetadata,
        EnvironmentVariableStatus, WorkspaceEnvironmentInspection,
    },
    workspace::{PersistedComponent, PersistedWorkspace},
};

const MAX_ENV_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone)]
struct Scope {
    component_id: Option<String>,
    component_name: Option<String>,
    directory: PathBuf,
}

#[derive(Debug, Default)]
struct VariableSources {
    present: BTreeSet<String>,
    required: BTreeSet<String>,
}

pub fn inspect(workspaces: &[PersistedWorkspace]) -> Vec<WorkspaceEnvironmentInspection> {
    workspaces.iter().map(inspect_workspace).collect()
}

fn inspect_workspace(workspace: &PersistedWorkspace) -> WorkspaceEnvironmentInspection {
    match inspect_workspace_inner(workspace) {
        Ok((files, variables)) => WorkspaceEnvironmentInspection {
            workspace_id: workspace.id.clone(),
            files,
            variables,
            error: None,
        },
        Err(error) => WorkspaceEnvironmentInspection {
            workspace_id: workspace.id.clone(),
            files: Vec::new(),
            variables: Vec::new(),
            error: Some(error),
        },
    }
}

fn inspect_workspace_inner(
    workspace: &PersistedWorkspace,
) -> Result<
    (
        Vec<EnvironmentFileMetadata>,
        Vec<EnvironmentVariableMetadata>,
    ),
    String,
> {
    let root = fs::canonicalize(&workspace.path)
        .map_err(|error| format!("Workspace folder is unavailable: {error}"))?;
    if !root.is_dir() {
        return Err("Workspace path is not a directory.".to_string());
    }

    let scopes = build_scopes(workspace, &root);
    let mut files = Vec::new();
    let mut variables = Vec::new();

    for scope in scopes {
        let (scope_files, scope_variables) = inspect_scope(&root, &scope)?;
        files.extend(scope_files);
        variables.extend(scope_variables);
    }

    files.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then_with(|| left.component_name.cmp(&right.component_name))
    });
    variables.sort_by(|left, right| {
        left.component_name
            .cmp(&right.component_name)
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok((files, variables))
}

fn build_scopes(workspace: &PersistedWorkspace, root: &Path) -> Vec<Scope> {
    let root_components = workspace
        .components
        .iter()
        .filter(|component| is_root_component(component))
        .collect::<Vec<_>>();

    let mut scopes = Vec::new();
    if root_components.len() == 1 {
        let component = root_components[0];
        scopes.push(Scope {
            component_id: Some(component.id.clone()),
            component_name: Some(component.name.clone()),
            directory: root.to_path_buf(),
        });
    } else {
        scopes.push(Scope {
            component_id: None,
            component_name: None,
            directory: root.to_path_buf(),
        });
    }

    for component in workspace
        .components
        .iter()
        .filter(|component| !is_root_component(component))
    {
        let candidate = root.join(&component.relative_path);
        let Ok(directory) = fs::canonicalize(candidate) else {
            continue;
        };
        if !directory.starts_with(root) || !directory.is_dir() {
            continue;
        }
        scopes.push(Scope {
            component_id: Some(component.id.clone()),
            component_name: Some(component.name.clone()),
            directory,
        });
    }

    scopes
}

fn is_root_component(component: &PersistedComponent) -> bool {
    component.relative_path.is_empty() || component.relative_path == "."
}

fn inspect_scope(
    root: &Path,
    scope: &Scope,
) -> Result<
    (
        Vec<EnvironmentFileMetadata>,
        Vec<EnvironmentVariableMetadata>,
    ),
    String,
> {
    let entries = fs::read_dir(&scope.directory).map_err(|error| {
        format!(
            "Could not inspect environment files in {}: {error}",
            display_relative(root, &scope.directory)
        )
    })?;

    let mut files = Vec::new();
    let mut variable_sources = BTreeMap::<String, VariableSources>::new();

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(kind) = classify_environment_file(&name) else {
            continue;
        };

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let Ok(canonical) = fs::canonicalize(&path) else {
            continue;
        };
        if !canonical.starts_with(root) {
            continue;
        }
        let Ok(metadata) = fs::metadata(&canonical) else {
            continue;
        };
        if metadata.len() > MAX_ENV_FILE_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(&canonical) else {
            continue;
        };

        let relative_path = display_relative(root, &canonical);
        files.push(EnvironmentFileMetadata {
            component_id: scope.component_id.clone(),
            component_name: scope.component_name.clone(),
            name,
            relative_path: relative_path.clone(),
            kind: kind.clone(),
        });

        for variable in parse_variable_names(&text) {
            let sources = variable_sources.entry(variable).or_default();
            match kind {
                EnvironmentFileKind::Example => {
                    sources.required.insert(relative_path.clone());
                }
                EnvironmentFileKind::Active | EnvironmentFileKind::Override => {
                    sources.present.insert(relative_path.clone());
                }
            }
        }
    }

    let variables = variable_sources
        .into_iter()
        .map(|(name, sources)| {
            let status = if sources.present.is_empty() {
                EnvironmentVariableStatus::Missing
            } else {
                EnvironmentVariableStatus::Present
            };
            EnvironmentVariableMetadata {
                component_id: scope.component_id.clone(),
                component_name: scope.component_name.clone(),
                name,
                status,
                source_paths: sources.present.into_iter().collect(),
                requirement_paths: sources.required.into_iter().collect(),
            }
        })
        .collect();

    Ok((files, variables))
}

fn classify_environment_file(name: &str) -> Option<EnvironmentFileKind> {
    if name != ".env" && !name.starts_with(".env.") {
        return None;
    }
    if name.contains(".example") || name.contains(".sample") || name.contains(".template") {
        return Some(EnvironmentFileKind::Example);
    }
    if name.ends_with(".local") {
        return Some(EnvironmentFileKind::Override);
    }
    Some(EnvironmentFileKind::Active)
}

fn parse_variable_names(text: &str) -> BTreeSet<String> {
    text.lines().filter_map(parse_variable_name).collect()
}

fn parse_variable_name(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    let (name, _) = line.split_once('=')?;
    let name = name.trim();
    is_valid_variable_name(name).then(|| name.to_string())
}

fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
        environment::{EnvironmentFileKind, EnvironmentVariableStatus},
        workspace::{PersistedComponent, PersistedWorkspace},
    };

    use super::{inspect, MAX_ENV_FILE_BYTES};

    fn fixture_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-environment-{name}-{unique}"));
        fs::create_dir_all(&root).expect("root");
        root
    }

    fn component(id: &str, name: &str, relative_path: &str) -> PersistedComponent {
        PersistedComponent {
            id: id.to_string(),
            name: name.to_string(),
            relative_path: relative_path.to_string(),
            technology: "Fixture".to_string(),
            runtime: "Fixture".to_string(),
            command: "fixture".to_string(),
            program: Some("fixture".to_string()),
            args: vec![],
            expected_ports: vec![],
            include_in_run_all: true,
            detection_confidence: DetectionConfidence::High,
        }
    }

    fn workspace(root: &Path, components: Vec<PersistedComponent>) -> PersistedWorkspace {
        PersistedWorkspace {
            id: "workspace".to_string(),
            name: "Workspace".to_string(),
            path: root.to_string_lossy().into_owned(),
            trusted: true,
            components,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn returns_names_and_presence_without_serializing_secret_values() {
        let root = fixture_root("secret-safe");
        fs::write(
            root.join(".env.example"),
            "DATABASE_URL=placeholder\nSECRET_TOKEN=placeholder\n",
        )
        .expect("example");
        fs::write(
            root.join(".env.local"),
            "export DATABASE_URL=postgres://private\nSECRET_TOKEN=super-secret-value\n",
        )
        .expect("active");

        let result = inspect(&[workspace(&root, vec![component("app", "App", ".")])]);
        let inspection = &result[0];
        assert!(inspection.error.is_none());
        assert_eq!(inspection.files.len(), 2);
        assert!(inspection
            .files
            .iter()
            .any(|file| file.kind == EnvironmentFileKind::Example));
        assert!(inspection
            .variables
            .iter()
            .all(|variable| { variable.status == EnvironmentVariableStatus::Present }));

        let serialized = serde_json::to_string(inspection).expect("serialize");
        assert!(serialized.contains("DATABASE_URL"));
        assert!(serialized.contains("SECRET_TOKEN"));
        assert!(!serialized.contains("postgres://private"));
        assert!(!serialized.contains("super-secret-value"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_same_variable_name_separate_across_monorepo_components() {
        let root = fixture_root("monorepo");
        fs::create_dir_all(root.join("apps/web")).expect("web");
        fs::create_dir_all(root.join("services/api")).expect("api");
        fs::write(root.join("apps/web/.env.example"), "API_URL=example\n").expect("web env");
        fs::write(root.join("services/api/.env.example"), "API_URL=example\n").expect("api env");
        fs::write(root.join("apps/web/.env.local"), "API_URL=web-private\n").expect("web local");

        let result = inspect(&[workspace(
            &root,
            vec![
                component("web", "Web", "apps/web"),
                component("api", "API", "services/api"),
            ],
        )]);
        let inspection = &result[0];
        let matching = inspection
            .variables
            .iter()
            .filter(|variable| variable.name == "API_URL")
            .collect::<Vec<_>>();

        assert_eq!(matching.len(), 2);
        assert!(matching.iter().any(|variable| {
            variable.component_id.as_deref() == Some("web")
                && variable.status == EnvironmentVariableStatus::Present
        }));
        assert!(matching.iter().any(|variable| {
            variable.component_id.as_deref() == Some("api")
                && variable.status == EnvironmentVariableStatus::Missing
        }));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skips_oversized_environment_files() {
        let root = fixture_root("oversized");
        let oversized = vec![b'x'; (MAX_ENV_FILE_BYTES + 1) as usize];
        fs::write(root.join(".env"), oversized).expect("oversized env");

        let result = inspect(&[workspace(&root, vec![])]);
        assert!(result[0].files.is_empty());
        assert!(result[0].variables.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn skips_environment_file_symlinks() {
        use std::os::unix::fs::symlink;

        let root = fixture_root("symlink");
        let outside = fixture_root("outside-secret").join("secret.env");
        fs::write(&outside, "LEAKED_SECRET=do-not-read\n").expect("outside secret");
        symlink(&outside, root.join(".env")).expect("env symlink");

        let result = inspect(&[workspace(&root, vec![])]);
        assert!(result[0].files.is_empty());
        assert!(result[0].variables.is_empty());
        assert!(!serde_json::to_string(&result[0])
            .expect("serialize")
            .contains("LEAKED_SECRET"));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside.parent().expect("outside parent"));
    }
}
