use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EditorSelection {
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

impl Default for ThemePreference {
    fn default() -> Self {
        Self::System
    }
}

fn default_log_retention_days() -> u16 {
    7
}

fn default_restore_last_workspace() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_editor: Option<EditorSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_ide: Option<String>,
    #[serde(default)]
    pub workspace_editors: BTreeMap<String, EditorSelection>,
    #[serde(default)]
    pub muted_workspace_sections: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub ignored_workspace_issues: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub ignored_workspace_candidates: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub theme: ThemePreference,
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: u16,
    #[serde(default = "default_restore_last_workspace")]
    pub restore_last_workspace: bool,
    #[serde(default)]
    pub confirm_before_stop_all: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_workspace_id: Option<String>,
    #[serde(default)]
    pub pinned_workspace_ids: Vec<String>,
    #[serde(default)]
    pub recent_workspace_ids: Vec<String>,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            default_editor: None,
            default_ide: None,
            workspace_editors: BTreeMap::new(),
            muted_workspace_sections: BTreeMap::new(),
            ignored_workspace_issues: BTreeMap::new(),
            ignored_workspace_candidates: BTreeMap::new(),
            theme: ThemePreference::System,
            log_retention_days: default_log_retention_days(),
            restore_last_workspace: default_restore_last_workspace(),
            confirm_before_stop_all: false,
            last_workspace_id: None,
            pinned_workspace_ids: Vec::new(),
            recent_workspace_ids: Vec::new(),
        }
    }
}
