use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DetectionConfidence {
    High,
    Possible,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedCandidate {
    pub id: String,
    pub name: String,
    pub path: String,
    pub technology: String,
    pub runtime: String,
    pub command: String,
    pub program: String,
    pub args: Vec<String>,
    pub expected_ports: Vec<u16>,
    pub confidence: DetectionConfidence,
    pub included: bool,
}

impl DetectedCandidate {
    pub fn new(
        detector: &str,
        relative_path: &str,
        name: String,
        technology: impl Into<String>,
        runtime: impl Into<String>,
        program: impl Into<String>,
        args: Vec<String>,
        expected_ports: Vec<u16>,
        confidence: DetectionConfidence,
    ) -> Self {
        let confidence_included = matches!(confidence, DetectionConfidence::High);
        let identity = if relative_path == "." {
            format!("{detector}-{name}")
        } else {
            format!("{detector}-{relative_path}-{name}")
        };
        let program = program.into();
        let command = format_command(&program, &args);

        Self {
            id: slugify(&identity),
            name,
            path: relative_path.to_string(),
            technology: technology.into(),
            runtime: runtime.into(),
            command,
            program,
            args,
            expected_ports,
            confidence,
            included: confidence_included,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceScanResult {
    pub root_path: String,
    pub candidates: Vec<DetectedCandidate>,
    pub scanned_directories: usize,
}

fn format_command(program: &str, args: &[String]) -> String {
    std::iter::once(display_token(program))
        .chain(args.iter().map(|arg| display_token(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_token(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_./:@=".contains(character))
    {
        return value.to_string();
    }

    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn slugify(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut previous_dash = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            output.push('-');
            previous_dash = true;
        }
    }

    output.trim_matches('-').to_string()
}
