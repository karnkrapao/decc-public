use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::models::process::{
    ProcessEvent, ProcessEventKind, ProcessHistory, RunSessionMetadata, RunSessionStatus,
};

const MAX_SEGMENT_BYTES: u64 = 1024 * 1024;
const MAX_SEGMENTS_PER_SESSION: usize = 4;
const MAX_SESSIONS_PER_WORKSPACE: usize = 100;
const MAX_PERSISTED_MESSAGE_BYTES: usize = 64 * 1024;
const DEFAULT_HISTORY_EVENTS: usize = 2_000;
const DEFAULT_RETENTION_DAYS: u16 = 7;

#[derive(Debug)]
struct LogState {
    retention_days: u16,
    active_sessions: HashMap<(String, String), String>,
    next_session_nonce: u64,
}

impl Default for LogState {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
            active_sessions: HashMap::new(),
            next_session_nonce: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LocalLogStore {
    state: Arc<Mutex<LogState>>,
}

impl LocalLogStore {
    pub fn record(&self, root: &Path, event: &ProcessEvent) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Local log store lock is unavailable.".to_string())?;

        let key = (event.workspace_id.clone(), event.component_id.clone());
        let session_id = if matches!(event.kind, ProcessEventKind::Starting) {
            let id = next_session_id(&mut state, event);
            state.active_sessions.insert(key.clone(), id.clone());
            id
        } else if let Some(id) = state.active_sessions.get(&key).cloned() {
            id
        } else {
            let id = next_session_id(&mut state, event);
            state.active_sessions.insert(key.clone(), id.clone());
            id
        };

        let workspace_dir = workspace_dir(root, &event.workspace_id);
        let session_dir = workspace_dir.join(&session_id);
        fs::create_dir_all(&session_dir)
            .map_err(|error| format!("Could not create local log session directory: {error}"))?;

        let metadata_path = session_dir.join("metadata.json");
        let mut metadata = if metadata_path.exists() {
            read_metadata(&metadata_path)?
        } else {
            RunSessionMetadata {
                id: session_id.clone(),
                workspace_id: event.workspace_id.clone(),
                component_id: event.component_id.clone(),
                component_name: event.component_name.clone(),
                pid: event.pid,
                started_at_ms: event.timestamp_ms,
                ended_at_ms: None,
                exit_code: None,
                status: initial_status(event),
                truncated: false,
            }
        };

        metadata.component_name = event.component_name.clone();
        if event.pid.is_some() {
            metadata.pid = event.pid;
        }

        match event.kind {
            ProcessEventKind::Starting => {
                metadata.status = RunSessionStatus::Starting;
                metadata.started_at_ms = event.timestamp_ms;
                metadata.ended_at_ms = None;
                metadata.exit_code = None;
            }
            ProcessEventKind::Started => {
                metadata.status = RunSessionStatus::Running;
                metadata.ended_at_ms = None;
                metadata.exit_code = None;
            }
            ProcessEventKind::Stopping | ProcessEventKind::Log => {
                if matches!(metadata.status, RunSessionStatus::Starting) {
                    metadata.status = RunSessionStatus::Running;
                }
            }
            ProcessEventKind::Exited => {
                metadata.status = RunSessionStatus::Exited;
                metadata.ended_at_ms = Some(event.timestamp_ms);
                metadata.exit_code = event.exit_code;
            }
            ProcessEventKind::Error => {
                metadata.status = RunSessionStatus::Error;
                metadata.ended_at_ms = Some(event.timestamp_ms);
                metadata.exit_code = event.exit_code;
            }
        }

        if !metadata.truncated {
            let persisted = persisted_event(event);
            let encoded = serde_json::to_vec(&persisted)
                .map_err(|error| format!("Could not serialize local process event: {error}"))?;

            if !append_event(&session_dir, &encoded)? {
                metadata.truncated = true;
            }
        }

        write_metadata(&metadata_path, &metadata)?;

        if matches!(
            event.kind,
            ProcessEventKind::Exited | ProcessEventKind::Error
        ) {
            state.active_sessions.remove(&key);
        }

        enforce_session_count(&workspace_dir)?;
        Ok(())
    }

