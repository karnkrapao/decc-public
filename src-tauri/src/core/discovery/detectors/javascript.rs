use std::{collections::BTreeMap, path::Path};

use serde::Deserialize;

use crate::models::detection::{DetectedCandidate, DetectionConfidence};

use super::{directory_name, read_text, DirectorySnapshot};

#[derive(Debug, Deserialize)]
struct PackageJson {
    name: Option<String>,
    #[serde(default)]
    scripts: BTreeMap<String, String>,
    #[serde(default)]
    dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "packageManager")]
    package_manager: Option<String>,
}

pub fn detect(root: &Path, directory: &DirectorySnapshot) -> Vec<DetectedCandidate> {
    if !directory.has_file("package.json") {
        return Vec::new();
    }

    let package_path = directory.absolute_path.join("package.json");
    let Some(text) = read_text(&package_path) else {
        return Vec::new();
    };
    let Ok(package) = serde_json::from_str::<PackageJson>(&text) else {
        return Vec::new();
    };

    let Some((script, confidence)) = select_primary_script(&package.scripts) else {
        return Vec::new();
    };

    let package_manager = detect_package_manager(
        root,
        &directory.absolute_path,
        package.package_manager.as_deref(),
    );
    let expected_ports = package
        .scripts
        .get(&script)
        .map(|command| extract_explicit_ports(command))
        .unwrap_or_default();
    let technology = detect_technology(&package);
    let runtime = package_manager.runtime_label();
    let name = package
        .name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| directory_name(directory));

    vec![DetectedCandidate::new(
        "javascript",
        &directory.relative_path,
        name,
        technology,
        runtime,
        package_manager.command_name(),
        vec!["run".to_string(), script],
        expected_ports,
        confidence,
    )]
}

fn select_primary_script(
    scripts: &BTreeMap<String, String>,
) -> Option<(String, DetectionConfidence)> {
    for name in ["dev", "start", "serve"] {
        if scripts.contains_key(name) {
            return Some((name.to_string(), DetectionConfidence::High));
        }
    }

    for name in ["preview", "storybook"] {
        if scripts.contains_key(name) {
            return Some((name.to_string(), DetectionConfidence::Possible));
        }
    }

    None
}

fn extract_explicit_ports(command: &str) -> Vec<u16> {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let mut ports = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let token = tokens[index]
            .trim_matches(|character: char| matches!(character, '\'' | '\"' | ',' | ';'));
        let candidate = if let Some(value) = token.strip_prefix("--port=") {
            Some(value)
        } else if token == "--port" || token == "-p" {
            tokens.get(index + 1).copied()
        } else if token.starts_with("-p") && token.len() > 2 {
            token.get(2..)
        } else if let Some(value) = token.strip_prefix("PORT=") {
            Some(value)
        } else {
            None
        };

        if let Some(value) = candidate {
            let value = value.trim_matches(|character: char| !character.is_ascii_digit());
            if let Ok(port) = value.parse::<u16>() {
                if port > 0 && !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
        index += 1;
    }

    ports.sort_unstable();
    ports
}

fn detect_technology(package: &PackageJson) -> &'static str {
    let has = |name: &str| {
        package.dependencies.contains_key(name) || package.dev_dependencies.contains_key(name)
    };

    if has("next") {
        "Next.js"
    } else if has("@remix-run/react") {
        "Remix"
    } else if has("nuxt") {
        "Nuxt"
    } else if has("svelte") || has("@sveltejs/kit") {
        "Svelte"
    } else if has("vite") {
        "Vite"
    } else if has("react") {
        "React"
    } else {
        "Node.js"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    fn command_name(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    fn runtime_label(self) -> &'static str {
        match self {
            Self::Bun => "Bun",
            Self::Npm | Self::Pnpm | Self::Yarn => "Node.js",
        }
    }
}

fn detect_package_manager(root: &Path, start: &Path, declared: Option<&str>) -> PackageManager {
    if let Some(manager) = declared
        .and_then(|value| value.split('@').next())
        .map(str::trim)
    {
        match manager {
            "bun" => return PackageManager::Bun,
            "pnpm" => return PackageManager::Pnpm,
            "yarn" => return PackageManager::Yarn,
            "npm" => return PackageManager::Npm,
            _ => {}
        }
    }

    let mut current = Some(start);

    while let Some(directory) = current {
        if directory.join("bun.lock").is_file() || directory.join("bun.lockb").is_file() {
            return PackageManager::Bun;
        }
        if directory.join("pnpm-lock.yaml").is_file() {
            return PackageManager::Pnpm;
        }
        if directory.join("yarn.lock").is_file() {
            return PackageManager::Yarn;
        }
        if directory.join("package-lock.json").is_file() {
            return PackageManager::Npm;
        }

        if directory == root {
            break;
        }
        current = directory.parent().filter(|parent| parent.starts_with(root));
    }

    PackageManager::Npm
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{detect_package_manager, extract_explicit_ports, PackageManager};

    #[test]
    fn package_manager_field_wins_before_lockfile_fallback() {
        assert_eq!(
            detect_package_manager(
                Path::new("/workspace"),
                Path::new("/workspace/app"),
                Some("bun@1.3.14"),
            ),
            PackageManager::Bun,
        );
        assert_eq!(
            detect_package_manager(
                Path::new("/workspace"),
                Path::new("/workspace/app"),
                Some("yarn@4.9.2"),
            ),
            PackageManager::Yarn,
        );
    }

    #[test]
    fn extracts_only_explicit_port_arguments() {
        assert_eq!(extract_explicit_ports("vite --port 5173"), vec![5173]);
        assert_eq!(extract_explicit_ports("next dev -p 3001"), vec![3001]);
        assert_eq!(
            extract_explicit_ports("PORT=8080 node server.js"),
            vec![8080]
        );
        assert_eq!(extract_explicit_ports("vite"), Vec::<u16>::new());
    }
}
