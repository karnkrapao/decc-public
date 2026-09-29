use std::{
    fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    process::{Child, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::core::background_command::background_command;
use crate::models::{
    git::{
        GitChange, GitChangeStatus, GitCommit, GitInspectionState, WorkspaceGitHeadInspection,
        WorkspaceGitInspection,
    },
    workspace::PersistedWorkspace,
};

const MAX_CHANGES: usize = 500;
const MAX_COMMITS: usize = 12;
const MAX_ERROR_BYTES: usize = 4096;
const MAX_COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

pub fn inspect(workspaces: &[PersistedWorkspace]) -> Vec<WorkspaceGitInspection> {
    workspaces.iter().map(inspect_workspace).collect()
}
pub fn inspect_heads(workspaces: &[PersistedWorkspace]) -> Vec<WorkspaceGitHeadInspection> {
    workspaces.iter().map(inspect_workspace_head).collect()
}

fn read_workspace_head_without_git(root: &Path) -> Option<(String, bool)> {
    let mut current = Some(root);

    while let Some(directory) = current {
        let dot_git = directory.join(".git");
        let git_dir = if dot_git.is_dir() {
            Some(dot_git)
        } else if dot_git.is_file() {
            let pointer = fs::read_to_string(&dot_git).ok()?;
            let relative = pointer.trim().strip_prefix("gitdir:")?.trim();
            let path = PathBuf::from(relative);
            Some(if path.is_absolute() {
                path
            } else {
                directory.join(path)
            })
        } else {
            None
        };

        if let Some(git_dir) = git_dir {
            let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
            let head = head.trim();
            if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
                if !branch.is_empty() {
                    return Some((branch.to_string(), false));
                }
            }

            if head.len() >= 7 && head.chars().all(|character| character.is_ascii_hexdigit()) {
                return Some((format!("Detached @ {}", &head[..head.len().min(8)]), true));
            }
            return None;
        }

        current = directory.parent();
    }

    None
}

fn inspect_workspace_head(workspace: &PersistedWorkspace) -> WorkspaceGitHeadInspection {
    let root = PathBuf::from(&workspace.path);
    if !root.is_dir() {
        return WorkspaceGitHeadInspection {
            workspace_id: workspace.id.clone(),
            state: GitInspectionState::Error,
            branch: None,
            detached: false,
            error: Some("Workspace folder is unavailable.".to_string()),
        };
    }

    if let Some((branch, detached)) = read_workspace_head_without_git(&root) {
        return WorkspaceGitHeadInspection {
            workspace_id: workspace.id.clone(),
            state: GitInspectionState::Ready,
            branch: Some(branch),
            detached,
            error: None,
        };
    }

    let inside = match run_git(&root, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(output) => output,
        Err(error) => {
            let inspection = inspection_from_run_error(workspace.id.clone(), error);
            return WorkspaceGitHeadInspection {
                workspace_id: workspace.id.clone(),
                state: inspection.state,
                branch: None,
                detached: false,
                error: inspection.error,
            };
        }
    };

    if !inside.status.success() || stdout_text(&inside).trim() != "true" {
        let error = output_error(&inside);
        return WorkspaceGitHeadInspection {
            workspace_id: workspace.id.clone(),
            state: if error.to_ascii_lowercase().contains("not a git repository") {
                GitInspectionState::NotRepository
            } else {
                GitInspectionState::Error
            },
            branch: None,
            detached: false,
            error: (!error.is_empty()).then_some(error),
        };
    }

    match run_git(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Ok(output) if output.status.success() => {
            let branch = stdout_text(&output).trim().to_string();
            if !branch.is_empty() {
                return WorkspaceGitHeadInspection {
                    workspace_id: workspace.id.clone(),
                    state: GitInspectionState::Ready,
                    branch: Some(branch),
                    detached: false,
                    error: None,
                };
            }
        }
        Ok(_) => {}
        Err(error) => {
            let inspection = inspection_from_run_error(workspace.id.clone(), error);
            return WorkspaceGitHeadInspection {
                workspace_id: workspace.id.clone(),
                state: inspection.state,
                branch: None,
                detached: false,
                error: inspection.error,
            };
        }
    }

    let short = run_git(&root, &["rev-parse", "--short", "HEAD"])
        .ok()
        .filter(|output| output.status.success())
        .map(|output| stdout_text(&output).trim().to_string())
        .filter(|value| !value.is_empty());
    let detached = short.is_some();
    let branch = short
        .map(|hash| format!("Detached @ {hash}"))
        .unwrap_or_else(|| "Unborn branch".to_string());

    WorkspaceGitHeadInspection {
        workspace_id: workspace.id.clone(),
        state: GitInspectionState::Ready,
        branch: Some(branch),
        detached,
        error: None,
    }
}

fn inspect_workspace(workspace: &PersistedWorkspace) -> WorkspaceGitInspection {
    let root = PathBuf::from(&workspace.path);
    if !root.is_dir() {
        return WorkspaceGitInspection::error(
            workspace.id.clone(),
            "Workspace folder is unavailable.",
        );
    }

    let inside = match run_git(&root, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(output) => output,
        Err(error) => return inspection_from_run_error(workspace.id.clone(), error),
    };

    if !inside.status.success() {
        let error = output_error(&inside);
        if error.to_ascii_lowercase().contains("not a git repository") {
            return WorkspaceGitInspection::not_repository(workspace.id.clone());
        }
        return WorkspaceGitInspection::error(
            workspace.id.clone(),
            if error.is_empty() {
                "Git could not inspect this workspace.".to_string()
            } else {
                error
            },
        );
    }
    if stdout_text(&inside).trim() != "true" {
        return WorkspaceGitInspection::not_repository(workspace.id.clone());
    }

    let repository_root = match run_git(&root, &["rev-parse", "--show-toplevel"]) {
        Ok(output) if output.status.success() => Some(stdout_text(&output).trim().to_string()),
        Ok(output) => {
            return WorkspaceGitInspection::error(workspace.id.clone(), output_error(&output))
        }
        Err(error) => return inspection_from_run_error(workspace.id.clone(), error),
    };

    let has_head = match run_git(&root, &["rev-parse", "--verify", "HEAD"]) {
        Ok(output) => output.status.success(),
        Err(error) => return inspection_from_run_error(workspace.id.clone(), error),
    };

    let symbolic_branch = match run_git(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Ok(output) if output.status.success() => {
            let branch = stdout_text(&output).trim().to_string();
            (!branch.is_empty()).then_some(branch)
        }
        Ok(_) => None,
        Err(error) => return inspection_from_run_error(workspace.id.clone(), error),
    };

    let (branch, detached) = if let Some(branch) = symbolic_branch {
        (Some(branch), false)
    } else if has_head {
        let short = match run_git(&root, &["rev-parse", "--short", "HEAD"]) {
            Ok(output) if output.status.success() => {
                let hash = stdout_text(&output).trim().to_string();
                (!hash.is_empty()).then_some(hash)
            }
            Ok(output) => {
                return WorkspaceGitInspection::error(workspace.id.clone(), output_error(&output))
            }
            Err(error) => return inspection_from_run_error(workspace.id.clone(), error),
        };
        (short.map(|hash| format!("Detached @ {hash}")), true)
    } else {
        (Some("Unborn branch".to_string()), false)
    };

    let upstream = if has_head {
        match run_git(
            &root,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        ) {
            Ok(output) if output.status.success() => {
                let value = stdout_text(&output).trim().to_string();
                (!value.is_empty()).then_some(value)
            }
            Ok(_) => None,
            Err(error) => return inspection_from_run_error(workspace.id.clone(), error),
        }
    } else {
        None
    };

    let (ahead, behind) = if upstream.is_some() {
        match run_git(
            &root,
            &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
        ) {
            Ok(output) if output.status.success() => {
                parse_ahead_behind(&stdout_text(&output)).unwrap_or((0, 0))
            }
            Ok(_) => (0, 0),
            Err(error) => return inspection_from_run_error(workspace.id.clone(), error),
        }
    } else {
        (0, 0)
    };

    let status = match run_git(
        &root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
            "--ignore-submodules=all",
            "--",
            ".",
        ],
    ) {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return WorkspaceGitInspection::error(workspace.id.clone(), output_error(&output))
        }
        Err(error) => return inspection_from_run_error(workspace.id.clone(), error),
    };
    let (changes, parsed_changes_truncated) = parse_status(&status.stdout, MAX_CHANGES);
    let changes_truncated = status.stdout_truncated || parsed_changes_truncated;

    let commits = if has_head {
        match run_git(
            &root,
            &[
                "log",
                "-n",
                &MAX_COMMITS.to_string(),
                "--pretty=format:%h%x1f%s%x1f%an%x1f%ar%x1e",
                "--",
                ".",
            ],
        ) {
            Ok(output) if output.status.success() => parse_commits(&output.stdout, MAX_COMMITS),
            Ok(_) | Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    WorkspaceGitInspection {
        workspace_id: workspace.id.clone(),
        state: GitInspectionState::Ready,
        repository_root,
        branch,
        detached,
        upstream,
        ahead,
        behind,
        changes,
        changes_truncated,
        commits,
        error: None,
    }
}

pub fn list_local_branches(workspace: &PersistedWorkspace) -> Result<Vec<String>, String> {
    let root = PathBuf::from(&workspace.path);
    if !root.is_dir() {
        return Err("Workspace folder is unavailable.".to_string());
    }

    let inside =
        run_git(&root, &["rev-parse", "--is-inside-work-tree"]).map_err(git_action_error)?;
    if !inside.status.success() || stdout_text(&inside).trim() != "true" {
        return Err("This workspace is not inside a Git working tree.".to_string());
    }

    let output = run_git(
        &root,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    )
    .map_err(git_action_error)?;
    if !output.status.success() {
        return Err(action_output_error(
            "Could not list local Git branches.",
            &output,
        ));
    }

    let mut branches = stdout_text(&output)
        .lines()
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .take(500)
        .map(str::to_string)
        .collect::<Vec<_>>();
    branches.sort_by_key(|branch| branch.to_ascii_lowercase());
    branches.dedup();
    Ok(branches)
}

pub fn switch_branch(
    workspace: &PersistedWorkspace,
    target_branch: &str,
) -> Result<WorkspaceGitInspection, String> {
    let target_branch = target_branch.trim();
    if target_branch.is_empty()
        || target_branch.len() > 256
        || target_branch.starts_with('-')
        || target_branch.chars().any(char::is_control)
    {
        return Err("Choose a valid local Git branch.".to_string());
    }

    let root = PathBuf::from(&workspace.path);
    if !root.is_dir() {
        return Err("Workspace folder is unavailable.".to_string());
    }

    let check = run_git(&root, &["check-ref-format", "--branch", target_branch])
        .map_err(git_action_error)?;
    if !check.status.success() {
        return Err("Choose a valid local Git branch.".to_string());
    }

    let reference = format!("refs/heads/{target_branch}");
    let exists = run_git(&root, &["show-ref", "--verify", "--quiet", &reference])
        .map_err(git_action_error)?;
    if !exists.status.success() {
        return Err(format!("Local branch {target_branch} was not found."));
    }

    let current = run_git(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .map_err(git_action_error)?;
    if current.status.success() && stdout_text(&current).trim() == target_branch {
        return Ok(inspect_workspace(workspace));
    }

    let dirty = run_git(
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
    )
    .map_err(git_action_error)?;
    if !dirty.status.success() {
        return Err(action_output_error(
            "Could not check the Git working tree before switching branches.",
            &dirty,
        ));
    }
    if !dirty.stdout.is_empty() {
        return Err(
            "The repository has uncommitted changes. Commit or stash them before switching branches in DECC."
                .to_string(),
        );
    }

    let switched = run_git_mutating(&root, &["switch", target_branch]).map_err(git_action_error)?;
    if !switched.status.success() {
        return Err(action_output_error(
            &format!("Could not switch to {target_branch}."),
            &switched,
        ));
    }

    Ok(inspect_workspace(workspace))
}

fn git_action_error(error: GitRunError) -> String {
    match error {
        GitRunError::Unavailable => "Git is not available to DECC on its current PATH.".to_string(),
        GitRunError::Timeout => "Git command timed out after 5 seconds.".to_string(),
        GitRunError::Spawn(message) => message,
    }
}

fn action_output_error(prefix: &str, output: &GitCommandOutput) -> String {
    let detail = output_error(output);
    if detail.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix} {detail}")
    }
}