    pub fn history(
        &self,
        root: &Path,
        workspace_id: &str,
        max_events: Option<usize>,
        session_id: Option<&str>,
    ) -> Result<ProcessHistory, String> {
        let workspace_dir = workspace_dir(root, workspace_id);
        if !workspace_dir.exists() {
            return Ok(ProcessHistory::default());
        }

        let mut sessions = load_sessions(&workspace_dir)?;
        sessions.sort_by(|left, right| {
            right
                .started_at_ms
                .cmp(&left.started_at_ms)
                .then_with(|| right.id.cmp(&left.id))
        });

        let limit = max_events.unwrap_or(DEFAULT_HISTORY_EVENTS);
        let mut events = if let Some(session_id) = session_id {
            let Some(session) = sessions.iter().find(|session| session.id == session_id) else {
                return Ok(ProcessHistory {
                    sessions,
                    events: Vec::new(),
                });
            };
            read_session_events(&workspace_dir.join(&session.id))?
        } else {
            let mut combined = Vec::new();
            for session in &sessions {
                combined.extend(read_session_events(&workspace_dir.join(&session.id))?);
            }
            combined
        };

        events.sort_by(|left, right| {
            right
                .timestamp_ms
                .cmp(&left.timestamp_ms)
                .then_with(|| right.component_id.cmp(&left.component_id))
        });
        events.truncate(limit);

        Ok(ProcessHistory { sessions, events })
    }

    pub fn clear_workspace(&self, root: &Path, workspace_id: &str) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Local log store lock is unavailable.".to_string())?;
        state
            .active_sessions
            .retain(|(stored_workspace_id, _), _| stored_workspace_id != workspace_id);

        let path = workspace_dir(root, workspace_id);
        if path.exists() {
            fs::remove_dir_all(&path)
                .map_err(|error| format!("Could not clear local process history: {error}"))?;
        }
        Ok(())
    }

    pub fn set_retention_days(&self, days: u16) -> Result<(), String> {
        if !matches!(days, 7 | 14 | 30) {
            return Err("Log retention must be 7, 14, or 30 days.".to_string());
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| "Local log store lock is unavailable.".to_string())?;
        state.retention_days = days;
        Ok(())
    }

    pub fn prune_all(&self, root: &Path) -> Result<usize, String> {
        let retention_days = self
            .state
            .lock()
            .map_err(|_| "Local log store lock is unavailable.".to_string())?
            .retention_days;

        if !root.exists() {
            return Ok(0);
        }

        let now_ms = unix_time_ms()?;
        let cutoff_ms = now_ms.saturating_sub(u64::from(retention_days) * 24 * 60 * 60 * 1_000);
        let mut removed = 0usize;

        for entry in fs::read_dir(root)
            .map_err(|error| format!("Could not inspect local process history root: {error}"))?
        {
            let entry = entry
                .map_err(|error| format!("Could not inspect local process history: {error}"))?;
            if !entry
                .file_type()
                .map_err(|error| format!("Could not inspect local process history item: {error}"))?
                .is_dir()
            {
                continue;
            }

            removed += prune_workspace_by_age(&entry.path(), cutoff_ms)?;
            enforce_session_count(&entry.path())?;
        }

        Ok(removed)
    }
}

fn next_session_id(state: &mut LogState, event: &ProcessEvent) -> String {
    state.next_session_nonce = state.next_session_nonce.wrapping_add(1);
    format!(
        "{}-{}-{}",
        event.timestamp_ms,
        event.pid.unwrap_or(0),
        state.next_session_nonce
    )
}

