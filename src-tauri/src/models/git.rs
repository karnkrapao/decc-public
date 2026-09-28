use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GitInspectionState {
    Ready,
    NotRepository,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GitChangeStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitChange {
    pub path: String,
    pub status: GitChangeStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommit {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub relative_time: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitHeadInspection {
    pub workspace_id: String,
    pub state: GitInspectionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub detached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitInspection {
    pub workspace_id: String,
    pub state: GitInspectionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub detached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changes: Vec<GitChange>,
    pub changes_truncated: bool,
    pub commits: Vec<GitCommit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl WorkspaceGitInspection {
    pub fn unavailable(workspace_id: String, message: impl Into<String>) -> Self {
        Self::empty(
            workspace_id,
            GitInspectionState::Unavailable,
            Some(message.into()),
        )
    }

    pub fn not_repository(workspace_id: String) -> Self {
        Self::empty(workspace_id, GitInspectionState::NotRepository, None)
    }

    pub fn error(workspace_id: String, message: impl Into<String>) -> Self {
        Self::empty(
            workspace_id,
            GitInspectionState::Error,
            Some(message.into()),
        )
    }

    fn empty(workspace_id: String, state: GitInspectionState, error: Option<String>) -> Self {
        Self {
            workspace_id,
            state,
            repository_root: None,
            branch: None,
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            changes: Vec::new(),
            changes_truncated: false,
            commits: Vec::new(),
            error,
        }
    }
}
