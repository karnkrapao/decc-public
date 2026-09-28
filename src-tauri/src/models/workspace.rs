use serde::{Deserialize, Serialize};

use super::detection::DetectionConfidence;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedComponent {
    pub id: String,
    pub name: String,
    pub relative_path: String,
    pub technology: String,
    pub runtime: String,
    pub command: String,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub expected_ports: Vec<u16>,
    pub include_in_run_all: bool,
    pub detection_confidence: DetectionConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWorkspace {
    pub id: String,
    pub name: String,
    pub path: String,
    pub trusted: bool,
    pub components: Vec<PersistedComponent>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}
