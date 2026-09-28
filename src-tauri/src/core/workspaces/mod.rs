mod store;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    core::discovery,
    models::{
        detection::DetectedCandidate,
        workspace::{PersistedComponent, PersistedWorkspace},
    },
};

pub use store::WorkspaceStore;

pub fn list(store_path: &Path) -> Result<Vec<PersistedWorkspace>, String> {
    Ok(WorkspaceStore::load(store_path)?.workspaces)
}

pub fn accept(
    store_path: &Path,
    root_path: &Path,
    selected_candidate_ids: &[String],
) -> Result<PersistedWorkspace, String> {
    let scan = discovery::scan_workspace(root_path)?;
    let selected: HashSet<&str> = selected_candidate_ids.iter().map(String::as_str).collect();
    let available: HashSet<&str> = scan
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect();
    let mut missing = selected.difference(&available).copied().collect::<Vec<_>>();
    missing.sort_unstable();
    if !missing.is_empty() {
        return Err(format!(
            "The workspace changed after scanning. Rescan before saving. Missing candidates: {}",
            missing.join(", ")
        ));
    }

    let components = scan
        .candidates
        .into_iter()
        .filter(|candidate| selected.contains(candidate.id.as_str()))
        .map(component_from_candidate)
        .collect::<Vec<_>>();

    let canonical_path = PathBuf::from(&scan.root_path);
    let now = unix_time_ms()?;
    let mut store = WorkspaceStore::load(store_path)?;
    let id = workspace_id(&scan.root_path);

    let workspace = if let Some(existing) = store.workspaces.iter_mut().find(|item| item.id == id) {
        existing.name = workspace_name(&canonical_path);
        existing.path = scan.root_path;
        existing.components = components;
        existing.updated_at_ms = now;
        existing.clone()
    } else {
        let workspace = PersistedWorkspace {
            id,
            name: workspace_name(&canonical_path),
            path: scan.root_path,
            trusted: false,
            components,
            created_at_ms: now,
            updated_at_ms: now,
        };
        store.workspaces.push(workspace.clone());
        workspace
    };

    store
        .workspaces
        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    store.save(store_path)?;
    Ok(workspace)
}

pub fn set_trusted(
    store_path: &Path,
    workspace_id: &str,
    trusted: bool,
) -> Result<PersistedWorkspace, String> {
    let mut store = WorkspaceStore::load(store_path)?;
    let now = unix_time_ms()?;
    let workspace = store
        .workspaces
        .iter_mut()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or_else(|| "Workspace was not found in local storage.".to_string())?;

    workspace.trusted = trusted;
    workspace.updated_at_ms = now;
    let updated = workspace.clone();
    store.save(store_path)?;
    Ok(updated)
}

pub fn remove(store_path: &Path, workspace_id: &str) -> Result<bool, String> {
    let mut store = WorkspaceStore::load(store_path)?;
    let previous_len = store.workspaces.len();
    store
        .workspaces
        .retain(|workspace| workspace.id != workspace_id);
    let removed = store.workspaces.len() != previous_len;
    if removed {
        store.save(store_path)?;
    }
    Ok(removed)
}

fn component_from_candidate(candidate: DetectedCandidate) -> PersistedComponent {
    PersistedComponent {
        id: candidate.id,
        name: candidate.name,
        relative_path: candidate.path,
        technology: candidate.technology,
        runtime: candidate.runtime,
        command: candidate.command,
        program: Some(candidate.program),
        args: candidate.args,
        expected_ports: candidate.expected_ports,
        include_in_run_all: true,
        detection_confidence: candidate.confidence,
    }
}

fn workspace_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Workspace")
        .to_string()
}

fn workspace_id(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace");
    let slug = slugify(name);
    format!(
        "workspace-{}-{:016x}",
        if slug.is_empty() { "local" } else { &slug },
        fnv1a64(path.as_bytes())
    )
}

