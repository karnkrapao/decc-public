use crate::{
    commands::workspace::{workspace_store_path, WorkspaceCommandState},
    core::{ide, workspaces},
    models::preferences::EditorSelection,
};

#[tauri::command]
pub async fn discover_editors() -> Result<Vec<EditorSelection>, String> {
    tauri::async_runtime::spawn_blocking(ide::discover_editors)
        .await
        .map_err(|error| format!("Editor discovery failed: {error}"))
}

#[tauri::command]
pub async fn resolve_editor_path(path: String) -> Result<EditorSelection, String> {
    tauri::async_runtime::spawn_blocking(move || ide::editor_from_path(&path))
        .await
        .map_err(|error| format!("Editor selection failed: {error}"))?
}

#[tauri::command]
pub async fn open_in_ide(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    workspace_id: String,
    component_id: Option<String>,
    relative_path: Option<String>,
    editor: EditorSelection,
) -> Result<(), String> {
    let store_path = workspace_store_path(&app)?;
    let workspace = {
        let _guard = workspace_state
            .store_lock
            .lock()
            .map_err(|_| "Workspace store lock is unavailable.".to_string())?;
        workspaces::list(&store_path)?
            .into_iter()
            .find(|workspace| workspace.id == workspace_id)
            .ok_or_else(|| "Workspace was not found in local storage.".to_string())?
    };

    tauri::async_runtime::spawn_blocking(move || {
        ide::open_in_ide(
            &workspace,
            component_id.as_deref(),
            relative_path.as_deref(),
            &editor,
        )
    })
    .await
    .map_err(|error| format!("IDE launch failed: {error}"))?
}
