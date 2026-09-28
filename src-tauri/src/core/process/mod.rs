pub mod metrics;
#[cfg(windows)]
mod windows_job;

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::time::Instant;

#[cfg(windows)]
use windows_job::WindowsJob;

use crate::{
    core::runtime,
    models::{
        process::{
            BulkProcessResult, ProcessActionError, ProcessEvent, ProcessEventKind, ProcessStatus,
            ProcessStream,
        },
        workspace::{PersistedComponent, PersistedWorkspace},
    },
};

pub type ProcessEventSink = Arc<dyn Fn(ProcessEvent) + Send + Sync + 'static>;

#[derive(Clone, Default)]
pub struct ProcessManager {
    processes: Arc<Mutex<HashMap<String, OwnedProcess>>>,
}

#[derive(Clone)]
struct OwnedProcess {
    workspace_id: String,
    component_id: String,
    component_name: String,
    pid: u32,
    started_at_ms: u64,
    child: Arc<Mutex<Child>>,
    stopping: Arc<AtomicBool>,
    #[cfg(windows)]
    job: Arc<WindowsJob>,
}

impl ProcessManager {
    pub fn start(
        &self,
        workspace: &PersistedWorkspace,
        component: &PersistedComponent,
        sink: ProcessEventSink,
    ) -> Result<ProcessStatus, String> {
        if !workspace.trusted {
            return Err("Workspace is not trusted for command execution.".to_string());
        }

        let program = component.program.as_deref().ok_or_else(|| {
            "This component predates the native execution contract. Rescan and add the workspace again."
                .to_string()
        })?;
        if program.trim().is_empty() {
            return Err("Component execution program is empty.".to_string());
        }

        let key = process_key(&workspace.id, &component.id);
        self.remove_stale_process(&key)?;
        {
            let processes = self
                .processes
                .lock()
                .map_err(|_| "Process manager lock is unavailable.".to_string())?;
            if processes.contains_key(&key) {
                return Err("This component already has a DECC-owned process.".to_string());
            }
        }

        let working_directory = resolve_working_directory(workspace, component)?;
        let execution = runtime::resolve_component_execution(workspace, component)?;
        emit(
            &sink,
            event_for(
                workspace,
                component,
                ProcessEventKind::Starting,
                ProcessStream::System,
                format!("starting: {}", component.command),
                None,
                None,
            ),
        );

        let mut command = Command::new(&execution.program);
        command
            .args(&component.args)
            .current_dir(&working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = execution.path {
            command.env("PATH", path);
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let message = format!(
                    "Could not start {} in {}: {error}",
                    component.command,
                    working_directory.display()
                );
                emit(
                    &sink,
                    event_for(
                        workspace,
                        component,
                        ProcessEventKind::Error,
                        ProcessStream::System,
                        message.clone(),
                        None,
                        None,
                    ),
                );
                return Err(message);
            }
        };

        #[cfg(windows)]
        let windows_job = match WindowsJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let message = format!(
                    "Could not secure Windows process-tree ownership for {}: {error}",
                    component.command
                );
                emit(
                    &sink,
                    event_for(
                        workspace,
                        component,
                        ProcessEventKind::Error,
                        ProcessStream::System,
                        message.clone(),
                        None,
                        None,
                    ),
                );
                return Err(message);
            }
        };

        let pid = child.id();
        let started_at_ms = unix_time_ms()?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(child));

        let owned = OwnedProcess {
            workspace_id: workspace.id.clone(),
            component_id: component.id.clone(),
            component_name: component.name.clone(),
            pid,
            started_at_ms,
            child: child.clone(),
            stopping: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            job: windows_job,
        };
        self.processes
            .lock()
            .map_err(|_| "Process manager lock is unavailable.".to_string())?
            .insert(key.clone(), owned.clone());

        if let Some(stdout) = stdout {
            spawn_reader(stdout, ProcessStream::Stdout, &owned, sink.clone());
        }
        if let Some(stderr) = stderr {
            spawn_reader(stderr, ProcessStream::Stderr, &owned, sink.clone());
        }
        self.spawn_monitor(key, owned.clone(), sink.clone());

        emit(
            &sink,
            ProcessEvent {
                workspace_id: owned.workspace_id.clone(),
                component_id: owned.component_id.clone(),
                component_name: owned.component_name.clone(),
                kind: ProcessEventKind::Started,
                stream: Some(ProcessStream::System),
                message: format!("Process started with PID {pid}"),
                pid: Some(pid),
                exit_code: None,
                timestamp_ms: started_at_ms,
            },
        );

        Ok(status_from_owned(&owned))
    }

    pub fn run_all(
        &self,
        workspace: &PersistedWorkspace,
        sink: ProcessEventSink,
    ) -> Result<BulkProcessResult, String> {
        if !workspace.trusted {
            return Err("Workspace is not trusted for command execution.".to_string());
        }

        let mut result = BulkProcessResult::default();
        for component in workspace
            .components
            .iter()
            .filter(|component| component.include_in_run_all)
        {
            match self.start(workspace, component, sink.clone()) {
                Ok(process) => result.processes.push(process),
                Err(message) if message.contains("already has a DECC-owned process") => {}
                Err(message) => result.errors.push(ProcessActionError {
                    component_id: component.id.clone(),
                    message,
                }),
            }
        }
        Ok(result)
    }

    pub fn stop(
        &self,
        workspace_id: &str,
        component_id: &str,
        sink: ProcessEventSink,
    ) -> Result<ProcessStatus, String> {
        let key = process_key(workspace_id, component_id);
        self.remove_stale_process(&key)?;
        let owned = self
            .processes
            .lock()
            .map_err(|_| "Process manager lock is unavailable.".to_string())?
            .get(&key)
            .cloned()
            .ok_or_else(|| "DECC does not own an active process for this component.".to_string())?;

        emit(
            &sink,
            ProcessEvent {
                workspace_id: owned.workspace_id.clone(),
                component_id: owned.component_id.clone(),
                component_name: owned.component_name.clone(),
                kind: ProcessEventKind::Stopping,
                stream: Some(ProcessStream::System),
                message: format!("Stopping DECC-owned PID {}", owned.pid),
                pid: Some(owned.pid),
                exit_code: None,
                timestamp_ms: unix_time_ms()?,
            },
        );

        request_stop(&owned)?;
        Ok(status_from_owned(&owned))
    }

    pub fn stop_all(
        &self,
        workspace_id: &str,
        sink: ProcessEventSink,
    ) -> Result<BulkProcessResult, String> {
        let statuses = self
            .list()?
            .into_iter()
            .filter(|process| process.workspace_id == workspace_id)
            .collect::<Vec<_>>();

        let mut result = BulkProcessResult::default();
        for process in statuses {
            match self.stop(workspace_id, &process.component_id, sink.clone()) {
                Ok(process) => result.processes.push(process),
                Err(message) => result.errors.push(ProcessActionError {
                    component_id: process.component_id,
                    message,
                }),
            }
        }
        Ok(result)
    }

    pub fn stop_everything(&self, sink: ProcessEventSink) -> Result<BulkProcessResult, String> {
        let statuses = self.list()?;
        let mut result = BulkProcessResult::default();
        for process in statuses {
            match self.stop(&process.workspace_id, &process.component_id, sink.clone()) {
                Ok(process) => result.processes.push(process),
                Err(message) => result.errors.push(ProcessActionError {
                    component_id: process.component_id,
                    message,
                }),
            }
        }
        Ok(result)
    }

    pub fn shutdown(&self) {
        let sink: ProcessEventSink = Arc::new(|_| {});
        let _ = self.stop_everything(sink);
        thread::sleep(Duration::from_millis(250));

        let remaining = self
            .processes
            .lock()
            .map(|processes| processes.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for process in remaining {
            let _ = force_stop(&process);
        }
    }

    pub fn list(&self) -> Result<Vec<ProcessStatus>, String> {
        let keys = self
            .processes
            .lock()
            .map_err(|_| "Process manager lock is unavailable.".to_string())?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            self.remove_stale_process(&key)?;
        }

        let mut statuses = self
            .processes
            .lock()
            .map_err(|_| "Process manager lock is unavailable.".to_string())?
            .values()
            .map(status_from_owned)
            .collect::<Vec<_>>();
        statuses.sort_by(|a, b| {
            a.workspace_id
                .cmp(&b.workspace_id)
                .then_with(|| a.component_id.cmp(&b.component_id))
        });
        Ok(statuses)
    }

    fn remove_stale_process(&self, key: &str) -> Result<(), String> {
        let owned = self
            .processes
            .lock()
            .map_err(|_| "Process manager lock is unavailable.".to_string())?
            .get(key)
            .cloned();
        let Some(owned) = owned else {
            return Ok(());
        };

        let exited = owned
            .child
            .lock()
            .map_err(|_| "Owned process lock is unavailable.".to_string())?
            .try_wait()
            .map_err(|error| format!("Could not inspect owned process: {error}"))?
            .is_some();
        if exited {
            if should_defer_stale_removal(&owned) {
                return Ok(());
            }
            finalize_owned_tree_after_root_exit(&owned)?;
            remove_if_same_pid(&self.processes, key, owned.pid)?;
        }
        Ok(())
    }

    fn spawn_monitor(&self, key: String, owned: OwnedProcess, sink: ProcessEventSink) {
        let processes = self.processes.clone();
        thread::spawn(move || loop {
            let result = match owned.child.lock() {
                Ok(mut child) => child.try_wait(),
                Err(_) => {
                    emit(
                        &sink,
                        process_event(
                            &owned,
                            ProcessEventKind::Error,
                            ProcessStream::System,
                            "Owned process lock became unavailable.".to_string(),
                            None,
                        ),
                    );
                    let _ = remove_if_same_pid(&processes, &key, owned.pid);
                    return;
                }
            };

            match result {
                Ok(Some(status)) => {
                    let exit_code = status.code();
                    if let Err(error) = finalize_owned_tree_after_root_exit(&owned) {
                        emit(
                            &sink,
                            process_event(
                                &owned,
                                ProcessEventKind::Error,
                                ProcessStream::System,
                                format!("Could not finish owned process-tree cleanup: {error}"),
                                None,
                            ),
                        );
                    }
                    let _ = remove_if_same_pid(&processes, &key, owned.pid);
                    emit(
                        &sink,
                        process_event(
                            &owned,
                            ProcessEventKind::Exited,
                            ProcessStream::System,
                            match exit_code {
                                Some(code) => format!("Process exited with code {code}"),
                                None => "Process exited without an exit code".to_string(),
                            },
                            exit_code,
                        ),
                    );
                    return;
                }
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(error) => {
                    let _ = remove_if_same_pid(&processes, &key, owned.pid);
                    emit(
                        &sink,
                        process_event(
                            &owned,
                            ProcessEventKind::Error,
                            ProcessStream::System,
                            format!("Could not inspect process state: {error}"),
                            None,
                        ),
                    );
                    return;
                }
            }
        });
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    reader: R,
    stream: ProcessStream,
    owned: &OwnedProcess,
    sink: ProcessEventSink,
) {
    let owned = owned.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) => return,
                Ok(_) => {
                    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                        bytes.pop();
                    }
                    emit(
                        &sink,
                        process_event(
                            &owned,
                            ProcessEventKind::Log,
                            stream.clone(),
                            String::from_utf8_lossy(&bytes).into_owned(),
                            None,
                        ),
                    );
                }
                Err(error) => {
                    emit(
                        &sink,
                        process_event(
                            &owned,
                            ProcessEventKind::Log,
                            ProcessStream::System,
                            format!("Could not read process output: {error}"),
                            None,
                        ),
                    );
                    return;
                }
            }
        }
    });
}

