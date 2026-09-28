use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeDiagnosticStatus {
    Ready,
    Missing,
    Incompatible,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolRuntimeDiagnostic {
    pub key: String,
    pub label: String,
    pub program: String,
    pub status: RuntimeDiagnosticStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement_source: Option<String>,
    pub detail: String,
    pub blocking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRuntimeInspection {
    pub component_id: String,
    pub ready: bool,
    pub diagnostics: Vec<ToolRuntimeDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRuntimeInspection {
    pub workspace_id: String,
    pub components: Vec<ComponentRuntimeInspection>,
}
