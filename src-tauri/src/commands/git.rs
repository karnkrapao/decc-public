use crate::{
    commands::{
        process::ProcessCommandState,
        workspace::{workspace_store_path, WorkspaceCommandState},
    },
    core::{git, workspaces},
    models::{
        git::{WorkspaceGitHeadInspection, WorkspaceGitInspection},
        workspace::PersistedWorkspace,
    },
};

#[tauri::command]
pub async fn inspect_git(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    workspace_id: Option<String>,
) -> Result<Vec<WorkspaceGitInspection>, String> {
    let store_path = workspace_store_path(&app)?;
    let mut workspaces = {
        let _guard = workspace_state
            .store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::list(&store_path)?
    };

    if let Some(workspace_id) = workspace_id {
        workspaces.retain(|workspace| workspace.id == workspace_id);
        if workspaces.is_empty() {
            return Err("Workspace was not found.".to_string());
        }
    }

    tauri::async_runtime::spawn_blocking(move || git::inspect(&workspaces))
        .await
        .map_err(|error| format!("Git inspection failed: {error}"))
}

#[tauri::command]
pub async fn inspect_git_heads(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    workspace_id: Option<String>,
) -> Result<Vec<WorkspaceGitHeadInspection>, String> {
    let store_path = workspace_store_path(&app)?;
    let mut workspaces = {
        let _guard = workspace_state
            .store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::list(&store_path)?
    };

    if let Some(workspace_id) = workspace_id {
        workspaces.retain(|workspace| workspace.id == workspace_id);
        if workspaces.is_empty() {
            return Err("Workspace was not found.".to_string());
        }
    }

    tauri::async_runtime::spawn_blocking(move || git::inspect_heads(&workspaces))
        .await
        .map_err(|error| format!("Git head inspection failed: {error}"))
}

#[tauri::command]
pub async fn list_git_branches(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    workspace_id: String,
) -> Result<Vec<String>, String> {
    let workspace = load_workspace(&app, &workspace_state, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || git::list_local_branches(&workspace))
        .await
        .map_err(|error| format!("Git branch loading failed: {error}"))?
}

#[tauri::command]
pub async fn switch_git_branch(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: String,
    branch: String,
) -> Result<WorkspaceGitInspection, String> {
    if process_state.has_workspace_processes(&workspace_id)? {
        return Err(
            "Stop DECC-owned processes for this workspace before switching Git branches."
                .to_string(),
        );
    }

    let workspace = load_workspace(&app, &workspace_state, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || git::switch_branch(&workspace, &branch))
        .await
        .map_err(|error| format!("Git branch switch failed: {error}"))?
}

fn load_workspace(
    app: &tauri::AppHandle,
    workspace_state: &WorkspaceCommandState,
    workspace_id: &str,
) -> Result<PersistedWorkspace, String> {
    let store_path = workspace_store_path(app)?;
    let _guard = workspace_state
        .store_lock
        .lock()
        .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
    workspaces::list(&store_path)?
        .into_iter()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or_else(|| "Workspace was not found.".to_string())
}