fn slugify(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut previous_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            output.push('-');
            previous_dash = true;
        }
    }
    output.trim_matches('-').to_string()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn unix_time_ms() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("System clock is before Unix epoch: {error}"))?;
    u64::try_from(duration.as_millis()).map_err(|_| "System time is out of range.".to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{accept, list, remove, set_trusted};

    fn fixture_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("decc-persist-{name}-{unique}"))
    }

    #[test]
    fn accepts_lists_and_persists_trust_without_accepting_frontend_commands() {
        let root = fixture_root("workspace");
        let store_path = fixture_root("store").join("workspaces.v1.json");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("package.json"),
            r#"{"name":"sample-web","scripts":{"dev":"vite"},"devDependencies":{"vite":"latest"}}"#,
        )
        .expect("package");

        let scanned = crate::core::discovery::scan_workspace(&root).expect("scan");
        let candidate_id = scanned.candidates[0].id.clone();
        let workspace = accept(&store_path, &root, &[candidate_id]).expect("accept");
        assert!(!workspace.trusted);
        assert_eq!(workspace.components.len(), 1);
        assert_eq!(workspace.components[0].command, "npm run dev");
        assert_eq!(workspace.components[0].program.as_deref(), Some("npm"));
        assert_eq!(
            workspace.components[0].args,
            vec!["run".to_string(), "dev".to_string()]
        );

        let trusted = set_trusted(&store_path, &workspace.id, true).expect("trust");
        assert!(trusted.trusted);
        let loaded = list(&store_path).expect("list");
        assert_eq!(loaded, vec![trusted.clone()]);

        assert!(remove(&store_path, &workspace.id).expect("remove"));
        assert!(list(&store_path).expect("empty").is_empty());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(store_path.parent().expect("store parent"));
    }

    #[test]
    fn reaccepting_after_rescan_preserves_workspace_trust() {
        let root = fixture_root("rescan");
        let store_path = fixture_root("rescan-store").join("workspaces.v1.json");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("package.json"),
            r#"{"name":"sample-web","scripts":{"dev":"vite"},"devDependencies":{"vite":"latest"}}"#,
        )
        .expect("package");

        let first_scan = crate::core::discovery::scan_workspace(&root).expect("first scan");
        let first = accept(&store_path, &root, &[first_scan.candidates[0].id.clone()])
            .expect("first accept");
        let trusted = set_trusted(&store_path, &first.id, true).expect("trust");
        assert!(trusted.trusted);

        fs::write(
            root.join("package.json"),
            r#"{"name":"sample-web","scripts":{"dev":"vite --port 4242"},"devDependencies":{"vite":"latest"}}"#,
        )
        .expect("updated package");
        let second_scan = crate::core::discovery::scan_workspace(&root).expect("second scan");
        let rescanned = accept(&store_path, &root, &[second_scan.candidates[0].id.clone()])
            .expect("rescan accept");

        assert_eq!(rescanned.id, first.id);
        assert!(rescanned.trusted);
        assert_eq!(rescanned.components[0].expected_ports, vec![4242]);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(store_path.parent().expect("store parent"));
    }

    #[test]
    fn rejects_stale_candidate_ids_when_workspace_changes_before_accept() {
        let root = fixture_root("stale");
        let store_path = fixture_root("stale-store").join("workspaces.v1.json");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("package.json"),
            r#"{"name":"sample-web","scripts":{"dev":"vite"}}"#,
        )
        .expect("package");

        let scanned = crate::core::discovery::scan_workspace(&root).expect("scan");
        let candidate_id = scanned.candidates[0].id.clone();
        fs::write(
            root.join("package.json"),
            r#"{"name":"sample-web","scripts":{"build":"vite build"}}"#,
        )
        .expect("change package");

        let error = accept(&store_path, &root, &[candidate_id]).expect_err("stale review");
        assert!(error.contains("changed after scanning"));
        assert!(list(&store_path).expect("list").is_empty());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(store_path.parent().expect("store parent"));
    }
}
