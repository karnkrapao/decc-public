use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::models::{
    preferences::EditorSelection,
    workspace::{PersistedComponent, PersistedWorkspace},
};

const MAX_DISCOVERY_ENTRIES: usize = 2400;

pub fn discover_editors() -> Vec<EditorSelection> {
    let mut editors = BTreeMap::<String, EditorSelection>::new();

    #[cfg(target_os = "macos")]
    discover_macos_editors(&mut editors);

    #[cfg(windows)]
    discover_windows_editors(&mut editors);

    #[cfg(not(any(target_os = "macos", windows)))]
    discover_path_editors(&mut editors);

    let mut editors = editors.into_values().collect::<Vec<_>>();
    editors.sort_by(|left, right| left.label.to_lowercase().cmp(&right.label.to_lowercase()));
    editors
}

pub fn editor_from_path(path: &str) -> Result<EditorSelection, String> {
    normalize_editor_path(Path::new(path))
}

pub fn open_in_ide(
    workspace: &PersistedWorkspace,
    component_id: Option<&str>,
    relative_path: Option<&str>,
    editor: &EditorSelection,
) -> Result<(), String> {
    let target = resolve_target(workspace, component_id, relative_path)?;
    let editor = normalize_editor_path(Path::new(&editor.path))?;
    let workspace_root = fs::canonicalize(&workspace.path)
        .map_err(|error| format!("Workspace folder is unavailable: {error}"))?;
    let editor_path = fs::canonicalize(&editor.path)
        .map_err(|error| format!("Editor is unavailable: {error}"))?;
    if editor_path.starts_with(&workspace_root) {
        return Err("Editor executable/application cannot be inside the workspace.".to_string());
    }
    launch(&editor, &target)
}

fn resolve_target(
    workspace: &PersistedWorkspace,
    component_id: Option<&str>,
    relative_path: Option<&str>,
) -> Result<PathBuf, String> {
    if component_id.is_some() && relative_path.is_some() {
        return Err(
            "Choose either a Component target or a relative file target, not both.".to_string(),
        );
    }

    let root = fs::canonicalize(&workspace.path)
        .map_err(|error| format!("Workspace folder is unavailable: {error}"))?;
    if !root.is_dir() {
        return Err("Workspace path is not a directory.".to_string());
    }

    let candidate = if let Some(component_id) = component_id {
        let component = workspace
            .components
            .iter()
            .find(|component| component.id == component_id)
            .ok_or_else(|| "Component was not found in this workspace.".to_string())?;
        component_path(&root, component)
    } else if let Some(relative_path) = relative_path {
        let relative = Path::new(relative_path);
        if relative.is_absolute() {
            return Err("IDE file target must be relative to the workspace.".to_string());
        }
        root.join(relative)
    } else {
        root.clone()
    };

    let target = fs::canonicalize(candidate)
        .map_err(|error| format!("IDE target is unavailable: {error}"))?;
    if !target.starts_with(&root) {
        return Err("IDE target resolves outside the workspace.".to_string());
    }
    Ok(target)
}

fn component_path(root: &Path, component: &PersistedComponent) -> PathBuf {
    if component.relative_path.is_empty() || component.relative_path == "." {
        root.to_path_buf()
    } else {
        root.join(&component.relative_path)
    }
}

