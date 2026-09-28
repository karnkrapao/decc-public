use crate::{
    commands::workspace::{select_workspaces, workspace_store_path, WorkspaceCommandState},
    core::{runtime, workspaces},
    models::runtime::WorkspaceRuntimeInspection,
};

#[tauri::command]
pub async fn inspect_runtimes(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    workspace_id: Option<String>,
) -> Result<Vec<WorkspaceRuntimeInspection>, String> {
    let store_path = workspace_store_path(&app)?;
    let workspaces = {
        let _guard = workspace_state
            .store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::list(&store_path)?
    };
    let workspaces = select_workspaces(workspaces, workspace_id)?;

    tauri::async_runtime::spawn_blocking(move || runtime::inspect(&workspaces))
        .await
        .map_err(|error| format!("Runtime inspection failed: {error}"))
}
