use crate::models::detection::{DetectedCandidate, DetectionConfidence};

use super::{directory_name, read_text, DirectorySnapshot};

const COMPOSE_FILES: &[&str] = &[
    "compose.yml",
    "compose.yaml",
    "docker-compose.yml",
    "docker-compose.yaml",
];

pub fn detect(directory: &DirectorySnapshot) -> Vec<DetectedCandidate> {
    let mut candidates = Vec::new();

    if COMPOSE_FILES.iter().any(|file| directory.has_file(file)) {
        candidates.push(DetectedCandidate::new(
            "compose",
            &directory.relative_path,
            format!("{} infrastructure", directory_name(directory)),
            "Docker Compose",
            "Docker",
            "docker",
            vec!["compose".to_string(), "up".to_string()],
            vec![],
            DetectionConfidence::High,
        ));
    }

    if directory.has_file("Makefile") {
        candidates.extend(make_candidates(directory));
    }

    for taskfile in ["Taskfile.yml", "Taskfile.yaml"] {
        if directory.has_file(taskfile) {
            if let Some(target) = task_target(directory, taskfile) {
                candidates.push(command_candidate(directory, "task", "Task", target));
            }
            break;
        }
    }

    if directory.has_file("justfile") {
        if let Some(target) = just_target(directory) {
            candidates.push(command_candidate(directory, "just", "just", target));
        }
    }

    candidates
}

fn command_candidate(
    directory: &DirectorySnapshot,
    detector: &str,
    technology: &str,
    target: String,
) -> DetectedCandidate {
    DetectedCandidate::new(
        detector,
        &directory.relative_path,
        format!("{} dev", directory_name(directory)),
        technology,
        "Project command",
        detector,
        vec![target],
        vec![],
        DetectionConfidence::High,
    )
}

fn make_candidates(directory: &DirectorySnapshot) -> Vec<DetectedCandidate> {
    let Some(text) = read_text(&directory.absolute_path.join("Makefile")) else {
        return Vec::new();
    };
    let targets = text
        .lines()
        .filter_map(|line| {
            let line = line.trim_end();
            if line.starts_with([' ', '\t', '#', '.']) {
                return None;
            }
            let (name, _) = line.split_once(':')?;
            let name = name.trim();
            (!name.is_empty() && !name.contains(' ')).then(|| name.to_string())
        })
        .collect::<Vec<_>>();
    let Some(target) = pick_named_target(targets.into_iter()) else {
        return Vec::new();
    };

    let apps = make_word_list(&text, &["VALID_APPS", "APPS"]);
    let positional_apps = !apps.is_empty()
        && text.contains("MAKECMDGOALS")
        && (text.contains("VALID_APPS") || text.contains("$(APPS)"));

    if !apps.is_empty() && target == "run" {
        return apps
            .into_iter()
            .map(|app| {
                let args = if positional_apps {
                    vec![target.clone(), app.clone()]
                } else {
                    vec![target.clone(), format!("APP={app}")]
                };
                DetectedCandidate::new(
                    "make",
                    &directory.relative_path,
                    format!("{} {}", directory_name(directory), app),
                    "Make",
                    "Project command",
                    "make",
                    args,
                    vec![],
                    DetectionConfidence::High,
                )
            })
            .collect();
    }

    vec![command_candidate(directory, "make", "Make", target)]
}

fn make_word_list(text: &str, keys: &[&str]) -> Vec<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        for key in keys {
            let Some(rest) = line.strip_prefix(key) else {
                continue;
            };
            let Some((operator, value)) = rest.trim_start().split_once('=') else {
                continue;
            };
            if !operator.trim().is_empty() && !matches!(operator.trim(), ":" | "?" | "+") {
                continue;
            }
            let values = value
                .split_whitespace()
                .map(str::trim)
                .filter(|value| {
                    !value.is_empty()
                        && value.chars().all(|character| {
                            character.is_ascii_alphanumeric() || "-_.".contains(character)
                        })
                })
                .map(str::to_string)
                .collect::<Vec<_>>();
            if !values.is_empty() {
                return values;
            }
        }
    }
    Vec::new()
}

fn task_target(directory: &DirectorySnapshot, file: &str) -> Option<String> {
    let text = read_text(&directory.absolute_path.join(file))?;
    let mut in_tasks = false;
    let mut names = Vec::new();

    for line in text.lines() {
        if line.trim() == "tasks:" {
            in_tasks = true;
            continue;
        }
        if !in_tasks {
            continue;
        }
        if !line.starts_with("  ") || line.starts_with("    ") {
            if !line.trim().is_empty() && !line.starts_with('#') {
                break;
            }
            continue;
        }
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_suffix(':') {
            if !name.is_empty() && !name.contains(' ') {
                names.push(name.to_string());
            }
        }
    }

    pick_named_target(names.into_iter())
}

fn just_target(directory: &DirectorySnapshot) -> Option<String> {
    let text = read_text(&directory.absolute_path.join("justfile"))?;
    pick_named_target(text.lines().filter_map(|line| {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with([' ', '\t', '#']) || trimmed.contains(":=") {
            return None;
        }
        let (name, _) = trimmed.split_once(':')?;
        let name = name.split_whitespace().next()?;
        (!name.is_empty()).then(|| name.to_string())
    }))
}

fn pick_named_target<I>(targets: I) -> Option<String>
where
    I: IntoIterator<Item = String>,
{
    let targets: Vec<String> = targets.into_iter().collect();
    for preferred in ["dev", "run", "start", "up"] {
        if targets.iter().any(|target| target == preferred) {
            return Some(preferred.to_string());
        }
    }
    None
}
