use crate::models::detection::{DetectedCandidate, DetectionConfidence};

use super::{directory_name, read_text, DirectorySnapshot};

pub fn detect(directory: &DirectorySnapshot) -> Vec<DetectedCandidate> {
    if !directory.has_file("go.mod") || !has_main_package(directory) {
        return Vec::new();
    }

    let name = module_name(directory).unwrap_or_else(|| directory_name(directory));

    vec![DetectedCandidate::new(
        "go",
        &directory.relative_path,
        name,
        "Go",
        "Go",
        "go",
        vec!["run".to_string(), ".".to_string()],
        vec![],
        DetectionConfidence::High,
    )]
}

fn has_main_package(directory: &DirectorySnapshot) -> bool {
    directory.files.iter().any(|file| {
        if !file.ends_with(".go") || file.ends_with("_test.go") {
            return false;
        }
        let Some(text) = read_text(&directory.absolute_path.join(file)) else {
            return false;
        };
        text.lines()
            .map(str::trim)
            .any(|line| line == "package main")
    })
}

fn module_name(directory: &DirectorySnapshot) -> Option<String> {
    let text = read_text(&directory.absolute_path.join("go.mod"))?;
    let module = text
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("module "))?
        .trim();
    module
        .split('/')
        .next_back()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}
