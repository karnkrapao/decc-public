use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::models::workspace::PersistedWorkspace;

const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStore {
    pub version: u32,
    pub workspaces: Vec<PersistedWorkspace>,
}

impl Default for WorkspaceStore {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            workspaces: Vec::new(),
        }
    }
}

impl WorkspaceStore {
    pub fn load(path: &Path) -> Result<Self, String> {
        let backup = backup_path(path);
        let temp = temp_path(path);

        if path.exists() {
            match load_one(path) {
                Ok(store) => return Ok(store),
                Err(primary_error) => {
                    if backup.exists() {
                        match load_one(&backup) {
                            Ok(store) => return Ok(store),
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
                Ok(store) => return Ok(store),
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

        Ok(Self::default())
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create DECC app-data directory: {error}"))?;
        }

        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("Could not serialize DECC workspace store: {error}"))?;
        let temp = temp_path(path);
        let backup = backup_path(path);

        {
            let mut file = File::create(&temp)
                .map_err(|error| format!("Could not create temporary workspace store: {error}"))?;
            file.write_all(&bytes)
                .map_err(|error| format!("Could not write temporary workspace store: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("Could not flush temporary workspace store: {error}"))?;
        }

        if path.exists() && load_one(path).is_ok() {
            fs::copy(path, &backup)
                .map_err(|error| format!("Could not back up workspace store: {error}"))?;
        }

        replace_file(&temp, path)?;
        Ok(())
    }
}

fn load_one(path: &Path) -> Result<WorkspaceStore, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read DECC workspace store: {error}"))?;
    let store: WorkspaceStore = serde_json::from_slice(&bytes)
        .map_err(|error| format!("DECC workspace store is invalid: {error}"))?;

    if store.version != STORE_VERSION {
        return Err(format!(
            "Unsupported workspace store version {}. Expected {}.",
            store.version, STORE_VERSION
        ));
    }

    Ok(store)
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
            .map_err(|error| format!("Could not replace workspace store: {error}"))?;
    }

    fs::rename(source, destination)
        .map_err(|error| format!("Could not commit workspace store: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::models::workspace::PersistedWorkspace;

    use super::{backup_path, temp_path, WorkspaceStore};

    fn fixture_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("decc-store-{name}-{unique}"))
            .join("workspaces.v1.json")
    }

    #[test]
    fn recovers_from_backup_when_primary_is_corrupted() {
        let path = fixture_path("recovery");
        let first = WorkspaceStore {
            version: 1,
            workspaces: vec![PersistedWorkspace {
                id: "one".to_string(),
                name: "One".to_string(),
                path: "/tmp/one".to_string(),
                trusted: false,
                components: vec![],
                created_at_ms: 1,
                updated_at_ms: 1,
            }],
        };
        first.save(&path).expect("first save");

        let mut second = first.clone();
        second.workspaces[0].trusted = true;
        second.workspaces[0].updated_at_ms = 2;
        second.save(&path).expect("second save creates backup");

        fs::write(&path, b"{ broken").expect("corrupt primary");
        let recovered = WorkspaceStore::load(&path).expect("recover backup");
        assert_eq!(recovered, first);
        assert!(backup_path(&path).exists());

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn rejects_corrupt_primary_backup_and_temp_instead_of_resetting_silently() {
        let path = fixture_path("all-corrupt");
        fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
        fs::write(&path, b"{ broken primary").expect("primary");
        fs::write(backup_path(&path), b"{ broken backup").expect("backup");
        fs::write(temp_path(&path), b"{ broken temp").expect("temp");

        let error = WorkspaceStore::load(&path).expect_err("all corrupt candidates must fail");
        assert!(error.contains("workspace store is invalid"));
        assert!(error.contains("Backup recovery also failed"));
        assert!(error.contains("Temporary recovery also failed"));

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn recovers_from_valid_temp_when_primary_is_corrupted_before_backup_exists() {
        let path = fixture_path("temp-recovery");
        let store = WorkspaceStore {
            version: 1,
            workspaces: vec![PersistedWorkspace {
                id: "one".to_string(),
                name: "One".to_string(),
                path: "/tmp/one".to_string(),
                trusted: true,
                components: vec![],
                created_at_ms: 1,
                updated_at_ms: 1,
            }],
        };
        store.save(&path).expect("initial save");
        assert!(!backup_path(&path).exists());

        fs::copy(&path, temp_path(&path)).expect("preserve valid temp");
        fs::write(&path, b"{ broken").expect("corrupt primary");

        let recovered = WorkspaceStore::load(&path).expect("recover temp");
        assert_eq!(recovered, store);

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }
}
