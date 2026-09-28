use std::sync::{Arc, Mutex};

use tauri::Manager;

use crate::{core::preferences, models::preferences::AppPreferences};

const PREFERENCES_STORE_FILE: &str = "preferences.v1.json";

#[derive(Clone, Default)]
pub struct PreferencesCommandState {
    store_lock: Arc<Mutex<()>>,
}

#[tauri::command]
pub async fn get_preferences(
    app: tauri::AppHandle,
    state: tauri::State<'_, PreferencesCommandState>,
) -> Result<AppPreferences, String> {
    let path = preferences_store_path(&app)?;
    let lock = state.store_lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock
            .lock()
            .map_err(|_| "Preferences store lock is unavailable.".to_string())?;
        preferences::load(&path)
    })
    .await
    .map_err(|error| format!("Preferences loading failed: {error}"))?
}

#[tauri::command]
pub async fn save_preferences(
    app: tauri::AppHandle,
    state: tauri::State<'_, PreferencesCommandState>,
    preferences: AppPreferences,
) -> Result<AppPreferences, String> {
    let path = preferences_store_path(&app)?;
    let lock = state.store_lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock
            .lock()
            .map_err(|_| "Preferences store lock is unavailable.".to_string())?;
        preferences::save(&path, preferences)
    })
    .await
    .map_err(|error| format!("Preferences save failed: {error}"))?
}

fn preferences_store_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(PREFERENCES_STORE_FILE))
        .map_err(|error| format!("Could not resolve DECC app-data directory: {error}"))
}
