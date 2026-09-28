use crate::{
    commands::{
        process::ProcessCommandState,
        workspace::{select_workspaces, workspace_store_path, WorkspaceCommandState},
    },
    core::{ports, workspaces},
    models::ports::WorkspacePortInspection,
};

#[tauri::command]
pub async fn inspect_ports(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: Option<String>,
) -> Result<Vec<WorkspacePortInspection>, String> {
    let store_path = workspace_store_path(&app)?;
    let workspaces = {
        let _guard = workspace_state
            .store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::list(&store_path)?
    };
    let workspaces = select_workspaces(workspaces, workspace_id)?;
    let owned = process_state.owned_processes()?;
    tauri::async_runtime::spawn_blocking(move || ports::inspect(&workspaces, &owned))
        .await
        .map_err(|error| format!("Port inspection failed: {error}"))?
}
