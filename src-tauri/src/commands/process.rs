use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tauri::{Emitter, Manager};

use crate::{
    commands::workspace::{workspace_store_path, WorkspaceCommandState},
    core::{
        logs::LocalLogStore,
        ports,
        process::{metrics, ProcessEventSink, ProcessManager},
        runtime, workspaces,
    },
    models::{
        ports::{ComponentPortInspection, PortListenerOrigin},
        process::{
            BulkProcessResult, ProcessActionError, ProcessEvent, ProcessHistory, ProcessStatus,
            ProcessUsage,
        },
        workspace::{PersistedComponent, PersistedWorkspace},
    },
};

pub const PROCESS_EVENT_NAME: &str = "decc-process-event";

#[derive(Clone, Default)]
pub struct ProcessCommandState {
    manager: ProcessManager,
    logs: LocalLogStore,
}

impl ProcessCommandState {
    pub(crate) fn shutdown(&self) {
        self.manager.shutdown();
    }

    pub(crate) fn owned_processes(&self) -> Result<Vec<ProcessStatus>, String> {
        self.manager.list()
    }

    pub(crate) fn has_workspace_processes(&self, workspace_id: &str) -> Result<bool, String> {
        Ok(self
            .manager
            .list()?
            .iter()
            .any(|process| process.workspace_id == workspace_id))
    }
}

#[tauri::command]
pub async fn start_component(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: String,
    component_id: String,
) -> Result<ProcessStatus, String> {
    start_component_inner(
        &app,
        &workspace_state,
        &process_state,
        &workspace_id,
        &component_id,
    )
    .await
}

async fn start_component_inner(
    app: &tauri::AppHandle,
    workspace_state: &WorkspaceCommandState,
    process_state: &ProcessCommandState,
    workspace_id: &str,
    component_id: &str,
) -> Result<ProcessStatus, String> {
    let workspace = load_workspace(app, workspace_state, workspace_id)?;
    let component = find_component(&workspace, component_id)?.clone();

    let runtime = {
        let workspace = workspace.clone();
        let component = component.clone();
        tauri::async_runtime::spawn_blocking(move || {
            runtime::inspect_component(&workspace, &component)
        })
        .await
        .map_err(|error| format!("Runtime preflight failed: {error}"))?
    };
    if !runtime.ready {
        return Err(runtime
            .blocking_message
            .unwrap_or_else(|| "Required runtime/tooling is unavailable.".to_string()));
    }

    let conflicts = inspect_component_conflicts(&workspace, process_state).await;
    if let Some(message) = conflicts.get(&component.id) {
        return Err(message.clone());
    }

    process_state.manager.start(
        &workspace,
        &component,
        app_event_sink(app.clone(), process_state.logs.clone()),
    )
}

#[tauri::command]
pub async fn stop_component(
    app: tauri::AppHandle,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: String,
    component_id: String,
) -> Result<ProcessStatus, String> {
    process_state.manager.stop(
        &workspace_id,
        &component_id,
        app_event_sink(app, process_state.logs.clone()),
    )
}

