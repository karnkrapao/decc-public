use crate::{
    commands::workspace::{select_workspaces, workspace_store_path, WorkspaceCommandState},
    core::{environment, workspaces},
    models::environment::WorkspaceEnvironmentInspection,
};

#[tauri::command]
pub async fn inspect_environment(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    workspace_id: Option<String>,
) -> Result<Vec<WorkspaceEnvironmentInspection>, String> {
    let store_path = workspace_store_path(&app)?;
    let workspaces = {
        let _guard = workspace_state
            .store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::list(&store_path)?
    };
    let workspaces = select_workspaces(workspaces, workspace_id)?;

    tauri::async_runtime::spawn_blocking(move || environment::inspect(&workspaces))
        .await
        .map_err(|error| format!("Environment inspection failed: {error}"))
}
