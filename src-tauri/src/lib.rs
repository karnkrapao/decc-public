mod commands;
mod core;
mod models;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(commands::workspace::WorkspaceCommandState::default())
        .manage(commands::preferences::PreferencesCommandState::default())
        .manage(commands::process::ProcessCommandState::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::workspace::scan_workspace,
            commands::workspace::cancel_workspace_scan,
            commands::workspace::list_workspaces,
            commands::workspace::accept_workspace,
            commands::workspace::set_workspace_trust,
            commands::workspace::remove_workspace,
            commands::preferences::get_preferences,
            commands::preferences::save_preferences,
            commands::environment::inspect_environment,
            commands::git::inspect_git,
            commands::git::inspect_git_heads,
            commands::git::list_git_branches,
            commands::git::switch_git_branch,
            commands::ide::discover_editors,
            commands::ide::resolve_editor_path,
            commands::ide::open_in_ide,
            commands::ports::inspect_ports,
            commands::runtime::inspect_runtimes,
            commands::process::start_component,
            commands::process::stop_component,
            commands::process::restart_component,
            commands::process::list_owned_processes,
            commands::process::inspect_process_usage,
            commands::process::get_process_history,
            commands::process::clear_process_history,
            commands::process::set_log_retention_days,
            commands::process::run_all,
            commands::process::stop_all,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            app_handle
                .state::<commands::process::ProcessCommandState>()
                .shutdown();
        }
    });
}
