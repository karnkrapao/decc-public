use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(windows)]
mod windows_process;

use crate::models::{
    ports::{
        ComponentPortInspection, PortListenerObservation, PortListenerOrigin,
        WorkspacePortInspection,
    },
    process::ProcessStatus,
    workspace::{PersistedComponent, PersistedWorkspace},
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawListener {
    address: String,
    port: u16,
    pid: Option<u32>,
    process_name: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ProcessSnapshot {
    parents: HashMap<u32, u32>,
    names: HashMap<u32, String>,
}

impl ProcessSnapshot {
    #[cfg(any(windows, test))]
    fn is_descendant_or_same(&self, pid: u32, root_pid: u32) -> bool {
        if pid == root_pid {
            return true;
        }

        let mut current = pid;
        let mut seen = HashSet::new();
        while seen.insert(current) {
            let Some(parent) = self.parents.get(&current).copied() else {
                return false;
            };
            if parent == root_pid {
                return true;
            }
            if parent == 0 || parent == current {
                return false;
            }
            current = parent;
        }
        false
    }
}

pub fn inspect(
    workspaces: &[PersistedWorkspace],
    owned_processes: &[ProcessStatus],
) -> Result<Vec<WorkspacePortInspection>, String> {
    #[cfg(windows)]
    {
        let process_snapshot = inspect_windows_processes()?;
        let listeners = inspect_listeners(&process_snapshot)?;
        return Ok(classify_with_process_snapshot(
            workspaces,
            owned_processes,
            &listeners,
            &process_snapshot,
        ));
    }

    #[cfg(not(windows))]
    {
        let listeners = inspect_listeners()?;
        Ok(classify(workspaces, owned_processes, &listeners))
    }
}

#[cfg(any(not(windows), test))]
fn classify(
    workspaces: &[PersistedWorkspace],
    owned_processes: &[ProcessStatus],
    listeners: &[RawListener],
) -> Vec<WorkspacePortInspection> {
    classify_with_process_snapshot(
        workspaces,
        owned_processes,
        listeners,
        &ProcessSnapshot::default(),
    )
}

fn classify_with_process_snapshot(
    workspaces: &[PersistedWorkspace],
    owned_processes: &[ProcessStatus],
    listeners: &[RawListener],
    process_snapshot: &ProcessSnapshot,
) -> Vec<WorkspacePortInspection> {
    let mut result = Vec::with_capacity(workspaces.len());

    for workspace in workspaces {
        let workspace_root = canonical_or_original(Path::new(&workspace.path));
        let mut components = Vec::with_capacity(workspace.components.len());

        for component in &workspace.components {
            let component_root = component_root(&workspace_root, component);
            let owned = owned_processes.iter().find(|process| {
                process.workspace_id == workspace.id && process.component_id == component.id
            });

            let mut observations = Vec::new();
            let mut seen = HashSet::new();

            for listener in listeners {
                let owned_by_component = listener
                    .pid
                    .zip(owned.map(|process| process.pid))
                    .is_some_and(|(listener_pid, owned_pid)| {
                        pid_belongs_to_owned(listener_pid, owned_pid, process_snapshot)
                    });
                let cwd_matches_component = listener
                    .cwd
                    .as_deref()
                    .is_some_and(|cwd| path_is_within(cwd, &component_root));
                let matches_expected_port = component.expected_ports.contains(&listener.port);

                if !owned_by_component && !cwd_matches_component && !matches_expected_port {
                    continue;
                }

                let key = (listener.port, listener.pid, listener.address.clone());
                if !seen.insert(key) {
                    continue;
                }

                observations.push(PortListenerObservation {
                    address: listener.address.clone(),
                    port: listener.port,
                    pid: listener.pid,
                    process_name: listener.process_name.clone(),
                    cwd: listener.cwd.clone(),
                    origin: if owned_by_component {
                        PortListenerOrigin::Owned
                    } else {
                        PortListenerOrigin::External
                    },
                    matches_expected_port,
                });
            }

            observations.sort_by(|a, b| {
                origin_rank(&a.origin)
                    .cmp(&origin_rank(&b.origin))
                    .then_with(|| a.port.cmp(&b.port))
                    .then_with(|| a.pid.cmp(&b.pid))
            });

            let external_running = observations.iter().any(|listener| {
                matches!(listener.origin, PortListenerOrigin::External)
                    && listener
                        .cwd
                        .as_deref()
                        .is_some_and(|cwd| path_is_within(cwd, &component_root))
            });
            let has_conflict = observations.iter().any(|listener| {
                matches!(listener.origin, PortListenerOrigin::External)
                    && listener.matches_expected_port
            });

            components.push(ComponentPortInspection {
                component_id: component.id.clone(),
                expected_ports: component.expected_ports.clone(),
                listeners: observations,
                external_running,
                has_conflict,
            });
        }

        result.push(WorkspacePortInspection {
            workspace_id: workspace.id.clone(),
            components,
        });
    }

    result
}

fn origin_rank(origin: &PortListenerOrigin) -> u8 {
    match origin {
        PortListenerOrigin::Owned => 0,
        PortListenerOrigin::External => 1,
    }
}

fn component_root(workspace_root: &Path, component: &PersistedComponent) -> PathBuf {
    if component.relative_path == "." {
        workspace_root.to_path_buf()
    } else {
        canonical_or_original(&workspace_root.join(&component.relative_path))
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn path_is_within(path: &str, root: &Path) -> bool {
    let candidate = canonical_or_original(Path::new(path));
    candidate.starts_with(root)
}

#[cfg(unix)]
fn pid_belongs_to_owned(
    listener_pid: u32,
    owned_pid: u32,
    _process_snapshot: &ProcessSnapshot,
) -> bool {
    if listener_pid == owned_pid {
        return true;
    }
    let pgid = unsafe { libc::getpgid(listener_pid as libc::pid_t) };
    pgid > 0 && pgid as u32 == owned_pid
}

#[cfg(windows)]
fn pid_belongs_to_owned(
    listener_pid: u32,
    owned_pid: u32,
    process_snapshot: &ProcessSnapshot,
) -> bool {
    process_snapshot.is_descendant_or_same(listener_pid, owned_pid)
}

#[cfg(not(any(unix, windows)))]
fn pid_belongs_to_owned(
    listener_pid: u32,
    owned_pid: u32,
    _process_snapshot: &ProcessSnapshot,
) -> bool {
    listener_pid == owned_pid
}

#[cfg(target_os = "macos")]
fn inspect_listeners() -> Result<Vec<RawListener>, String> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"])
        .output()
        .map_err(|error| format!("Could not inspect listening TCP ports with lsof: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "lsof port inspection failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let mut listeners = parse_lsof_listeners(&String::from_utf8_lossy(&output.stdout));
    let mut pids = listeners
        .iter()
        .filter_map(|listener| listener.pid)
        .collect::<Vec<_>>();
    pids.sort_unstable();
    pids.dedup();

    if !pids.is_empty() {
        let pid_list = pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        if let Ok(cwd_output) = Command::new("/usr/sbin/lsof")
            .args(["-nP", "-a", "-p", &pid_list, "-d", "cwd", "-Fpcn"])
            .output()
        {
            let cwd_map = parse_lsof_cwds(&String::from_utf8_lossy(&cwd_output.stdout));
            for listener in &mut listeners {
                if let Some(pid) = listener.pid {
                    listener.cwd = cwd_map.get(&pid).cloned();
                }
            }
        }
    }
    Ok(listeners)
}

#[cfg(target_os = "linux")]
fn inspect_listeners() -> Result<Vec<RawListener>, String> {
    let output = run_first_available(&[
        ("/usr/bin/ss", &["-ltnpH"]),
        ("/bin/ss", &["-ltnpH"]),
        ("ss", &["-ltnpH"]),
    ])?;
    let mut listeners = parse_ss_listeners(&String::from_utf8_lossy(&output.stdout));
    for listener in &mut listeners {
        if let Some(pid) = listener.pid {
            listener.cwd = std::fs::read_link(format!("/proc/{pid}/cwd"))
                .ok()
                .map(|path| path.to_string_lossy().into_owned());
            if listener.process_name.is_none() {
                listener.process_name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                    .ok()
                    .map(|name| name.trim().to_string());
            }
        }
    }
    Ok(listeners)
}

#[cfg(windows)]
fn inspect_listeners(process_snapshot: &ProcessSnapshot) -> Result<Vec<RawListener>, String> {
    let output = Command::new("netstat.exe")
        .args(["-ano", "-p", "tcp"])
        .output()
        .map_err(|error| format!("Could not inspect listening TCP ports with netstat: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "netstat port inspection failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut listeners = parse_netstat_listeners(&String::from_utf8_lossy(&output.stdout));

    for listener in &mut listeners {
        if let Some(pid) = listener.pid {
            listener.process_name = process_snapshot.names.get(&pid).cloned();
        }
    }
    Ok(listeners)
}

#[cfg(windows)]
fn inspect_windows_processes() -> Result<ProcessSnapshot, String> {
    let (parents, names) = windows_process::inspect()?;
    Ok(ProcessSnapshot { parents, names })
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn inspect_listeners() -> Result<Vec<RawListener>, String> {
    Err("Port inspection is not implemented on this platform.".to_string())
}

#[cfg(target_os = "linux")]
fn run_first_available(candidates: &[(&str, &[&str])]) -> Result<std::process::Output, String> {
    let mut last_error = None;
    for (program, args) in candidates {
        match Command::new(program).args(*args).output() {
            Ok(output) if output.status.success() => return Ok(output),
            Ok(output) => {
                last_error = Some(format!(
                    "{program} failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            Err(error) => last_error = Some(format!("{program}: {error}")),
        }
    }
    Err(last_error
        .unwrap_or_else(|| "No supported TCP listener inspector is available.".to_string()))
}

#[cfg(any(target_os = "macos", test))]
fn parse_lsof_listeners(text: &str) -> Vec<RawListener> {
    let mut listeners = Vec::new();
    let mut pid = None;
    let mut process_name = None;

    for line in text.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse::<u32>().ok();
            process_name = None;
        } else if let Some(value) = line.strip_prefix('c') {
            process_name = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix('n') {
            if let Some((address, port)) = split_address_port(value) {
                listeners.push(RawListener {
                    address,
                    port,
                    pid,
                    process_name: process_name.clone(),
                    cwd: None,
                });
            }
        }
    }
    listeners
}

#[cfg(any(target_os = "macos", test))]
fn parse_lsof_cwds(text: &str) -> HashMap<u32, String> {
    let mut result = HashMap::new();
    let mut pid = None;
    let mut cwd_record = false;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse::<u32>().ok();
            cwd_record = false;
        } else if line == "fcwd" {
            cwd_record = true;
        } else if cwd_record {
            if let (Some(pid), Some(value)) = (pid, line.strip_prefix('n')) {
                result.insert(pid, value.to_string());
            }
            cwd_record = false;
        }
    }
    result
}

#[cfg(target_os = "linux")]
fn parse_ss_listeners(text: &str) -> Vec<RawListener> {
    text.lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let local = *fields.get(3)?;
            let (address, port) = split_address_port(local)?;
            let pid = line
                .split("pid=")
                .nth(1)
                .and_then(|rest| {
                    rest.split(|character: char| !character.is_ascii_digit())
                        .next()
                })
                .and_then(|value| value.parse::<u32>().ok());
            let process_name = line
                .split("((\"")
                .nth(1)
                .and_then(|rest| rest.split("\"").next())
                .map(str::to_string);
            Some(RawListener {
                address,
                port,
                pid,
                process_name,
                cwd: None,
            })
        })
        .collect()
}

#[cfg(windows)]
fn parse_netstat_listeners(text: &str) -> Vec<RawListener> {
    text.lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 5 || !fields[0].eq_ignore_ascii_case("TCP") {
                return None;
            }
            if !fields[3].eq_ignore_ascii_case("LISTENING") {
                return None;
            }
            let (address, port) = split_address_port(fields[1])?;
            let pid = fields[4].parse::<u32>().ok();
            Some(RawListener {
                address,
                port,
                pid,
                process_name: None,
                cwd: None,
            })
        })
        .collect()
}

fn split_address_port(value: &str) -> Option<(String, u16)> {
    let (address, port) = value.rsplit_once(':')?;
    let port = port.parse::<u16>().ok()?;
    Some((address.to_string(), port))
}

#[cfg(test)]
mod tests {
    use crate::models::{
        detection::DetectionConfidence,
        ports::PortListenerOrigin,
        process::ProcessStatus,
        workspace::{PersistedComponent, PersistedWorkspace},
    };

    #[cfg(windows)]
    use super::classify_with_process_snapshot;
    use super::{
        classify, parse_lsof_cwds, parse_lsof_listeners, split_address_port, ProcessSnapshot,
        RawListener,
    };

    fn workspace(expected_ports: Vec<u16>) -> PersistedWorkspace {
        PersistedWorkspace {
            id: "workspace".to_string(),
            name: "Workspace".to_string(),
            path: "/tmp/decc-port-workspace".to_string(),
            trusted: true,
            components: vec![PersistedComponent {
                id: "web".to_string(),
                name: "Web".to_string(),
                relative_path: ".".to_string(),
                technology: "Vite".to_string(),
                runtime: "Node.js".to_string(),
                command: "npm run dev".to_string(),
                program: Some("npm".to_string()),
                args: vec!["run".to_string(), "dev".to_string()],
                expected_ports,
                include_in_run_all: true,
                detection_confidence: DetectionConfidence::High,
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn parses_macos_lsof_listener_and_cwd_output() {
        let listeners =
            parse_lsof_listeners("p8211\ncnode\nf20\nn127.0.0.1:3000\nf21\nn[::1]:3000\n");
        assert_eq!(listeners.len(), 2);
        assert_eq!(listeners[0].pid, Some(8211));
        assert_eq!(listeners[0].process_name.as_deref(), Some("node"));
        assert_eq!(listeners[0].port, 3000);

        let cwds = parse_lsof_cwds("p8211\ncnode\nfcwd\nn/Users/test/project\n");
        assert_eq!(
            cwds.get(&8211).map(String::as_str),
            Some("/Users/test/project")
        );
    }

    #[test]
    fn parses_ipv4_ipv6_and_wildcard_addresses() {
        assert_eq!(
            split_address_port("127.0.0.1:5173"),
            Some(("127.0.0.1".to_string(), 5173))
        );
        assert_eq!(
            split_address_port("[::1]:8080"),
            Some(("[::1]".to_string(), 8080))
        );
        assert_eq!(split_address_port("*:3000"), Some(("*".to_string(), 3000)));
    }

    #[test]
    fn classifies_external_workspace_listener_and_expected_port_conflict() {
        let workspace = workspace(vec![3000]);
        let listeners = vec![RawListener {
            address: "127.0.0.1".to_string(),
            port: 3000,
            pid: Some(9000),
            process_name: Some("node".to_string()),
            cwd: Some("/tmp/decc-port-workspace".to_string()),
        }];

        let result = classify(&[workspace], &[], &listeners);
        let component = &result[0].components[0];
        assert!(component.external_running);
        assert!(component.has_conflict);
        assert_eq!(component.listeners.len(), 1);
        assert_eq!(component.listeners[0].origin, PortListenerOrigin::External);
        assert!(component.listeners[0].matches_expected_port);
    }

    #[test]
    fn classifies_exact_owned_pid_without_marking_conflict() {
        let workspace = workspace(vec![]);
        let owned = vec![ProcessStatus {
            workspace_id: "workspace".to_string(),
            component_id: "web".to_string(),
            component_name: "Web".to_string(),
            pid: 4242,
            started_at_ms: 1,
        }];
        let listeners = vec![RawListener {
            address: "*".to_string(),
            port: 5173,
            pid: Some(4242),
            process_name: Some("node".to_string()),
            cwd: None,
        }];

        let result = classify(&[workspace], &owned, &listeners);
        let component = &result[0].components[0];
        assert!(!component.external_running);
        assert!(!component.has_conflict);
        assert_eq!(component.listeners[0].origin, PortListenerOrigin::Owned);
        assert_eq!(component.listeners[0].port, 5173);
    }

    #[test]
    fn process_snapshot_tracks_multi_level_descendants_and_rejects_cycles() {
        let mut snapshot = ProcessSnapshot::default();
        snapshot.parents.insert(5002, 5001);
        snapshot.parents.insert(5001, 5000);
        snapshot.parents.insert(7000, 7001);
        snapshot.parents.insert(7001, 7000);

        assert!(snapshot.is_descendant_or_same(5000, 5000));
        assert!(snapshot.is_descendant_or_same(5001, 5000));
        assert!(snapshot.is_descendant_or_same(5002, 5000));
        assert!(!snapshot.is_descendant_or_same(5002, 9000));
        assert!(!snapshot.is_descendant_or_same(7000, 9000));
    }

    #[cfg(windows)]
    #[test]
    fn windows_descendant_listener_is_owned_without_false_port_conflict() {
        let workspace = workspace(vec![5173]);
        let owned = vec![ProcessStatus {
            workspace_id: "workspace".to_string(),
            component_id: "web".to_string(),
            component_name: "Web".to_string(),
            pid: 5000,
            started_at_ms: 1,
        }];
        let listeners = vec![RawListener {
            address: "127.0.0.1".to_string(),
            port: 5173,
            pid: Some(5002),
            process_name: Some("node.exe".to_string()),
            cwd: None,
        }];
        let mut snapshot = ProcessSnapshot::default();
        snapshot.parents.insert(5002, 5001);
        snapshot.parents.insert(5001, 5000);

        let result = classify_with_process_snapshot(&[workspace], &owned, &listeners, &snapshot);
        let component = &result[0].components[0];

        assert_eq!(component.listeners[0].origin, PortListenerOrigin::Owned);
        assert!(!component.external_running);
        assert!(!component.has_conflict);
    }
}
