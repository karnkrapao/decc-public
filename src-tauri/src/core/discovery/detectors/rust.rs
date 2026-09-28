use crate::models::detection::{DetectedCandidate, DetectionConfidence};

use super::{directory_name, read_text, DirectorySnapshot};

pub fn detect(directory: &DirectorySnapshot) -> Vec<DetectedCandidate> {
    if !directory.has_file("Cargo.toml") {
        return Vec::new();
    }

    let Some(text) = read_text(&directory.absolute_path.join("Cargo.toml")) else {
        return Vec::new();
    };

    if !has_package_section(&text) {
        return Vec::new();
    }

    let name = package_name(&text).unwrap_or_else(|| directory_name(directory));

    let (args, confidence) = if directory.absolute_path.join("src/main.rs").is_file() {
        (vec!["run".to_string()], DetectionConfidence::High)
    } else if let Some(bin_name) = first_bin_name(&text) {
        (
            vec!["run".to_string(), "--bin".to_string(), bin_name],
            DetectionConfidence::High,
        )
    } else {
        return Vec::new();
    };

    vec![DetectedCandidate::new(
        "rust",
        &directory.relative_path,
        name,
        "Rust",
        "Cargo",
        "cargo",
        args,
        vec![],
        confidence,
    )]
}

fn has_package_section(text: &str) -> bool {
    text.lines().any(|line| line.trim() == "[package]")
}

fn package_name(text: &str) -> Option<String> {
    value_in_section(text, "[package]", "name")
}

fn first_bin_name(text: &str) -> Option<String> {
    value_in_section(text, "[[bin]]", "name")
}

fn value_in_section(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == section;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((candidate_key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if candidate_key.trim() != key {
            continue;
        }
        let value = value.trim().trim_matches(['"', '\'']);
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }

    None
}