fn workspace_dir(root: &Path, workspace_id: &str) -> PathBuf {
    root.join(hex_encode(workspace_id.as_bytes()))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn initial_status(event: &ProcessEvent) -> RunSessionStatus {
    match event.kind {
        ProcessEventKind::Starting => RunSessionStatus::Starting,
        ProcessEventKind::Started | ProcessEventKind::Stopping | ProcessEventKind::Log => {
            RunSessionStatus::Running
        }
        ProcessEventKind::Exited => RunSessionStatus::Exited,
        ProcessEventKind::Error => RunSessionStatus::Error,
    }
}

fn persisted_event(event: &ProcessEvent) -> ProcessEvent {
    let mut persisted = event.clone();
    persisted.message = truncate_utf8(&persisted.message, MAX_PERSISTED_MESSAGE_BYTES);
    persisted
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn append_event(session_dir: &Path, encoded: &[u8]) -> Result<bool, String> {
    let record_bytes = encoded.len() as u64 + 1;
    let mut segment_index = latest_segment_index(session_dir)?.unwrap_or(0);
    let mut target_segment = segment_path(session_dir, segment_index);

    if target_segment.exists() {
        let length = fs::metadata(&target_segment)
            .map_err(|error| format!("Could not inspect local log segment: {error}"))?
            .len();
        if length.saturating_add(record_bytes) > MAX_SEGMENT_BYTES {
            segment_index += 1;
            if segment_index >= MAX_SEGMENTS_PER_SESSION {
                return Ok(false);
            }
            target_segment = segment_path(session_dir, segment_index);
        }
    }

    if record_bytes > MAX_SEGMENT_BYTES {
        return Ok(false);
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target_segment)
        .map_err(|error| format!("Could not open local log segment: {error}"))?;
    file.write_all(encoded)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|error| format!("Could not append local process event: {error}"))?;
    Ok(true)
}

fn latest_segment_index(session_dir: &Path) -> Result<Option<usize>, String> {
    let mut latest = None;
    for entry in fs::read_dir(session_dir)
        .map_err(|error| format!("Could not inspect local log session: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("Could not inspect local log session item: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("Could not inspect local log segment type: {error}"))?
            .is_file()
        {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(index) = name
            .strip_prefix("events-")
            .and_then(|value| value.strip_suffix(".jsonl"))
            .and_then(|value| value.parse::<usize>().ok())
        else {
            continue;
        };
        latest = Some(latest.map_or(index, |current: usize| current.max(index)));
    }
    Ok(latest)
}

fn segment_path(session_dir: &Path, index: usize) -> PathBuf {
    session_dir.join(format!("events-{index:03}.jsonl"))
}

fn write_metadata(path: &Path, metadata: &RunSessionMetadata) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(metadata)
        .map_err(|error| format!("Could not serialize run-session metadata: {error}"))?;
    let temp = path.with_extension("json.tmp");

    {
        let mut file = File::create(&temp)
            .map_err(|error| format!("Could not create run-session metadata: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("Could not write run-session metadata: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush run-session metadata: {error}"))?;
    }

    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("Could not replace run-session metadata: {error}"))?;
    }

    fs::rename(&temp, path)
        .map_err(|error| format!("Could not commit run-session metadata: {error}"))
}

fn read_metadata(path: &Path) -> Result<RunSessionMetadata, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read run-session metadata: {error}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Run-session metadata is invalid: {error}"))
}

