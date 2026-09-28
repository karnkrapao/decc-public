use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::models::detection::WorkspaceScanResult;

use super::detectors::{self, DirectorySnapshot};

const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "coverage",
    "bin",
    "obj",
    ".cache",
    ".turbo",
    ".vite",
    ".parcel-cache",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".venv",
    "venv",
];

const SCAN_CANCELLED: &str = "Workspace scan cancelled.";

pub fn scan_workspace(root: &Path) -> Result<WorkspaceScanResult, String> {
    let cancelled = AtomicBool::new(false);
    scan_workspace_with_cancel(root, &cancelled)
}

pub fn scan_workspace_with_cancel(
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<WorkspaceScanResult, String> {
    check_cancelled(cancelled)?;
    if !root.exists() {
        return Err("The selected workspace folder no longer exists.".to_string());
    }
    if !root.is_dir() {
        return Err("The selected workspace path is not a folder.".to_string());
    }

    let root = fs::canonicalize(root)
        .map_err(|error| format!("Could not resolve workspace folder: {error}"))?;

    let mut directories = Vec::new();
    collect_directories(&root, &root, &mut directories, cancelled)?;

    let mut candidates = Vec::new();
    for directory in &directories {
        check_cancelled(cancelled)?;
        candidates.extend(detectors::detect(&root, directory));
    }
    check_cancelled(cancelled)?;

    candidates.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.technology.cmp(&b.technology))
    });
    candidates.dedup_by(|a, b| a.id == b.id);

    Ok(WorkspaceScanResult {
        root_path: root.to_string_lossy().into_owned(),
        scanned_directories: directories.len(),
        candidates,
    })
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err(SCAN_CANCELLED.to_string())
    } else {
        Ok(())
    }
}

fn collect_directories(
    root: &Path,
    current: &Path,
    output: &mut Vec<DirectorySnapshot>,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    check_cancelled(cancelled)?;
    let mut files = HashSet::new();
    let mut child_directories: Vec<PathBuf> = Vec::new();

    let entries = fs::read_dir(current)
        .map_err(|error| format!("Could not read {}: {error}", current.display()))?;

    for entry in entries {
        check_cancelled(cancelled)?;
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();

        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            if !should_ignore_directory(&name) {
                child_directories.push(entry.path());
            }
        } else if file_type.is_file() {
            files.insert(name);
        }
    }

    check_cancelled(cancelled)?;
    let relative_path = relative_display_path(root, current);
    output.push(DirectorySnapshot {
        absolute_path: current.to_path_buf(),
        relative_path,
        files,
    });

    child_directories.sort();
    for child in child_directories {
        check_cancelled(cancelled)?;
        // A single unreadable nested folder should not make the entire workspace
        // undiscoverable. Cancellation is different: it must stop the traversal.
        if let Err(error) = collect_directories(root, &child, output, cancelled) {
            if error == SCAN_CANCELLED {
                return Err(error);
            }
        }
    }

    Ok(())
}

fn should_ignore_directory(name: &str) -> bool {
    IGNORED_DIRECTORIES
        .iter()
        .any(|ignored| name.eq_ignore_ascii_case(ignored))
}