fn normalize_editor_path(path: &Path) -> Result<EditorSelection, String> {
    if !path.is_absolute() {
        return Err("Editor path must be absolute.".to_string());
    }
    let canonical =
        fs::canonicalize(path).map_err(|error| format!("Editor path is unavailable: {error}"))?;

    #[cfg(target_os = "macos")]
    let valid = (canonical.is_dir()
        && canonical
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app")))
        || is_executable_file(&canonical);

    #[cfg(not(target_os = "macos"))]
    let valid = is_executable_file(&canonical);

    if !valid {
        return Err("Selected path is not a launchable editor application/executable.".to_string());
    }

    let label = editor_label(&canonical);
    Ok(EditorSelection {
        label,
        path: canonical.to_string_lossy().into_owned(),
    })
}

fn editor_label(path: &Path) -> String {
    let name = path
        .file_stem()
        .or_else(|| path.file_name())
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Editor".to_string());

    match name.to_ascii_lowercase().as_str() {
        "code" => "Visual Studio Code".to_string(),
        "codium" => "VSCodium".to_string(),
        "subl" => "Sublime Text".to_string(),
        "idea" | "idea64" => "IntelliJ IDEA".to_string(),
        "webstorm64" => "WebStorm".to_string(),
        "goland64" => "GoLand".to_string(),
        "rider64" => "Rider".to_string(),
        "pycharm64" => "PyCharm".to_string(),
        other if other.ends_with(".app") => name.trim_end_matches(".app").to_string(),
        _ => name,
    }
}

fn looks_like_editor_name(name: &str) -> bool {
    let normalized = name
        .trim()
        .trim_end_matches(".app")
        .trim_end_matches(".exe")
        .to_ascii_lowercase();

    if matches!(
        normalized.as_str(),
        "code"
            | "code - insiders"
            | "visual studio code"
            | "visual studio code - insiders"
            | "vscodium"
            | "xcode"
            | "cursor"
            | "zed"
            | "windsurf"
            | "subl"
            | "sublime text"
            | "nova"
            | "bbedit"
            | "coteditor"
            | "textmate"
            | "fleet"
            | "trae"
    ) {
        return true;
    }

    [
        "webstorm",
        "goland",
        "rider",
        "pycharm",
        "phpstorm",
        "rubymine",
        "intellij idea",
        "android studio",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

fn insert_editor(editors: &mut BTreeMap<String, EditorSelection>, path: &Path) {
    let Ok(editor) = normalize_editor_path(path) else {
        return;
    };
    let key = editor.label.trim().to_ascii_lowercase();
    match editors.get(&key) {
        Some(existing)
            if editor_path_priority(&existing.path) >= editor_path_priority(&editor.path) => {}
        _ => {
            editors.insert(key, editor);
        }
    }
}

fn editor_path_priority(path: &str) -> u8 {
    let path = Path::new(path);
    #[cfg(target_os = "macos")]
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        return 3;
    }

    if path.is_absolute() {
        2
    } else {
        1
    }
}

#[cfg(target_os = "macos")]
fn discover_macos_editors(editors: &mut BTreeMap<String, EditorSelection>) {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = home_dir() {
        roots.push(home.join("Applications"));
    }

    for root in roots {
        scan_macos_app_root(&root, editors);
    }

    discover_path_editors(editors);
}

#[cfg(target_os = "macos")]
fn scan_macos_app_root(root: &Path, editors: &mut BTreeMap<String, EditorSelection>) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;

    while let Some((directory, depth)) = stack.pop() {
        if visited >= MAX_DISCOVERY_ENTRIES || depth > 4 {
            break;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited >= MAX_DISCOVERY_ENTRIES {
                break;
            }
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
            {
                if looks_like_editor_name(&file_name) {
                    insert_editor(editors, &path);
                }
                continue;
            }
            if depth < 4 && entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                stack.push((path, depth + 1));
            }
        }
    }
}

#[cfg(windows)]
fn discover_windows_editors(editors: &mut BTreeMap<String, EditorSelection>) {
    let mut roots = Vec::new();
    for key in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(value) = std::env::var_os(key) {
            roots.push(PathBuf::from(value));
        }
    }

    for root in roots {
        scan_windows_editor_root(&root, editors);
    }
    discover_path_editors(editors);
}

#[cfg(windows)]
fn scan_windows_editor_root(root: &Path, editors: &mut BTreeMap<String, EditorSelection>) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;

    while let Some((directory, depth)) = stack.pop() {
        if visited >= MAX_DISCOVERY_ENTRIES || depth > 4 {
            break;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited >= MAX_DISCOVERY_ENTRIES {
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_exe = path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
            if is_exe && looks_like_editor_name(&name) {
                insert_editor(editors, &path);
            } else if depth < 4
                && entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                && (depth == 0
                    || looks_like_editor_name(&name)
                    || name.eq_ignore_ascii_case("Programs"))
            {
                stack.push((path, depth + 1));
            }
        }
    }
}

