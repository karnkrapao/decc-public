use crate::models::detection::{DetectedCandidate, DetectionConfidence};

use super::{read_text, DirectorySnapshot};

pub fn detect(directory: &DirectorySnapshot) -> Vec<DetectedCandidate> {
    let mut projects: Vec<_> = directory
        .files
        .iter()
        .filter(|file| file.ends_with(".csproj"))
        .cloned()
        .collect();
    projects.sort();

    projects
        .into_iter()
        .filter_map(|project| detect_project(directory, &project))
        .collect()
}

fn detect_project(directory: &DirectorySnapshot, project: &str) -> Option<DetectedCandidate> {
    let text = read_text(&directory.absolute_path.join(project))?;
    let is_web = text.contains("Microsoft.NET.Sdk.Web");
    let output_type = xml_value(&text, "OutputType");
    let runnable = is_web
        || matches!(
            output_type
                .as_deref()
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("exe") | Some("winexe")
        );

    if !runnable {
        return None;
    }

    let name = xml_value(&text, "AssemblyName")
        .or_else(|| project.strip_suffix(".csproj").map(str::to_string))
        .unwrap_or_else(|| "dotnet-app".to_string());
    let technology = if is_web { "ASP.NET Core" } else { ".NET" };

    Some(DetectedCandidate::new(
        "dotnet",
        &directory.relative_path,
        name,
        technology,
        ".NET",
        "dotnet",
        vec![
            "run".to_string(),
            "--project".to_string(),
            project.to_string(),
        ],
        vec![],
        DetectionConfidence::High,
    ))
}

fn xml_value(text: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{tag}>");
    let end_tag = format!("</{tag}>");
    let start = text.find(&start_tag)? + start_tag.len();
    let end = text[start..].find(&end_tag)? + start;
    let value = text[start..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}
