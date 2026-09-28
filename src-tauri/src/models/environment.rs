use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentFileKind {
    Active,
    Example,
    Override,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentVariableStatus {
    Present,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentFileMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_name: Option<String>,
    pub name: String,
    pub relative_path: String,
    pub kind: EnvironmentFileKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVariableMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_name: Option<String>,
    pub name: String,
    pub status: EnvironmentVariableStatus,
    pub source_paths: Vec<String>,
    pub requirement_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEnvironmentInspection {
    pub workspace_id: String,
    pub files: Vec<EnvironmentFileMetadata>,
    pub variables: Vec<EnvironmentVariableMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