#[derive(Debug)]
enum GitRunError {
    Unavailable,
    Timeout,
    Spawn(String),
}

struct GitCommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
}

fn run_git(root: &Path, args: &[&str]) -> Result<GitCommandOutput, GitRunError> {
    let program = find_git_executable(root).ok_or(GitRunError::Unavailable)?;
    run_git_with_program_mode(&program, root, args, GIT_COMMAND_TIMEOUT, true)
}

fn run_git_mutating(root: &Path, args: &[&str]) -> Result<GitCommandOutput, GitRunError> {
    let program = find_git_executable(root).ok_or(GitRunError::Unavailable)?;
    run_git_with_program_mode(&program, root, args, GIT_COMMAND_TIMEOUT, false)
}

fn find_git_executable(workspace_root: &Path) -> Option<PathBuf> {
    let process_cwd = std::env::current_dir().ok()?;
    let mut directories = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();

    #[cfg(target_os = "macos")]
    directories.extend([
        PathBuf::from("/usr/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);

    #[cfg(all(unix, not(target_os = "macos")))]
    directories.extend([
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/bin"),
    ]);

    #[cfg(windows)]
    {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            directories.push(PathBuf::from(program_files).join("Git").join("cmd"));
        }
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            directories.push(
                PathBuf::from(local_app_data)
                    .join("Programs")
                    .join("Git")
                    .join("cmd"),
            );
        }
    }

    directories.dedup();
    let path = std::env::join_paths(directories).ok()?;
    find_git_executable_in_path(workspace_root, &path, &process_cwd)
}

fn find_git_executable_in_path(
    workspace_root: &Path,
    path: &std::ffi::OsStr,
    process_cwd: &Path,
) -> Option<PathBuf> {
    let workspace_root = fs_canonicalize(workspace_root)?;

    for directory in std::env::split_paths(path) {
        let directory = if directory.is_absolute() {
            directory
        } else {
            process_cwd.join(directory)
        };

        #[cfg(windows)]
        let candidates = vec![directory.join("git.exe")];

        #[cfg(not(windows))]
        let candidates = vec![directory.join("git")];

        for candidate in candidates {
            let Some(candidate) = fs_canonicalize(&candidate) else {
                continue;
            };
            if candidate.starts_with(&workspace_root) || !is_executable_file(&candidate) {
                continue;
            }
            return Some(candidate);
        }
    }

    None
}

fn fs_canonicalize(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

#[cfg(all(test, unix))]
fn run_git_with_program(
    program: &Path,
    root: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<GitCommandOutput, GitRunError> {
    run_git_with_program_mode(program, root, args, timeout, true)
}

fn run_git_with_program_mode(
    program: &Path,
    root: &Path,
    args: &[&str],
    timeout: Duration,
    disable_optional_locks: bool,
) -> Result<GitCommandOutput, GitRunError> {
    let mut command = background_command(program);
    command
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .env("LC_ALL", "C")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_CEILING_DIRECTORIES")
        .env_remove("GIT_DISCOVERY_ACROSS_FILESYSTEM")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if disable_optional_locks {
        command.env("GIT_OPTIONAL_LOCKS", "0");
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command.spawn().map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            GitRunError::Unavailable
        } else {
            GitRunError::Spawn(format!("Could not inspect Git context: {error}"))
        }
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GitRunError::Spawn("Git stdout was unavailable.".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| GitRunError::Spawn("Git stderr was unavailable.".to_string()))?;

    let stdout_reader = thread::spawn(move || read_capped(stdout, MAX_COMMAND_OUTPUT_BYTES));
    let stderr_reader = thread::spawn(move || read_capped(stderr, MAX_COMMAND_OUTPUT_BYTES));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                terminate_git_process(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(GitRunError::Timeout);
            }
            Err(error) => {
                terminate_git_process(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(GitRunError::Spawn(format!(
                    "Could not inspect Git process state: {error}"
                )));
            }
        }
    };

    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| GitRunError::Spawn("Git stdout reader failed.".to_string()))?;
    let (stderr, _) = stderr_reader
        .join()
        .map_err(|_| GitRunError::Spawn("Git stderr reader failed.".to_string()))?;

    Ok(GitCommandOutput {
        status,
        stdout,
        stderr,
        stdout_truncated,
    })
}

#[cfg(unix)]
fn terminate_git_process(child: &mut Child) {
    let pid = child.id();
    let _ = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_git_process(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_capped<R: Read>(mut reader: R, limit: usize) -> (Vec<u8>, bool) {
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0u8; 8192];
    let mut truncated = false;

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let remaining = limit.saturating_sub(output.len());
                if remaining > 0 {
                    output.extend_from_slice(&buffer[..read.min(remaining)]);
                }
                if read > remaining {
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }

    (output, truncated)
}

fn inspection_from_run_error(workspace_id: String, error: GitRunError) -> WorkspaceGitInspection {
    match error {
        GitRunError::Unavailable => WorkspaceGitInspection::unavailable(
            workspace_id,
            "Git is not available to DECC on its current PATH.",
        ),
        GitRunError::Timeout => {
            WorkspaceGitInspection::error(workspace_id, "Git inspection timed out after 5 seconds.")
        }
        GitRunError::Spawn(message) => WorkspaceGitInspection::error(workspace_id, message),
    }
}

fn stdout_text(output: &GitCommandOutput) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn output_error(output: &GitCommandOutput) -> String {
    let source = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    let end = source.len().min(MAX_ERROR_BYTES);
    String::from_utf8_lossy(&source[..end]).trim().to_string()
}

fn parse_ahead_behind(text: &str) -> Option<(u32, u32)> {
    let mut fields = text.split_whitespace();
    let ahead = fields.next()?.parse::<u32>().ok()?;
    let behind = fields.next()?.parse::<u32>().ok()?;
    Some((ahead, behind))
}

fn parse_status(bytes: &[u8], limit: usize) -> (Vec<GitChange>, bool) {
    let fields = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>();
    let mut changes = Vec::new();
    let mut index = 0;
    let mut total = 0usize;

    while index < fields.len() {
        let record = fields[index];
        index += 1;
        if record.len() < 3 {
            continue;
        }

        let x = record[0] as char;
        let y = record[1] as char;
        let path_start = if record.get(2) == Some(&b' ') { 3 } else { 2 };
        if path_start > record.len() {
            continue;
        }
        let path = String::from_utf8_lossy(&record[path_start..]).into_owned();
        if path.is_empty() {
            continue;
        }

        total += 1;
        if changes.len() < limit {
            changes.push(GitChange {
                path,
                status: change_status(x, y),
            });
        }

        if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') {
            index = index.saturating_add(1).min(fields.len());
        }
    }

    (changes, total > limit)
}

fn change_status(x: char, y: char) -> GitChangeStatus {
    if x == '?' && y == '?' {
        return GitChangeStatus::Untracked;
    }
    if x == 'D' || y == 'D' {
        return GitChangeStatus::Deleted;
    }
    if x == 'A' || y == 'A' {
        return GitChangeStatus::Added;
    }
    GitChangeStatus::Modified
}

fn parse_commits(bytes: &[u8], limit: usize) -> Vec<GitCommit> {
    let text = String::from_utf8_lossy(bytes);
    text.split('\u{1e}')
        .filter_map(|record| {
            let record = record.trim_matches(['\n', '\r']);
            if record.is_empty() {
                return None;
            }
            let mut fields = record.split('\u{1f}');
            let hash = fields.next()?.trim().to_string();
            let message = fields.next()?.to_string();
            let author = fields.next()?.to_string();
            let relative_time = fields.next()?.trim().to_string();
            if hash.is_empty() {
                return None;
            }
            Some(GitCommit {
                hash,
                message,
                author,
                relative_time,
            })
        })
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::core::background_command::background_command;
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    use crate::models::{
        git::{GitChangeStatus, GitInspectionState},
        workspace::PersistedWorkspace,
    };

    #[cfg(unix)]
    use super::{find_git_executable_in_path, run_git_with_program, GitRunError};
    use super::{
        inspect, inspect_heads, list_local_branches, parse_ahead_behind, parse_commits,
        parse_status, switch_branch,
    };

    struct CleanupDir(std::path::PathBuf);

    impl Drop for CleanupDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_porcelain_changes_and_rename_without_counting_source_twice() {
        let input = b" M src/main.rs\0A  src/new.rs\0 D src/old.rs\0?? notes.txt\0R  src/new-name.rs\0src/old-name.rs\0";
        let (changes, truncated) = parse_status(input, 20);
        assert!(!truncated);
        assert_eq!(changes.len(), 5);
        assert_eq!(changes[0].status, GitChangeStatus::Modified);
        assert_eq!(changes[1].status, GitChangeStatus::Added);
        assert_eq!(changes[2].status, GitChangeStatus::Deleted);
        assert_eq!(changes[3].status, GitChangeStatus::Untracked);
        assert_eq!(changes[4].path, "src/new-name.rs");
    }

    #[test]
    fn parses_ahead_behind_and_recent_commits() {
        assert_eq!(parse_ahead_behind("3\t2\n"), Some((3, 2)));
        let commits = parse_commits(
            b"abc123\x1ffirst commit\x1fAda\x1f2 hours ago\x1edef456\x1fnext\x1fLin\x1f3 hours ago\x1e",
            12,
        );
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "abc123");
        assert_eq!(commits[0].message, "first commit");
        assert_eq!(commits[1].author, "Lin");
    }

    #[cfg(unix)]
    #[test]
    fn kills_git_inspection_that_exceeds_the_timeout() {
        use std::os::unix::fs::PermissionsExt;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-git-timeout-{unique}"));
        fs::create_dir_all(&root).expect("root");
        let program = root.join("fake-git");
        fs::write(&program, "#!/bin/sh\nsleep 2\n").expect("script");
        let mut permissions = fs::metadata(&program).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).expect("permissions");

        let started = Instant::now();
        let result = run_git_with_program(&program, &root, &["status"], Duration::from_millis(80));
        assert!(matches!(result, Err(GitRunError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(1));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_git_executable_from_inside_the_workspace() {
        use std::os::unix::fs::PermissionsExt;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-git-path-{unique}"));
        let workspace_bin = root.join("workspace/bin");
        let external_bin = root.join("external/bin");
        fs::create_dir_all(&workspace_bin).expect("workspace bin");
        fs::create_dir_all(&external_bin).expect("external bin");

        for program in [workspace_bin.join("git"), external_bin.join("git")] {
            fs::write(&program, "#!/bin/sh\nexit 0\n").expect("fake git");
            let mut permissions = fs::metadata(&program).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&program, permissions).expect("permissions");
        }

        let workspace_root = root.join("workspace");
        let path = std::env::join_paths([workspace_bin, external_bin.clone()]).expect("PATH");
        let found = find_git_executable_in_path(&workspace_root, &path, &root)
            .expect("external git should be selected");
        assert_eq!(
            found,
            fs::canonicalize(external_bin.join("git")).expect("canonical external git")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lightweight_head_inspection_reads_standard_git_head_without_spawning_git() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-git-direct-head-{unique}"));
        fs::create_dir_all(root.join(".git")).expect("git dir");
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/feature/sidebar\n").expect("head");

        let workspace = PersistedWorkspace {
            id: "direct-head".to_string(),
            name: "Direct head".to_string(),
            path: root.to_string_lossy().into_owned(),
            trusted: false,
            components: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let heads = inspect_heads(&[workspace]);
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].state, GitInspectionState::Ready);
        assert_eq!(heads[0].branch.as_deref(), Some("feature/sidebar"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lightweight_head_inspection_reports_branch_without_full_git_payload() {
        if background_command("git").arg("--version").output().is_err() {
            return;
        }

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-git-head-{unique}"));
        fs::create_dir_all(&root).expect("root");

        if !background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "-q"])
            .status()
            .expect("git init")
            .success()
        {
            let _ = fs::remove_dir_all(&root);
            return;
        }

        let current = background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["branch", "--show-current"])
            .output()
            .expect("current branch");
        let branch = String::from_utf8_lossy(&current.stdout).trim().to_string();

        let workspace = PersistedWorkspace {
            id: "head".to_string(),
            name: "Head".to_string(),
            path: root.to_string_lossy().into_owned(),
            trusted: false,
            components: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let heads = inspect_heads(&[workspace]);
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].state, GitInspectionState::Ready);
        assert_eq!(heads[0].branch.as_deref(), Some(branch.as_str()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn switches_existing_local_branch_only_when_repository_is_clean() {
        if background_command("git").arg("--version").output().is_err() {
            return;
        }

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-git-switch-{unique}"));
        fs::create_dir_all(&root).expect("root");

        let init = background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "-q"])
            .status()
            .expect("git init");
        if !init.success() {
            let _ = fs::remove_dir_all(&root);
            return;
        }

        fs::write(root.join("tracked.txt"), "one\n").expect("tracked");
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "."])
            .status()
            .expect("git add");
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args([
                "-c",
                "user.name=DECC Test",
                "-c",
                "user.email=decc@example.invalid",
                "commit",
                "-q",
                "-m",
                "initial",
            ])
            .status()
            .expect("git commit");

        let current = background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["branch", "--show-current"])
            .output()
            .expect("current branch");
        let original = String::from_utf8_lossy(&current.stdout).trim().to_string();
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["branch", "feature"])
            .status()
            .expect("create feature");

        let workspace = PersistedWorkspace {
            id: "switch".to_string(),
            name: "Switch".to_string(),
            path: root.to_string_lossy().into_owned(),
            trusted: false,
            components: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let branches = list_local_branches(&workspace).expect("branches");
        assert!(branches.contains(&original));
        assert!(branches.contains(&"feature".to_string()));

        let switched = switch_branch(&workspace, "feature").expect("switch feature");
        assert_eq!(switched.branch.as_deref(), Some("feature"));

        fs::write(root.join("tracked.txt"), "dirty\n").expect("dirty");
        let error = switch_branch(&workspace, &original).expect_err("dirty switch blocked");
        assert!(error.contains("uncommitted changes"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn isolates_non_repository_state_per_workspace() {
        if background_command("git").arg("--version").output().is_err() {
            return;
        }

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-git-isolation-{unique}"));
        let repo = root.join("repo");
        let plain = root.join("plain");
        fs::create_dir_all(&repo).expect("repo");
        fs::create_dir_all(&plain).expect("plain");

        let init = background_command("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-q"])
            .status()
            .expect("git init");
        if !init.success() {
            let _ = fs::remove_dir_all(&root);
            return;
        }

        let make_workspace = |id: &str, path: &std::path::Path| PersistedWorkspace {
            id: id.to_string(),
            name: id.to_string(),
            path: path.to_string_lossy().into_owned(),
            trusted: false,
            components: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let result = inspect(&[
            make_workspace("repo", &repo),
            make_workspace("plain", &plain),
        ]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].state, GitInspectionState::Ready);
        assert_eq!(result[1].state, GitInspectionState::NotRepository);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scopes_nested_workspace_changes_and_history_to_its_path() {
        if background_command("git").arg("--version").output().is_err() {
            return;
        }

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-git-nested-{unique}"));
        let _cleanup = CleanupDir(root.clone());
        let nested = root.join("apps/web");
        fs::create_dir_all(&nested).expect("nested");

        let init = background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "-q"])
            .status()
            .expect("git init");
        if !init.success() {
            let _ = fs::remove_dir_all(&root);
            return;
        }

        fs::write(root.join("root.txt"), "root one\n").expect("root file");
        fs::write(nested.join("tracked.txt"), "web one\n").expect("nested file");
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "."])
            .status()
            .expect("git add");
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args([
                "-c",
                "user.name=DECC Test",
                "-c",
                "user.email=decc@example.invalid",
                "commit",
                "-q",
                "-m",
                "initial nested",
            ])
            .status()
            .expect("initial commit");

        fs::write(root.join("root.txt"), "root two\n").expect("root modify");
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "root.txt"])
            .status()
            .expect("git add root");
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args([
                "-c",
                "user.name=DECC Test",
                "-c",
                "user.email=decc@example.invalid",
                "commit",
                "-q",
                "-m",
                "root only",
            ])
            .status()
            .expect("root-only commit");

        fs::write(root.join("outside.txt"), "outside untracked\n").expect("outside untracked");
        fs::write(nested.join("tracked.txt"), "web two\n").expect("nested modify");
        fs::write(nested.join("new.txt"), "web untracked\n").expect("nested untracked");

        let workspace = PersistedWorkspace {
            id: "nested".to_string(),
            name: "Nested".to_string(),
            path: nested.to_string_lossy().into_owned(),
            trusted: false,
            components: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let result = inspect(&[workspace]);
        assert_eq!(result[0].state, GitInspectionState::Ready);
        let canonical_root = fs::canonicalize(&root).expect("canonical repo root");
        let reported_root = result[0]
            .repository_root
            .as_deref()
            .expect("reported repository root");
        let canonical_reported_root =
            fs::canonicalize(reported_root).expect("canonical reported repository root");
        assert_eq!(canonical_reported_root, canonical_root);
        assert_eq!(result[0].changes.len(), 2);
        assert!(result[0]
            .changes
            .iter()
            .all(|change| !change.path.contains("outside.txt")));
        assert!(result[0]
            .commits
            .iter()
            .any(|commit| commit.message == "initial nested"));
        assert!(!result[0]
            .commits
            .iter()
            .any(|commit| commit.message == "root only"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inspects_a_real_temporary_repository_when_git_is_available() {
        if background_command("git").arg("--version").output().is_err() {
            return;
        }
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("decc-git-{unique}"));
        fs::create_dir_all(&root).expect("root");

        let init = background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "-q"])
            .status()
            .expect("git init");
        if !init.success() {
            let _ = fs::remove_dir_all(&root);
            return;
        }
        fs::write(root.join("tracked.txt"), "one\n").expect("tracked");
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "tracked.txt"])
            .status()
            .expect("git add");
        background_command("git")
            .arg("-C")
            .arg(&root)
            .args([
                "-c",
                "user.name=DECC Test",
                "-c",
                "user.email=decc@example.invalid",
                "commit",
                "-q",
                "-m",
                "initial",
            ])
            .status()
            .expect("git commit");
        fs::write(root.join("tracked.txt"), "two\n").expect("modify");
        fs::write(root.join("new.txt"), "new\n").expect("untracked");
        let index_before = fs::read(root.join(".git/index")).expect("index before");

        let workspace = PersistedWorkspace {
            id: "workspace".to_string(),
            name: "Fixture".to_string(),
            path: root.to_string_lossy().into_owned(),
            trusted: false,
            components: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let result = inspect(&[workspace]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, GitInspectionState::Ready);
        assert_eq!(result[0].changes.len(), 2);
        assert_eq!(result[0].commits.len(), 1);
        assert_eq!(result[0].commits[0].message, "initial");
        let index_after = fs::read(root.join(".git/index")).expect("index after");
        assert_eq!(index_after, index_before);

        let _ = fs::remove_dir_all(&root);
    }
}