fn load_sessions(workspace_dir: &Path) -> Result<Vec<RunSessionMetadata>, String> {
    let mut sessions = Vec::new();
    for entry in fs::read_dir(workspace_dir)
        .map_err(|error| format!("Could not inspect workspace log history: {error}"))?
    {
        let entry = entry
            .map_err(|error| format!("Could not inspect workspace log history item: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("Could not inspect workspace log history type: {error}"))?
            .is_dir()
        {
            continue;
        }

        let metadata_path = entry.path().join("metadata.json");
        if !metadata_path.exists() {
            continue;
        }
        sessions.push(read_metadata(&metadata_path)?);
    }
    Ok(sessions)
}

fn read_session_events(session_dir: &Path) -> Result<Vec<ProcessEvent>, String> {
    let mut segment_paths = Vec::new();
    for entry in fs::read_dir(session_dir)
        .map_err(|error| format!("Could not inspect run-session history: {error}"))?
    {
        let entry = entry
            .map_err(|error| format!("Could not inspect run-session history item: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("Could not inspect run-session history type: {error}"))?
            .is_file()
        {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("events-") && name.ends_with(".jsonl") {
            segment_paths.push(entry.path());
        }
    }
    segment_paths.sort();

    let mut events = Vec::new();
    for path in segment_paths {
        let file = File::open(&path)
            .map_err(|error| format!("Could not open local process history: {error}"))?;
        for line in BufReader::new(file).lines() {
            let line =
                line.map_err(|error| format!("Could not read local process history: {error}"))?;
            if line.trim().is_empty() {
                continue;
            }
            events.push(
                serde_json::from_str::<ProcessEvent>(&line)
                    .map_err(|error| format!("Local process history is invalid: {error}"))?,
            );
        }
    }
    Ok(events)
}

fn enforce_session_count(workspace_dir: &Path) -> Result<(), String> {
    if !workspace_dir.exists() {
        return Ok(());
    }

    let mut sessions = load_sessions(workspace_dir)?;
    if sessions.len() <= MAX_SESSIONS_PER_WORKSPACE {
        return Ok(());
    }

    sessions.sort_by(|left, right| {
        left.started_at_ms
            .cmp(&right.started_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut remaining = sessions.len();
    for session in sessions {
        if remaining <= MAX_SESSIONS_PER_WORKSPACE {
            break;
        }
        if matches!(
            session.status,
            RunSessionStatus::Starting | RunSessionStatus::Running
        ) {
            continue;
        }

        let path = workspace_dir.join(&session.id);
        if path.exists() {
            fs::remove_dir_all(&path)
                .map_err(|error| format!("Could not prune local run session: {error}"))?;
            remaining -= 1;
        }
    }

    Ok(())
}

fn prune_workspace_by_age(workspace_dir: &Path, cutoff_ms: u64) -> Result<usize, String> {
    let sessions = load_sessions(workspace_dir)?;
    let mut removed = 0usize;

    for session in sessions {
        if matches!(
            session.status,
            RunSessionStatus::Starting | RunSessionStatus::Running
        ) {
            continue;
        }
        let terminal_at = session.ended_at_ms.unwrap_or(session.started_at_ms);
        if terminal_at >= cutoff_ms {
            continue;
        }

        let path = workspace_dir.join(&session.id);
        if path.exists() {
            fs::remove_dir_all(&path)
                .map_err(|error| format!("Could not prune expired run session: {error}"))?;
            removed += 1;
        }
    }

    Ok(removed)
}

fn unix_time_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|error| format!("System clock is before the Unix epoch: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::models::process::{ProcessEvent, ProcessEventKind, ProcessStream, RunSessionStatus};

    use super::{LocalLogStore, MAX_PERSISTED_MESSAGE_BYTES, MAX_SEGMENTS_PER_SESSION};

    fn fixture_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-logs-{name}-{unique}"));
        fs::create_dir_all(&root).expect("root");
        root
    }

    fn event(
        kind: ProcessEventKind,
        timestamp_ms: u64,
        message: impl Into<String>,
        pid: Option<u32>,
        exit_code: Option<i32>,
    ) -> ProcessEvent {
        ProcessEvent {
            workspace_id: "workspace".to_string(),
            component_id: "api".to_string(),
            component_name: "API".to_string(),
            kind,
            stream: Some(ProcessStream::System),
            message: message.into(),
            pid,
            exit_code,
            timestamp_ms,
        }
    }

    #[test]
    fn persists_and_reads_run_session_history() {
        let root = fixture_root("history");
        let logs = LocalLogStore::default();

        logs.record(
            &root,
            &event(ProcessEventKind::Starting, 100, "starting", None, None),
        )
        .expect("starting");
        logs.record(
            &root,
            &event(ProcessEventKind::Started, 110, "started", Some(42), None),
        )
        .expect("started");
        logs.record(
            &root,
            &event(ProcessEventKind::Log, 120, "hello", Some(42), None),
        )
        .expect("log");
        logs.record(
            &root,
            &event(ProcessEventKind::Exited, 130, "done", Some(42), Some(0)),
        )
        .expect("exited");

        let history = logs
            .history(&root, "workspace", Some(20), None)
            .expect("history");
        assert_eq!(history.sessions.len(), 1);
        assert_eq!(history.sessions[0].status, RunSessionStatus::Exited);
        assert_eq!(history.sessions[0].pid, Some(42));
        assert_eq!(history.sessions[0].exit_code, Some(0));
        assert_eq!(history.events.len(), 4);
        assert_eq!(history.events[0].timestamp_ms, 130);
        assert_eq!(history.events[3].timestamp_ms, 100);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clear_workspace_removes_persisted_history() {
        let root = fixture_root("clear");
        let logs = LocalLogStore::default();

        logs.record(
            &root,
            &event(ProcessEventKind::Started, 100, "started", Some(42), None),
        )
        .expect("started");
        assert_eq!(
            logs.history(&root, "workspace", None, None)
                .expect("before")
                .sessions
                .len(),
            1
        );

        logs.clear_workspace(&root, "workspace").expect("clear");
        assert!(logs
            .history(&root, "workspace", None, None)
            .expect("after")
            .sessions
            .is_empty());

        logs.record(
            &root,
            &event(ProcessEventKind::Log, 200, "continued", Some(42), None),
        )
        .expect("continuation");
        let history = logs
            .history(&root, "workspace", None, None)
            .expect("continued history");
        assert_eq!(history.sessions.len(), 1);
        assert_eq!(history.events.len(), 1);
        assert_eq!(history.events[0].message, "continued");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rotates_segments_and_marks_session_truncated_at_the_bound() {
        let root = fixture_root("rotation");
        let logs = LocalLogStore::default();

        logs.record(
            &root,
            &event(ProcessEventKind::Started, 100, "started", Some(42), None),
        )
        .expect("started");

        let large = "x".repeat(MAX_PERSISTED_MESSAGE_BYTES * 2);
        for index in 0..100u64 {
            logs.record(
                &root,
                &event(
                    ProcessEventKind::Log,
                    200 + index,
                    large.clone(),
                    Some(42),
                    None,
                ),
            )
            .expect("large log");
        }

        let history = logs
            .history(&root, "workspace", None, None)
            .expect("history");
        assert_eq!(history.sessions.len(), 1);
        assert!(history.sessions[0].truncated);
        assert!(history.events.len() < 101);
        assert!(history
            .events
            .iter()
            .filter(|event| matches!(event.kind, ProcessEventKind::Log))
            .all(|event| event.message.len() <= MAX_PERSISTED_MESSAGE_BYTES));

        let workspace_root = fs::read_dir(&root)
            .expect("root entries")
            .next()
            .expect("workspace dir")
            .expect("workspace entry")
            .path();
        let session_root = fs::read_dir(workspace_root)
            .expect("workspace entries")
            .next()
            .expect("session dir")
            .expect("session entry")
            .path();
        let segment_count = fs::read_dir(session_root)
            .expect("session files")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("events-"))
            .count();
        assert_eq!(segment_count, MAX_SEGMENTS_PER_SESSION);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn loads_one_selected_run_session_without_hiding_session_index() {
        let root = fixture_root("session-selection");
        let logs = LocalLogStore::default();

        logs.record(
            &root,
            &event(ProcessEventKind::Started, 100, "first start", Some(1), None),
        )
        .expect("first start");
        logs.record(
            &root,
            &event(ProcessEventKind::Exited, 110, "first end", Some(1), Some(0)),
        )
        .expect("first end");
        logs.record(
            &root,
            &event(
                ProcessEventKind::Started,
                200,
                "second start",
                Some(2),
                None,
            ),
        )
        .expect("second start");
        logs.record(
            &root,
            &event(
                ProcessEventKind::Exited,
                210,
                "second end",
                Some(2),
                Some(0),
            ),
        )
        .expect("second end");

        let all = logs
            .history(&root, "workspace", Some(50), None)
            .expect("all");
        assert_eq!(all.sessions.len(), 2);
        let first_id = all
            .sessions
            .iter()
            .find(|session| session.started_at_ms == 100)
            .expect("first session")
            .id
            .clone();

        let selected = logs
            .history(&root, "workspace", Some(50), Some(&first_id))
            .expect("selected");
        assert_eq!(selected.sessions.len(), 2);
        assert_eq!(selected.events.len(), 2);
        assert!(selected
            .events
            .iter()
            .all(|event| event.timestamp_ms <= 110));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retention_days_prune_terminal_sessions_by_age() {
        let root = fixture_root("retention");
        let logs = LocalLogStore::default();
        logs.set_retention_days(7).expect("retention");

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis() as u64;
        let old = now - 9 * 24 * 60 * 60 * 1_000;
        let recent = now - 24 * 60 * 60 * 1_000;

        logs.record(
            &root,
            &event(ProcessEventKind::Started, old, "old start", Some(1), None),
        )
        .expect("old start");
        logs.record(
            &root,
            &event(
                ProcessEventKind::Exited,
                old + 10,
                "old end",
                Some(1),
                Some(0),
            ),
        )
        .expect("old end");
        logs.record(
            &root,
            &event(
                ProcessEventKind::Started,
                recent,
                "recent start",
                Some(2),
                None,
            ),
        )
        .expect("recent start");
        logs.record(
            &root,
            &event(
                ProcessEventKind::Exited,
                recent + 10,
                "recent end",
                Some(2),
                Some(0),
            ),
        )
        .expect("recent end");
        logs.record(
            &root,
            &event(
                ProcessEventKind::Started,
                old + 20,
                "active start",
                Some(3),
                None,
            ),
        )
        .expect("active start");

        assert_eq!(logs.prune_all(&root).expect("prune"), 1);
        let history = logs
            .history(&root, "workspace", None, None)
            .expect("history");
        assert_eq!(history.sessions.len(), 2);
        assert!(history
            .sessions
            .iter()
            .any(|session| session.pid == Some(2) && session.status == RunSessionStatus::Exited));
        assert!(history.sessions.iter().any(|session| {
            session.pid == Some(3)
                && matches!(
                    session.status,
                    RunSessionStatus::Starting | RunSessionStatus::Running
                )
        }));

        let _ = fs::remove_dir_all(root);
    }
}
