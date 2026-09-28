use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::models::preferences::AppPreferences;

const STORE_VERSION: u32 = 1;
const MAX_WORKSPACE_IDS: usize = 100;
const MAX_WORKSPACE_ID_LEN: usize = 256;
const MAX_WORKSPACE_NOTICES: usize = 200;
const MAX_NOTICE_ID_LEN: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreferencesStore {
    version: u32,
    preferences: AppPreferences,
}

impl Default for PreferencesStore {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            preferences: AppPreferences::default(),
        }
    }
}

pub fn load(path: &Path) -> Result<AppPreferences, String> {
    let backup = backup_path(path);
    let temp = temp_path(path);

    if path.exists() {
        match load_one(path) {
            Ok(preferences) => return Ok(preferences),
            Err(primary_error) => {
                if backup.exists() {
                    match load_one(&backup) {
                        Ok(preferences) => return Ok(preferences),
                        Err(backup_error) => {
                            if temp.exists() {
                                return load_one(&temp).map_err(|temp_error| {
                                    format!(
                                        "{primary_error} Backup recovery also failed: {backup_error} Temporary recovery also failed: {temp_error}"
                                    )
                                });
                            }
                            return Err(format!(
                                "{primary_error} Backup recovery also failed: {backup_error}"
                            ));
                        }
                    }
                }

                if temp.exists() {
                    return load_one(&temp).map_err(|temp_error| {
                        format!("{primary_error} Temporary recovery also failed: {temp_error}")
                    });
                }
                return Err(primary_error);
            }
        }
    }

    if backup.exists() {
        match load_one(&backup) {
            Ok(preferences) => return Ok(preferences),
            Err(backup_error) => {
                if temp.exists() {
                    return load_one(&temp).map_err(|temp_error| {
                        format!(
                            "Backup recovery failed: {backup_error} Temporary recovery also failed: {temp_error}"
                        )
                    });
                }
                return Err(format!("Backup recovery failed: {backup_error}"));
            }
        }
    }

    if temp.exists() {
        return load_one(&temp);
    }

    Ok(AppPreferences::default())
}

pub fn save(path: &Path, preferences: AppPreferences) -> Result<AppPreferences, String> {
    let preferences = normalize(preferences)?;
    let store = PreferencesStore {
        version: STORE_VERSION,
        preferences: preferences.clone(),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create DECC app-data directory: {error}"))?;
    }

    let bytes = serde_json::to_vec_pretty(&store)
        .map_err(|error| format!("Could not serialize DECC preferences: {error}"))?;
    let temp = temp_path(path);
    let backup = backup_path(path);
    {
        let mut file = File::create(&temp)
            .map_err(|error| format!("Could not create temporary preferences file: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("Could not write temporary preferences file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush temporary preferences file: {error}"))?;
    }

    if path.exists() && load_one(path).is_ok() {
        fs::copy(path, &backup)
            .map_err(|error| format!("Could not back up preferences file: {error}"))?;
    }

    replace_file(&temp, path)?;
    Ok(preferences)
}

fn load_one(path: &Path) -> Result<AppPreferences, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read DECC preferences: {error}"))?;
    let store: PreferencesStore = serde_json::from_slice(&bytes)
        .map_err(|error| format!("DECC preferences are invalid: {error}"))?;
    if store.version != STORE_VERSION {
        return Err(format!(
            "Unsupported preferences version {}. Expected {}.",
            store.version, STORE_VERSION
        ));
    }
    normalize(store.preferences)
}

fn normalize(mut preferences: AppPreferences) -> Result<AppPreferences, String> {
    if !matches!(preferences.log_retention_days, 7 | 14 | 30) {
        return Err("Log retention must be 7, 14, or 30 days.".to_string());
    }

    preferences.last_workspace_id = preferences
        .last_workspace_id
        .filter(|id| valid_workspace_id(id));

    preferences.pinned_workspace_ids = normalize_workspace_ids(preferences.pinned_workspace_ids);
    preferences.recent_workspace_ids = normalize_workspace_ids(preferences.recent_workspace_ids);
    preferences.muted_workspace_sections =
        normalize_workspace_notice_map(preferences.muted_workspace_sections, true);
    preferences.ignored_workspace_issues =
        normalize_workspace_notice_map(preferences.ignored_workspace_issues, false);
    preferences.ignored_workspace_candidates =
        normalize_workspace_notice_map(preferences.ignored_workspace_candidates, false);
    preferences
        .workspace_editors
        .retain(|workspace_id, editor| valid_workspace_id(workspace_id) && valid_editor(editor));
    if preferences
        .default_editor
        .as_ref()
        .is_some_and(|editor| !valid_editor(editor))
    {
        preferences.default_editor = None;
    }

    Ok(preferences)
}

fn valid_editor(editor: &crate::models::preferences::EditorSelection) -> bool {
    !editor.label.trim().is_empty()
        && editor.label.len() <= 160
        && editor.path.len() <= 4096
        && Path::new(&editor.path).is_absolute()
}

fn normalize_workspace_ids(ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| valid_workspace_id(id))
        .filter(|id| seen.insert(id.clone()))
        .take(MAX_WORKSPACE_IDS)
        .collect()
}

fn valid_workspace_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_WORKSPACE_ID_LEN
}