#[cfg(unix)]
fn request_stop(owned: &OwnedProcess) -> Result<(), String> {
    owned.stopping.store(true, Ordering::Release);

    let group = -(owned.pid as i32);
    let result = unsafe { libc::kill(group, libc::SIGTERM) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(format!("Could not stop DECC-owned process group: {error}"));
        }
    }

    let pid = owned.pid;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1500));
        if unix_process_group_exists(pid) {
            let _ = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        }
    });
    Ok(())
}

#[cfg(unix)]
fn unix_process_group_exists(pid: u32) -> bool {
    if unsafe { libc::kill(-(pid as i32), 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(unix)]
fn force_stop(owned: &OwnedProcess) -> Result<(), String> {
    let result = unsafe { libc::kill(-(owned.pid as i32), libc::SIGKILL) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(format!(
                "Could not force-stop DECC-owned process group: {error}"
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn should_defer_stale_removal(owned: &OwnedProcess) -> bool {
    owned.stopping.load(Ordering::Acquire) && unix_process_group_exists(owned.pid)
}

#[cfg(not(unix))]
fn should_defer_stale_removal(_owned: &OwnedProcess) -> bool {
    false
}

#[cfg(unix)]
fn finalize_owned_tree_after_root_exit(owned: &OwnedProcess) -> Result<(), String> {
    if !owned.stopping.load(Ordering::Acquire) {
        return force_stop(owned);
    }

    let deadline = Instant::now() + Duration::from_millis(1700);
    while unix_process_group_exists(owned.pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    if unix_process_group_exists(owned.pid) {
        force_stop(owned)?;
    }
    Ok(())
}

#[cfg(windows)]
fn request_stop(owned: &OwnedProcess) -> Result<(), String> {
    owned.stopping.store(true, Ordering::Release);
    owned.job.terminate()
}

#[cfg(windows)]
fn force_stop(owned: &OwnedProcess) -> Result<(), String> {
    owned.job.terminate()
}

#[cfg(windows)]
fn finalize_owned_tree_after_root_exit(owned: &OwnedProcess) -> Result<(), String> {
    if owned.stopping.load(Ordering::Acquire) {
        return Ok(());
    }
    force_stop(owned)
}

#[cfg(not(any(unix, windows)))]
fn finalize_owned_tree_after_root_exit(_owned: &OwnedProcess) -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn request_stop(owned: &OwnedProcess) -> Result<(), String> {
    owned.stopping.store(true, Ordering::Release);
    let mut child = owned
        .child
        .lock()
        .map_err(|_| "Owned process lock is unavailable.".to_string())?;
    if child
        .try_wait()
        .map_err(|error| format!("Could not inspect owned process: {error}"))?
        .is_none()
    {
        child
            .kill()
            .map_err(|error| format!("Could not stop DECC-owned process: {error}"))?;
    }
    Ok(())
}

fn resolve_working_directory(
    workspace: &PersistedWorkspace,
    component: &PersistedComponent,
) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(&workspace.path)
        .map_err(|error| format!("Workspace folder is unavailable: {error}"))?;
    let candidate = if component.relative_path == "." {
        root.clone()
    } else {
        root.join(&component.relative_path)
    };
    let working = std::fs::canonicalize(candidate)
        .map_err(|error| format!("Component working directory is unavailable: {error}"))?;
    if !working.starts_with(&root) {
        return Err("Component working directory resolves outside the workspace.".to_string());
    }
    Ok(working)
}

fn process_key(workspace_id: &str, component_id: &str) -> String {
    format!("{workspace_id}\u{1f}{component_id}")
}

fn status_from_owned(process: &OwnedProcess) -> ProcessStatus {
    ProcessStatus {
        workspace_id: process.workspace_id.clone(),
        component_id: process.component_id.clone(),
        component_name: process.component_name.clone(),
        pid: process.pid,
        started_at_ms: process.started_at_ms,
    }
}

fn remove_if_same_pid(
    processes: &Arc<Mutex<HashMap<String, OwnedProcess>>>,
    key: &str,
    pid: u32,
) -> Result<(), String> {
    let mut processes = processes
        .lock()
        .map_err(|_| "Process manager lock is unavailable.".to_string())?;
    if processes.get(key).is_some_and(|process| process.pid == pid) {
        processes.remove(key);
    }
    Ok(())
}

fn event_for(
    workspace: &PersistedWorkspace,
    component: &PersistedComponent,
    kind: ProcessEventKind,
    stream: ProcessStream,
    message: String,
    pid: Option<u32>,
    exit_code: Option<i32>,
) -> ProcessEvent {
    ProcessEvent {
        workspace_id: workspace.id.clone(),
        component_id: component.id.clone(),
        component_name: component.name.clone(),
        kind,
        stream: Some(stream),
        message,
        pid,
        exit_code,
        timestamp_ms: unix_time_ms().unwrap_or_default(),
    }
}

fn process_event(
    owned: &OwnedProcess,
    kind: ProcessEventKind,
    stream: ProcessStream,
    message: String,
    exit_code: Option<i32>,
) -> ProcessEvent {
    ProcessEvent {
        workspace_id: owned.workspace_id.clone(),
        component_id: owned.component_id.clone(),
        component_name: owned.component_name.clone(),
        kind,
        stream: Some(stream),
        message,
        pid: Some(owned.pid),
        exit_code,
        timestamp_ms: unix_time_ms().unwrap_or_default(),
    }
}

fn emit(sink: &ProcessEventSink, event: ProcessEvent) {
    sink(event);
}

fn unix_time_ms() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("System clock is before Unix epoch: {error}"))?;
    u64::try_from(duration.as_millis()).map_err(|_| "System time is out of range.".to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Mutex,
        },
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use crate::models::{
        detection::DetectionConfidence,
        process::{ProcessEvent, ProcessEventKind},
        workspace::{PersistedComponent, PersistedWorkspace},
    };

    use super::{ProcessEventSink, ProcessManager};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct CleanupDir(PathBuf);

    impl CleanupDir {
        fn workspace(workspace: &PersistedWorkspace) -> Self {
            Self(PathBuf::from(&workspace.path))
        }
    }

    impl Drop for CleanupDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture_workspace(trusted: bool, program: String, args: Vec<String>) -> PersistedWorkspace {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "decc-process-{}-{unique}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("fixture root");
        PersistedWorkspace {
            id: format!("workspace-{unique}"),
            name: "Process fixture".to_string(),
            path: root.to_string_lossy().into_owned(),
            trusted,
            components: vec![PersistedComponent {
                id: "component".to_string(),
                name: "Fixture".to_string(),
                relative_path: ".".to_string(),
                technology: "Fixture".to_string(),
                runtime: "Fixture".to_string(),
                command: "fixture command".to_string(),
                program: Some(program),
                args,
                expected_ports: vec![],
                include_in_run_all: true,
                detection_confidence: DetectionConfidence::High,
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[cfg(unix)]
    fn output_command() -> (String, Vec<String>) {
        (
            "/bin/sh".to_string(),
            vec![
                "-c".to_string(),
                "printf 'hello\\n'; printf 'oops\\n' >&2; sleep 0.15".to_string(),
            ],
        )
    }

    #[cfg(windows)]
    fn output_command() -> (String, Vec<String>) {
        (
            "cmd".to_string(),
            vec![
                "/C".to_string(),
                "echo hello&&echo oops>&2&&ping -n 2 127.0.0.1 >nul".to_string(),
            ],
        )
    }

    #[test]
    fn refuses_untrusted_workspace_before_spawn() {
        let (program, args) = output_command();
        let workspace = fixture_workspace(false, program, args);
        let manager = ProcessManager::default();
        let sink: ProcessEventSink = Arc::new(|_| {});
        let error = manager
            .start(&workspace, &workspace.components[0], sink)
            .expect_err("untrusted workspace should fail");
        assert!(error.contains("not trusted"));
        assert!(manager.list().expect("list").is_empty());
        let _ = fs::remove_dir_all(&workspace.path);
    }

    #[test]
    fn captures_output_and_releases_owned_process_after_exit() {
        let (program, args) = output_command();
        let workspace = fixture_workspace(true, program, args);
        let _cleanup = CleanupDir::workspace(&workspace);
        let manager = ProcessManager::default();
        let events = Arc::new(Mutex::new(Vec::<ProcessEvent>::new()));
        let sink_events = events.clone();
        let sink: ProcessEventSink = Arc::new(move |event| {
            sink_events.lock().expect("events").push(event);
        });

        let status = manager
            .start(&workspace, &workspace.components[0], sink)
            .expect("start");
        assert!(status.pid > 0);

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let exited = events
                .lock()
                .expect("events")
                .iter()
                .any(|event| event.kind == ProcessEventKind::Exited);
            if manager.list().expect("list").is_empty() && exited {
                break;
            }
            assert!(Instant::now() < deadline, "process did not exit in time");
            thread::sleep(Duration::from_millis(25));
        }

        let events = events.lock().expect("events");
        assert!(events
            .iter()
            .any(|event| event.kind == ProcessEventKind::Started));
        assert!(events.iter().any(|event| event.message == "hello"));
        assert!(events.iter().any(|event| event.message == "oops"));
        assert!(events
            .iter()
            .any(|event| event.kind == ProcessEventKind::Exited));
        let _ = fs::remove_dir_all(&workspace.path);
    }

    #[cfg(unix)]
    fn long_running_command() -> (String, Vec<String>) {
        (
            "/bin/sh".to_string(),
            vec!["-c".to_string(), "sleep 5".to_string()],
        )
    }

    #[cfg(windows)]
    fn long_running_command() -> (String, Vec<String>) {
        (
            "cmd".to_string(),
            vec!["/C".to_string(), "ping -n 6 127.0.0.1 >nul".to_string()],
        )
    }

    #[test]
    fn stopped_component_can_start_again_as_a_new_owned_process() {
        let (program, args) = long_running_command();
        let workspace = fixture_workspace(true, program, args);
        let manager = ProcessManager::default();
        let sink: ProcessEventSink = Arc::new(|_| {});

        let first = manager
            .start(&workspace, &workspace.components[0], sink.clone())
            .expect("first start");
        manager
            .stop(&workspace.id, &workspace.components[0].id, sink.clone())
            .expect("stop");

        let deadline = Instant::now() + Duration::from_secs(5);
        while !manager.list().expect("list").is_empty() {
            assert!(
                Instant::now() < deadline,
                "owned process did not stop before restart"
            );
            thread::sleep(Duration::from_millis(25));
        }

        let second = manager
            .start(&workspace, &workspace.components[0], sink.clone())
            .expect("second start");
        assert_ne!(first.pid, second.pid);
        manager
            .stop(&workspace.id, &workspace.components[0].id, sink)
            .expect("cleanup stop");

        let deadline = Instant::now() + Duration::from_secs(5);
        while !manager.list().expect("list").is_empty() {
            assert!(Instant::now() < deadline, "cleanup process did not stop");
            thread::sleep(Duration::from_millis(25));
        }
        let _ = fs::remove_dir_all(&workspace.path);
    }

    #[cfg(unix)]
    #[test]
    fn stop_terminates_owned_descendants_without_touching_external_processes() {
        let command = r#"sh -c 'trap "printf stopped > child-stopped.marker; exit 0" TERM; printf ready > child-ready.marker; while :; do sleep 1; done' & wait"#;
        let workspace = fixture_workspace(
            true,
            "/bin/sh".to_string(),
            vec!["-c".to_string(), command.to_string()],
        );
        let manager = ProcessManager::default();
        let sink: ProcessEventSink = Arc::new(|_| {});
        let root = std::path::PathBuf::from(&workspace.path);
        let ready = root.join("child-ready.marker");
        let stopped = root.join("child-stopped.marker");

        manager
            .start(&workspace, &workspace.components[0], sink.clone())
            .expect("start owned tree");

        let ready_deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() {
            assert!(
                Instant::now() < ready_deadline,
                "owned descendant did not become ready"
            );
            thread::sleep(Duration::from_millis(20));
        }

        let mut external = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 10"])
            .spawn()
            .expect("spawn external process");

        manager
            .stop(&workspace.id, &workspace.components[0].id, sink)
            .expect("stop owned tree");

        let stop_deadline = Instant::now() + Duration::from_secs(5);
        while !stopped.exists() || !manager.list().expect("list").is_empty() {
            assert!(
                Instant::now() < stop_deadline,
                "owned process group did not terminate completely"
            );
            thread::sleep(Duration::from_millis(20));
        }

        assert!(
            external.try_wait().expect("inspect external").is_none(),
            "stopping an owned process group must not terminate an unrelated process"
        );

        external.kill().expect("cleanup external");
        let _ = external.wait();
        let _ = fs::remove_dir_all(&workspace.path);
    }

    #[cfg(windows)]
    #[test]
    fn windows_job_stop_terminates_descendants_without_touching_external_processes() {
        let command = "ping -n 2 127.0.0.1 >nul&&cmd /D /C descendant.cmd";
        let workspace = fixture_workspace(
            true,
            "cmd".to_string(),
            vec!["/D".to_string(), "/C".to_string(), command.to_string()],
        );
        let _cleanup = CleanupDir::workspace(&workspace);
        let manager = ProcessManager::default();
        let sink: ProcessEventSink = Arc::new(|_| {});
        let root = std::path::PathBuf::from(&workspace.path);
        fs::write(
            root.join("descendant.cmd"),
            "@echo off\r\necho ready>child-ready.marker\r\nping -n 5 127.0.0.1 >nul\r\necho leaked>leaked.marker\r\n",
        )
        .expect("descendant fixture");
        let ready = root.join("child-ready.marker");
        let leaked = root.join("leaked.marker");

        manager
            .start(&workspace, &workspace.components[0], sink.clone())
            .expect("start owned Windows job");

        let ready_deadline = Instant::now() + Duration::from_secs(8);
        while !ready.exists() {
            assert!(
                Instant::now() < ready_deadline,
                "owned Windows descendant did not become ready"
            );
            thread::sleep(Duration::from_millis(50));
        }

        let mut external = std::process::Command::new("cmd")
            .args(["/C", "ping -n 10 127.0.0.1 >nul"])
            .spawn()
            .expect("spawn external Windows process");

        manager
            .stop(&workspace.id, &workspace.components[0].id, sink)
            .expect("stop owned Windows job");

        let stop_deadline = Instant::now() + Duration::from_secs(8);
        while !manager.list().expect("list").is_empty() {
            assert!(
                Instant::now() < stop_deadline,
                "owned Windows job did not terminate completely"
            );
            thread::sleep(Duration::from_millis(50));
        }

        thread::sleep(Duration::from_secs(4));
        assert!(
            !leaked.exists(),
            "a descendant survived Windows Job Object termination"
        );
        assert!(
            external.try_wait().expect("inspect external").is_none(),
            "terminating an owned Windows Job Object must not kill unrelated processes"
        );

        external.kill().expect("cleanup external");
        let _ = external.wait();
        let _ = fs::remove_dir_all(&workspace.path);
    }

    #[cfg(unix)]
    #[test]
    fn root_exit_cleans_up_owned_descendants_before_releasing_ownership() {
        let command = r#"sh -c 'sleep 1; printf leaked > leaked.marker' & printf ready > child-ready.marker; exit 0"#;
        let workspace = fixture_workspace(
            true,
            "/bin/sh".to_string(),
            vec!["-c".to_string(), command.to_string()],
        );
        let manager = ProcessManager::default();
        let sink: ProcessEventSink = Arc::new(|_| {});
        let root = std::path::PathBuf::from(&workspace.path);
        let ready = root.join("child-ready.marker");
        let leaked = root.join("leaked.marker");

        manager
            .start(&workspace, &workspace.components[0], sink)
            .expect("start owned tree");

        let ready_deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() {
            assert!(
                Instant::now() < ready_deadline,
                "owned descendant did not become ready"
            );
            thread::sleep(Duration::from_millis(20));
        }

        let release_deadline = Instant::now() + Duration::from_secs(5);
        while !manager.list().expect("list").is_empty() {
            assert!(
                Instant::now() < release_deadline,
                "ownership was not released after root exit"
            );
            thread::sleep(Duration::from_millis(20));
        }

        thread::sleep(Duration::from_millis(1200));
        assert!(
            !leaked.exists(),
            "a descendant survived after its owned root process exited"
        );

        let _ = fs::remove_dir_all(&workspace.path);
    }

    #[test]
    fn run_all_respects_inclusion_and_stop_all_targets_owned_processes() {
        let (program, args) = long_running_command();
        let mut workspace = fixture_workspace(true, program, args);
        let mut excluded = workspace.components[0].clone();
        excluded.id = "excluded".to_string();
        excluded.name = "Excluded".to_string();
        excluded.include_in_run_all = false;
        workspace.components.push(excluded);

        let manager = ProcessManager::default();
        let events = Arc::new(Mutex::new(Vec::<ProcessEvent>::new()));
        let sink_events = events.clone();
        let sink: ProcessEventSink = Arc::new(move |event| {
            sink_events.lock().expect("events").push(event);
        });

        let started = manager.run_all(&workspace, sink.clone()).expect("run all");
        assert_eq!(started.processes.len(), 1);
        assert!(started.errors.is_empty());
        assert_eq!(started.processes[0].component_id, "component");
        assert_eq!(manager.list().expect("list").len(), 1);

        let duplicate = manager
            .start(&workspace, &workspace.components[0], sink.clone())
            .expect_err("second process for same component must fail");
        assert!(duplicate.contains("already has"));

        let stopped = manager.stop_all(&workspace.id, sink).expect("stop all");
        assert_eq!(stopped.processes.len(), 1);
        assert!(stopped.errors.is_empty());

        let deadline = Instant::now() + Duration::from_secs(5);
        while !manager.list().expect("list").is_empty() {
            assert!(
                Instant::now() < deadline,
                "owned process did not stop in time"
            );
            thread::sleep(Duration::from_millis(25));
        }
        let events = events.lock().expect("events");
        assert!(events
            .iter()
            .any(|event| event.kind == ProcessEventKind::Stopping));
        assert!(!events.iter().any(|event| event.component_id == "excluded"));
        let _ = fs::remove_dir_all(&workspace.path);
    }
}