fn discover_path_editors(editors: &mut BTreeMap<String, EditorSelection>) {
    let Some(path) = std::env::var_os("PATH") else {
        return;
    };
    const COMMANDS: &[&str] = &[
        "code", "cursor", "zed", "windsurf", "codium", "subl", "webstorm", "goland", "rider",
        "pycharm", "phpstorm", "rubymine", "idea", "fleet",
    ];

    for directory in std::env::split_paths(&path) {
        for command in COMMANDS {
            #[cfg(windows)]
            let candidate = directory.join(format!("{command}.exe"));
            #[cfg(not(windows))]
            let candidate = directory.join(command);
            if is_executable_file(&candidate) {
                insert_editor(editors, &candidate);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn launch(editor: &EditorSelection, target: &Path) -> Result<(), String> {
    let path = Path::new(&editor.path);
    if path.is_dir()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        let status = Command::new("/usr/bin/open")
            .arg("-a")
            .arg(path)
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("Could not open {}: {error}", editor.label))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{} could not be opened.", editor.label))
        }
    } else {
        Command::new(path)
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open {}: {error}", editor.label))
    }
}

#[cfg(not(target_os = "macos"))]
fn launch(editor: &EditorSelection, target: &Path) -> Result<(), String> {
    Command::new(&editor.path)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open {}: {error}", editor.label))
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

#[cfg(target_os = "macos")]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::models::{
        detection::DetectionConfidence,
        workspace::{PersistedComponent, PersistedWorkspace},
    };

    use super::{editor_label, insert_editor, looks_like_editor_name, resolve_target};

    fn fixture_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-ide-{name}-{unique}"));
        fs::create_dir_all(&root).expect("root");
        root
    }

    fn workspace(root: &std::path::Path) -> PersistedWorkspace {
        PersistedWorkspace {
            id: "workspace".to_string(),
            name: "Workspace".to_string(),
            path: root.to_string_lossy().into_owned(),
            trusted: false,
            components: vec![PersistedComponent {
                id: "web".to_string(),
                name: "Web".to_string(),
                relative_path: "apps/web".to_string(),
                technology: "Vite".to_string(),
                runtime: "Node.js".to_string(),
                command: "npm run dev".to_string(),
                program: Some("npm".to_string()),
                args: vec!["run".to_string(), "dev".to_string()],
                expected_ports: vec![],
                include_in_run_all: true,
                detection_confidence: DetectionConfidence::High,
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn recognizes_common_editor_names_without_limiting_to_three_apps() {
        assert!(looks_like_editor_name("Visual Studio Code.app"));
        assert!(looks_like_editor_name("WebStorm.app"));
        assert!(looks_like_editor_name("Sublime Text.app"));
        assert!(looks_like_editor_name("Windsurf.app"));
        assert!(looks_like_editor_name("Xcode.app"));
        assert!(!looks_like_editor_name("Barcode Scanner.app"));
        assert!(!looks_like_editor_name("Calculator.app"));
        assert_eq!(
            editor_label(std::path::Path::new("/usr/local/bin/code")),
            "Visual Studio Code"
        );
    }

    #[test]
    fn dedupes_editor_cli_and_product_names() {
        let root = fixture_root("dedupe");
        let code = root.join("code");
        let app_name = root.join("Visual Studio Code");
        fs::write(&code, b"").expect("code");
        fs::write(&app_name, b"").expect("app name");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [&code, &app_name] {
                let mut permissions = fs::metadata(path).expect("metadata").permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(path, permissions).expect("permissions");
            }
        }

        let mut editors = BTreeMap::new();
        insert_editor(&mut editors, &code);
        insert_editor(&mut editors, &app_name);
        assert_eq!(editors.len(), 1);
        assert_eq!(
            editors.values().next().map(|editor| editor.label.as_str()),
            Some("Visual Studio Code")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_workspace_component_and_file_targets_inside_root() {
        let root = fixture_root("targets");
        fs::create_dir_all(root.join("apps/web")).expect("component");
        fs::write(root.join(".env"), "SECRET=value\n").expect("env");
        let workspace = workspace(&root);

        assert_eq!(
            resolve_target(&workspace, None, None).expect("workspace"),
            fs::canonicalize(&root).expect("root canonical")
        );
        assert_eq!(
            resolve_target(&workspace, Some("web"), None).expect("component"),
            fs::canonicalize(root.join("apps/web")).expect("component canonical")
        );
        assert_eq!(
            resolve_target(&workspace, None, Some(".env")).expect("file"),
            fs::canonicalize(root.join(".env")).expect("file canonical")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_target_that_resolves_outside_workspace() {
        use std::os::unix::fs::symlink;

        let root = fixture_root("escape");
        fs::create_dir_all(root.join("apps/web")).expect("component");
        let outside_root = fixture_root("outside");
        let outside = outside_root.join("secret.env");
        fs::write(&outside, "SECRET=value\n").expect("outside");
        symlink(&outside, root.join(".env")).expect("symlink");
        let workspace = workspace(&root);

        let error = resolve_target(&workspace, None, Some(".env")).expect_err("escape");
        assert!(error.contains("outside the workspace"));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside_root);
    }
}
