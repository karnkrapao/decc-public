use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tauri::Manager;

use crate::{
    core::{discovery, workspaces},
    models::{detection::WorkspaceScanResult, workspace::PersistedWorkspace},
};

const WORKSPACE_STORE_FILE: &str = "workspaces.v1.json";

pub(crate) fn select_workspaces(
    mut workspaces: Vec<PersistedWorkspace>,
    workspace_id: Option<String>,
) -> Result<Vec<PersistedWorkspace>, String> {
    if let Some(workspace_id) = workspace_id {
        workspaces.retain(|workspace| workspace.id == workspace_id);
        if workspaces.is_empty() {
            return Err("Workspace was not found.".to_string());
        }
    }
    Ok(workspaces)
}

#[derive(Clone, Default)]
pub struct WorkspaceCommandState {
    pub(crate) store_lock: Arc<Mutex<()>>,
    active_scans: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl WorkspaceCommandState {
    fn begin_scan(&self, scan_id: &str) -> Result<Arc<AtomicBool>, String> {
        if scan_id.is_empty() || scan_id.len() > 128 {
            return Err("Scan request ID is invalid.".to_string());
        }

        let mut active = self
            .active_scans
            .lock()
            .map_err(|_| "Workspace scan state is unavailable.".to_string())?;
        if active.contains_key(scan_id) {
            return Err("A scan with this request ID is already active.".to_string());
        }

        let cancelled = Arc::new(AtomicBool::new(false));
        active.insert(scan_id.to_string(), cancelled.clone());
        Ok(cancelled)
    }

    fn cancel_scan(&self, scan_id: &str) -> Result<bool, String> {
        let active = self
            .active_scans
            .lock()
            .map_err(|_| "Workspace scan state is unavailable.".to_string())?;
        let Some(cancelled) = active.get(scan_id) else {
            return Ok(false);
        };
        cancelled.store(true, Ordering::Relaxed);
        Ok(true)
    }

    fn finish_scan(&self, scan_id: &str, cancelled: &Arc<AtomicBool>) -> Result<(), String> {
        let mut active = self
            .active_scans
            .lock()
            .map_err(|_| "Workspace scan state is unavailable.".to_string())?;
        if active
            .get(scan_id)
            .is_some_and(|current| Arc::ptr_eq(current, cancelled))
        {
            active.remove(scan_id);
        }
        Ok(())
    }
}

#[tauri::command]
pub async fn scan_workspace(
    state: tauri::State<'_, WorkspaceCommandState>,
    path: String,
    scan_id: String,
) -> Result<WorkspaceScanResult, String> {
    let path = PathBuf::from(path);
    let cancelled = state.begin_scan(&scan_id)?;
    let scan_cancelled = cancelled.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        discovery::scan_workspace_with_cancel(&path, &scan_cancelled)
    })
    .await;

    state.finish_scan(&scan_id, &cancelled)?;
    joined.map_err(|error| format!("Workspace scan failed: {error}"))?
}

#[tauri::command]
pub async fn cancel_workspace_scan(
    state: tauri::State<'_, WorkspaceCommandState>,
    scan_id: String,
) -> Result<bool, String> {
    state.cancel_scan(&scan_id)
}

#[tauri::command]
pub async fn list_workspaces(
    app: tauri::AppHandle,
    state: tauri::State<'_, WorkspaceCommandState>,
) -> Result<Vec<PersistedWorkspace>, String> {
    let store_path = workspace_store_path(&app)?;
    let store_lock = state.store_lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::list(&store_path)
    })
    .await
    .map_err(|error| format!("Workspace loading failed: {error}"))?
}

#[tauri::command]
pub async fn accept_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, WorkspaceCommandState>,
    process_state: tauri::State<'_, crate::commands::process::ProcessCommandState>,
    root_path: String,
    candidate_ids: Vec<String>,
) -> Result<PersistedWorkspace, String> {
    let store_path = workspace_store_path(&app)?;
    let root_path = PathBuf::from(root_path);
    let canonical_root = std::fs::canonicalize(&root_path)
        .map_err(|error| format!("Workspace folder is unavailable: {error}"))?;
    let canonical_text = canonical_root.to_string_lossy().into_owned();

    let existing_workspace_id = {
        let _guard = state
            .store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::list(&store_path)?
            .into_iter()
            .find(|workspace| workspace.path == canonical_text)
            .map(|workspace| workspace.id)
    };
    if let Some(workspace_id) = existing_workspace_id {
        if process_state.has_workspace_processes(&workspace_id)? {
            return Err(
                "Stop all DECC-owned processes before updating this workspace from a rescan."
                    .to_string(),
            );
        }
    }

    let store_lock = state.store_lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::accept(&store_path, &root_path, &candidate_ids)
    })
    .await
    .map_err(|error| format!("Workspace persistence failed: {error}"))?
}

#[tauri::command]
pub async fn set_workspace_trust(
    app: tauri::AppHandle,
    state: tauri::State<'_, WorkspaceCommandState>,
    workspace_id: String,
    trusted: bool,
) -> Result<PersistedWorkspace, String> {
    let store_path = workspace_store_path(&app)?;
    let store_lock = state.store_lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::set_trusted(&store_path, &workspace_id, trusted)
    })
    .await
    .map_err(|error| format!("Workspace trust update failed: {error}"))?
}

#[tauri::command]
pub async fn remove_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, WorkspaceCommandState>,
    process_state: tauri::State<'_, crate::commands::process::ProcessCommandState>,
    workspace_id: String,
) -> Result<bool, String> {
    if process_state.has_workspace_processes(&workspace_id)? {
        return Err("Stop all DECC-owned processes before removing this workspace.".to_string());
    }
    let store_path = workspace_store_path(&app)?;
    let store_lock = state.store_lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::remove(&store_path, &workspace_id)
    })
    .await
    .map_err(|error| format!("Workspace removal failed: {error}"))?
}

pub(crate) fn workspace_store_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(WORKSPACE_STORE_FILE))
        .map_err(|error| format!("Could not resolve DECC app-data directory: {error}"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::WorkspaceCommandState;

    #[test]
    fn scan_cancellation_is_request_scoped() {
        let state = WorkspaceCommandState::default();
        let first = state.begin_scan("scan-one").expect("first scan");
        let second = state.begin_scan("scan-two").expect("second scan");

        assert!(state.cancel_scan("scan-one").expect("cancel first"));
        assert!(first.load(Ordering::Relaxed));
        assert!(!second.load(Ordering::Relaxed));

        state.finish_scan("scan-one", &first).expect("finish first");
        assert!(!state.cancel_scan("scan-one").expect("first removed"));
        assert!(state.cancel_scan("scan-two").expect("cancel second"));
        assert!(second.load(Ordering::Relaxed));

        state
            .finish_scan("scan-two", &second)
            .expect("finish second");
    }

    #[test]
    fn duplicate_active_scan_id_is_rejected_until_original_finishes() {
        let state = WorkspaceCommandState::default();
        let first = state.begin_scan("same-scan").expect("first scan");
        let error = state
            .begin_scan("same-scan")
            .expect_err("duplicate ID should fail");
        assert!(error.contains("already active"));

        state.finish_scan("same-scan", &first).expect("finish");
        assert!(state.begin_scan("same-scan").is_ok());
    }
}