#[tauri::command]
pub async fn restart_component(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: String,
    component_id: String,
) -> Result<ProcessStatus, String> {
    process_state.manager.stop(
        &workspace_id,
        &component_id,
        app_event_sink(app.clone(), process_state.logs.clone()),
    )?;

    let manager = process_state.manager.clone();
    let wait_workspace_id = workspace_id.clone();
    let wait_component_id = component_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        loop {
            let still_owned = manager.list()?.iter().any(|process| {
                process.workspace_id == wait_workspace_id
                    && process.component_id == wait_component_id
            });
            if !still_owned {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(
                    "Timed out waiting for the existing DECC-owned process to stop before restart."
                        .to_string(),
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    })
    .await
    .map_err(|error| format!("Restart wait failed: {error}"))??;

    start_component_inner(
        &app,
        &workspace_state,
        &process_state,
        &workspace_id,
        &component_id,
    )
    .await
}

#[tauri::command]
pub async fn list_owned_processes(
    process_state: tauri::State<'_, ProcessCommandState>,
) -> Result<Vec<ProcessStatus>, String> {
    process_state.manager.list()
}

#[tauri::command]
pub async fn inspect_process_usage(
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: Option<String>,
) -> Result<Vec<ProcessUsage>, String> {
    let mut processes = process_state.manager.list()?;
    if let Some(workspace_id) = workspace_id {
        processes.retain(|process| process.workspace_id == workspace_id);
    }

    tauri::async_runtime::spawn_blocking(move || metrics::inspect(&processes))
        .await
        .map_err(|error| format!("Process resource inspection failed: {error}"))?
}

#[tauri::command]
pub async fn get_process_history(
    app: tauri::AppHandle,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: String,
    max_events: Option<usize>,
    session_id: Option<String>,
) -> Result<ProcessHistory, String> {
    let root = process_log_root(&app)?;
    let logs = process_state.logs.clone();
    tauri::async_runtime::spawn_blocking(move || {
        logs.history(&root, &workspace_id, max_events, session_id.as_deref())
    })
    .await
    .map_err(|error| format!("Local process history loading failed: {error}"))?
}

#[tauri::command]
pub async fn clear_process_history(
    app: tauri::AppHandle,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: String,
) -> Result<(), String> {
    let root = process_log_root(&app)?;
    let logs = process_state.logs.clone();
    tauri::async_runtime::spawn_blocking(move || logs.clear_workspace(&root, &workspace_id))
        .await
        .map_err(|error| format!("Local process history cleanup failed: {error}"))?
}

#[tauri::command]
pub async fn set_log_retention_days(
    app: tauri::AppHandle,
    process_state: tauri::State<'_, ProcessCommandState>,
    days: u16,
) -> Result<usize, String> {
    process_state.logs.set_retention_days(days)?;
    let root = process_log_root(&app)?;
    let logs = process_state.logs.clone();
    tauri::async_runtime::spawn_blocking(move || logs.prune_all(&root))
        .await
        .map_err(|error| format!("Local process history pruning failed: {error}"))?
}

#[tauri::command]
pub async fn run_all(
    app: tauri::AppHandle,
    workspace_state: tauri::State<'_, WorkspaceCommandState>,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: String,
) -> Result<BulkProcessResult, String> {
    let workspace = load_workspace(&app, &workspace_state, &workspace_id)?;
    if !workspace.trusted {
        return Err("Workspace is not trusted for command execution.".to_string());
    }

    let runtime_components = {
        let workspace = workspace.clone();
        tauri::async_runtime::spawn_blocking(move || {
            runtime::inspect(std::slice::from_ref(&workspace))
        })
        .await
        .map_err(|error| format!("Runtime preflight failed: {error}"))?
        .into_iter()
        .next()
        .map(|inspection| inspection.components)
        .unwrap_or_default()
    };
    let runtime_blockers = runtime_components
        .into_iter()
        .filter(|inspection| !inspection.ready)
        .map(|inspection| {
            (
                inspection.component_id,
                inspection
                    .blocking_message
                    .unwrap_or_else(|| "Required runtime/tooling is unavailable.".to_string()),
            )
        })
        .collect::<HashMap<_, _>>();

    let conflicts = inspect_component_conflicts(&workspace, &process_state).await;

    let active_components = process_state
        .owned_processes()?
        .into_iter()
        .filter(|process| process.workspace_id == workspace.id)
        .map(|process| process.component_id)
        .collect::<HashSet<_>>();

    let mut result = BulkProcessResult::default();
    let mut eligible_workspace = workspace.clone();
    eligible_workspace.components = workspace
        .components
        .iter()
        .filter(|component| component.include_in_run_all)
        .filter_map(|component| {
            if active_components.contains(&component.id) {
                return None;
            }
            if let Some(message) = runtime_blockers.get(&component.id) {
                result.errors.push(ProcessActionError {
                    component_id: component.id.clone(),
                    message: message.clone(),
                });
                return None;
            }
            if let Some(message) = conflicts.get(&component.id) {
                result.errors.push(ProcessActionError {
                    component_id: component.id.clone(),
                    message: message.clone(),
                });
                return None;
            }
            Some(component.clone())
        })
        .collect();

    let started = process_state.manager.run_all(
        &eligible_workspace,
        app_event_sink(app, process_state.logs.clone()),
    )?;
    result.processes.extend(started.processes);
    result.errors.extend(started.errors);
    Ok(result)
}

#[tauri::command]
pub async fn stop_all(
    app: tauri::AppHandle,
    process_state: tauri::State<'_, ProcessCommandState>,
    workspace_id: String,
) -> Result<BulkProcessResult, String> {
    process_state.manager.stop_all(
        &workspace_id,
        app_event_sink(app, process_state.logs.clone()),
    )
}

async fn inspect_component_conflicts(
    workspace: &PersistedWorkspace,
    process_state: &ProcessCommandState,
) -> HashMap<String, String> {
    let owned = process_state.owned_processes().unwrap_or_default();
    let workspace = workspace.clone();
    let inspection = tauri::async_runtime::spawn_blocking(move || {
        ports::inspect(std::slice::from_ref(&workspace), &owned)
    })
    .await
    .ok()
    .and_then(Result::ok)
    .and_then(|mut inspections| inspections.pop());

    inspection
        .map(|inspection| {
            inspection
                .components
                .iter()
                .filter_map(|component| {
                    conflict_message(component)
                        .map(|message| (component.component_id.clone(), message))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn conflict_message(component: &ComponentPortInspection) -> Option<String> {
    let listener = component.listeners.iter().find(|listener| {
        matches!(listener.origin, PortListenerOrigin::External)
            && (listener.matches_expected_port || component.external_running)
    })?;

    let process = listener
        .process_name
        .as_deref()
        .unwrap_or("External process");
    let pid = listener
        .pid
        .map(|pid| format!(" · PID {pid}"))
        .unwrap_or_default();
    let cwd = listener
        .cwd
        .as_deref()
        .map(|cwd| format!(" · {cwd}"))
        .unwrap_or_default();

    if listener.matches_expected_port {
        Some(format!(
            "Port {} is already in use by {process}{pid}{cwd}. DECC will not take over or stop that process.",
            listener.port
        ))
    } else {
        Some(format!(
            "{process}{pid} is already listening on port {} from this component workspace{cwd}. DECC will not start a duplicate or take over that process.",
            listener.port
        ))
    }
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
        .ok_or_else(|| "Workspace was not found in local storage.".to_string())
}

fn find_component<'a>(
    workspace: &'a PersistedWorkspace,
    component_id: &str,
) -> Result<&'a PersistedComponent, String> {
    workspace
        .components
        .iter()
        .find(|component| component.id == component_id)
        .ok_or_else(|| "Component was not found in the persisted workspace.".to_string())
}

fn app_event_sink(app: tauri::AppHandle, logs: LocalLogStore) -> ProcessEventSink {
    Arc::new(move |event: ProcessEvent| {
        if let Ok(root) = process_log_root(&app) {
            let _ = logs.record(&root, &event);
        }
        let _ = app.emit(PROCESS_EVENT_NAME, event);
    })
}

fn process_log_root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("logs").join("v1"))
        .map_err(|error| format!("Could not resolve DECC app-data log directory: {error}"))
}
