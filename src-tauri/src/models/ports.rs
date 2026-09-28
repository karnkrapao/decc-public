use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PortListenerOrigin {
    Owned,
    External,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortListenerObservation {
    pub address: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub cwd: Option<String>,
    pub origin: PortListenerOrigin,
    pub matches_expected_port: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPortInspection {
    pub component_id: String,
    pub expected_ports: Vec<u16>,
    pub listeners: Vec<PortListenerObservation>,
    pub external_running: bool,
    pub has_conflict: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePortInspection {
    pub workspace_id: String,
    pub components: Vec<ComponentPortInspection>,
}