fn normalize_workspace_notice_map(
    values: BTreeMap<String, Vec<String>>,
    sections_only: bool,
) -> BTreeMap<String, Vec<String>> {
    values
        .into_iter()
        .filter(|(workspace_id, _)| valid_workspace_id(workspace_id))
        .filter_map(|(workspace_id, notices)| {
            let mut seen = HashSet::new();
            let notices = notices
                .into_iter()
                .filter(|notice| {
                    !notice.is_empty()
                        && notice.len() <= MAX_NOTICE_ID_LEN
                        && (!sections_only
                            || matches!(
                                notice.as_str(),
                                "overview" | "logs" | "environment" | "git"
                            ))
                })
                .filter(|notice| seen.insert(notice.clone()))
                .take(MAX_WORKSPACE_NOTICES)
                .collect::<Vec<_>>();
            (!notices.is_empty()).then_some((workspace_id, notices))
        })
        .collect()
}

fn temp_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(windows)]
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("Could not replace preferences file: {error}"))?;
    }

    fs::rename(source, destination)
        .map_err(|error| format!("Could not commit preferences file: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::models::preferences::{AppPreferences, EditorSelection, ThemePreference};

    use super::{backup_path, load, save, temp_path};

    fn fixture_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("decc-preferences-{name}-{unique}"))
            .join("preferences.v1.json")
    }

    #[test]
    fn round_trips_preferences_outside_the_repository_model() {
        let path = fixture_path("roundtrip");
        let preferences = AppPreferences {
            default_editor: Some(EditorSelection {
                label: "Editor".to_string(),
                path: if cfg!(windows) {
                    r"C:\Tools\Editor.exe".to_string()
                } else {
                    "/Applications/Editor.app".to_string()
                },
            }),
            default_ide: Some("VS Code".to_string()),
            workspace_editors: Default::default(),
            muted_workspace_sections: std::collections::BTreeMap::from([(
                "workspace-one".to_string(),
                vec!["logs".to_string(), "environment".to_string()],
            )]),
            ignored_workspace_issues: std::collections::BTreeMap::from([(
                "workspace-one".to_string(),
                vec!["runtime:api:node".to_string()],
            )]),
            ignored_workspace_candidates: std::collections::BTreeMap::from([(
                "workspace-one".to_string(),
                vec!["worker".to_string()],
            )]),
            theme: ThemePreference::Dark,
            log_retention_days: 14,
            restore_last_workspace: true,
            confirm_before_stop_all: true,
            last_workspace_id: Some("workspace-one".to_string()),
            pinned_workspace_ids: vec!["workspace-one".to_string()],
            recent_workspace_ids: vec!["workspace-two".to_string(), "workspace-one".to_string()],
        };

        let saved = save(&path, preferences.clone()).expect("save");
        assert_eq!(saved, preferences);
        assert_eq!(load(&path).expect("load"), preferences);

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn recovers_preferences_from_previous_valid_backup() {
        let path = fixture_path("backup-recovery");
        let mut first = AppPreferences::default();
        first.theme = ThemePreference::Dark;
        save(&path, first.clone()).expect("first save");

        let mut second = first.clone();
        second.theme = ThemePreference::Light;
        save(&path, second).expect("second save creates backup");

        fs::write(&path, b"{ broken").expect("corrupt primary");
        let recovered = load(&path).expect("recover backup");
        assert_eq!(recovered, first);
        assert!(backup_path(&path).exists());

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn recovers_preferences_from_valid_temp_after_interrupted_commit() {
        let path = fixture_path("temp-recovery");
        let mut preferences = AppPreferences::default();
        preferences.theme = ThemePreference::Dark;
        save(&path, preferences.clone()).expect("save");
        assert!(!backup_path(&path).exists());

        fs::copy(&path, temp_path(&path)).expect("preserve valid temp");
        fs::write(&path, b"{ broken").expect("corrupt primary");

        let recovered = load(&path).expect("recover temp");
        assert_eq!(recovered, preferences);

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn rejects_corrupt_primary_backup_and_temp_instead_of_resetting_silently() {
        let path = fixture_path("all-corrupt");
        fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
        fs::write(&path, b"{ broken primary").expect("primary");
        fs::write(backup_path(&path), b"{ broken backup").expect("backup");
        fs::write(temp_path(&path), b"{ broken temp").expect("temp");

        let error = load(&path).expect_err("all corrupt candidates must fail");
        assert!(error.contains("preferences are invalid"));
        assert!(error.contains("Backup recovery also failed"));
        assert!(error.contains("Temporary recovery also failed"));

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn normalizes_duplicate_and_invalid_workspace_navigation_ids() {
        let path = fixture_path("normalize");
        let mut preferences = AppPreferences::default();
        preferences.last_workspace_id = Some(String::new());
        preferences.pinned_workspace_ids = vec![
            "one".to_string(),
            "one".to_string(),
            String::new(),
            "two".to_string(),
        ];

        let saved = save(&path, preferences).expect("save");
        assert_eq!(saved.last_workspace_id, None);
        assert_eq!(
            saved.pinned_workspace_ids,
            vec!["one".to_string(), "two".to_string()]
        );

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn rejects_unsupported_retention_choice() {
        let path = fixture_path("retention");
        let mut preferences = AppPreferences::default();
        preferences.log_retention_days = 9;
        assert!(save(&path, preferences).is_err());
    }
}