fn relative_display_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.as_os_str().is_empty() {
        return ".".to_string();
    }

    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::AtomicBool,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{scan_workspace, scan_workspace_with_cancel, SCAN_CANCELLED};

    fn fixture_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("decc-{name}-{unique}"))
    }

    #[test]
    fn cancelled_scan_exits_before_traversal() {
        let root = fixture_root("cancelled");
        fs::create_dir_all(root.join("apps/web")).expect("create web");
        fs::write(
            root.join("apps/web/package.json"),
            r#"{"name":"web","scripts":{"dev":"vite"}}"#,
        )
        .expect("package");

        let cancelled = AtomicBool::new(true);
        let error =
            scan_workspace_with_cancel(&root, &cancelled).expect_err("cancelled scan should stop");
        assert_eq!(error, SCAN_CANCELLED);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_generated_directories_and_detects_nested_apps() {
        let root = fixture_root("scan");
        fs::create_dir_all(root.join("apps/web")).expect("create web");
        fs::create_dir_all(root.join("node_modules/ignored")).expect("create ignored");
        fs::write(
            root.join("apps/web/package.json"),
            r#"{"name":"web","scripts":{"dev":"vite"},"devDependencies":{"vite":"latest"}}"#,
        )
        .expect("package");
        fs::write(
            root.join("node_modules/ignored/package.json"),
            r#"{"name":"ignored","scripts":{"dev":"vite"}}"#,
        )
        .expect("ignored package");

        let result = scan_workspace(&root).expect("scan");
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].name, "web");
        assert_eq!(result.candidates[0].path, "apps/web");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_go_main_package() {
        let root = fixture_root("go");
        fs::create_dir_all(root.join("api")).expect("create api");
        fs::write(root.join("api/go.mod"), "module example.com/acme/api\n").expect("go mod");
        fs::write(root.join("api/main.go"), "package main\n\nfunc main() {}\n").expect("go main");

        let result = scan_workspace(&root).expect("scan");
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].technology, "Go");
        assert_eq!(result.candidates[0].command, "go run .");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_rust_dotnet_and_repository_orchestration() {
        let root = fixture_root("ecosystems");

        fs::create_dir_all(root.join("tools/cli/src")).expect("create rust");
        fs::write(
            root.join("tools/cli/Cargo.toml"),
            "[package]\nname = \"acme-cli\"\nversion = \"0.1.0\"\n",
        )
        .expect("cargo manifest");
        fs::write(root.join("tools/cli/src/main.rs"), "fn main() {}\n").expect("rust main");

        fs::create_dir_all(root.join("services/web")).expect("create dotnet");
        fs::write(
            root.join("services/web/Web App.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk.Web"><PropertyGroup><AssemblyName>Acme.Web</AssemblyName></PropertyGroup></Project>"#,
        )
        .expect("csproj");

        fs::create_dir_all(root.join("infra")).expect("create infra");
        fs::write(root.join("infra/compose.yaml"), "services: {}\n").expect("compose");
        fs::write(root.join("infra/Makefile"), "dev:\n\t@echo dev\n").expect("make");

        let result = scan_workspace(&root).expect("scan");
        let technologies: Vec<_> = result
            .candidates
            .iter()
            .map(|candidate| candidate.technology.as_str())
            .collect();

        assert!(technologies.contains(&"Rust"));
        assert!(technologies.contains(&"ASP.NET Core"));
        assert!(technologies.contains(&"Docker Compose"));
        assert!(technologies.contains(&"Make"));
        assert!(result
            .candidates
            .iter()
            .any(|candidate| candidate.command == "cargo run"));
        assert!(result
            .candidates
            .iter()
            .any(|candidate| candidate.command == "dotnet run --project \"Web App.csproj\""));
        let dotnet = result
            .candidates
            .iter()
            .find(|candidate| candidate.technology == "ASP.NET Core")
            .expect("dotnet candidate");
        assert_eq!(dotnet.program, "dotnet");
        assert_eq!(
            dotnet.args,
            vec![
                "run".to_string(),
                "--project".to_string(),
                "Web App.csproj".to_string(),
            ]
        );
        assert!(result
            .candidates
            .iter()
            .any(|candidate| candidate.command == "docker compose up"));
        assert!(result
            .candidates
            .iter()
            .any(|candidate| candidate.command == "make dev"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_make_app_variable_run_targets() {
        let root = fixture_root("make-app-variable");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("Makefile"),
            r#"
VALID_APPS := api worker
APP ?=
.PHONY: run
run:
	@echo run $(APP)
"#,
        )
        .expect("makefile");

        let result = scan_workspace(&root).expect("scan");
        let make_commands = result
            .candidates
            .iter()
            .filter(|candidate| candidate.technology == "Make")
            .map(|candidate| candidate.command.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            make_commands,
            vec!["make run APP=api", "make run APP=worker"],
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_make_positional_app_run_targets() {
        let root = fixture_root("make-apps");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("Makefile"),
            r#"
VALID_APPS := api worker
GOAL_APP := $(firstword $(filter $(VALID_APPS),$(MAKECMDGOALS)))
ifneq ($(GOAL_APP),)
APP := $(GOAL_APP)
endif
.PHONY: run $(VALID_APPS)
run:
	@echo run $(APP)
$(VALID_APPS):
"#,
        )
        .expect("makefile");

        let result = scan_workspace(&root).expect("scan");
        let make_commands = result
            .candidates
            .iter()
            .filter(|candidate| candidate.technology == "Make")
            .map(|candidate| candidate.command.as_str())
            .collect::<Vec<_>>();

        assert_eq!(make_commands, vec!["make run api", "make run worker"]);

        let _ = fs::remove_dir_all(root);
    }
}
