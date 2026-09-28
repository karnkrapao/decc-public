mod dotnet;
mod go;
mod javascript;
mod orchestration;
mod rust;

use std::{collections::HashSet, path::PathBuf};

use crate::models::detection::DetectedCandidate;

#[derive(Debug)]
pub struct DirectorySnapshot {
    pub absolute_path: PathBuf,
    pub relative_path: String,
    pub files: HashSet<String>,
}

impl DirectorySnapshot {
    pub fn has_file(&self, name: &str) -> bool {
        self.files.contains(name)
    }
}

pub fn detect(root: &std::path::Path, directory: &DirectorySnapshot) -> Vec<DetectedCandidate> {
    let mut candidates = Vec::new();
    candidates.extend(javascript::detect(root, directory));
    candidates.extend(go::detect(directory));
    candidates.extend(rust::detect(directory));
    candidates.extend(dotnet::detect(directory));
    candidates.extend(orchestration::detect(directory));
    candidates
}

pub(super) fn directory_name(directory: &DirectorySnapshot) -> String {
    directory
        .absolute_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace")
        .to_string()
}

pub(super) fn read_text(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}
