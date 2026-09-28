use std::{
    collections::HashMap,
    process::{Command, Stdio},
};

#[cfg(unix)]
use std::collections::HashSet;

use crate::models::process::{ProcessStatus, ProcessUsage};

pub fn inspect(processes: &[ProcessStatus]) -> Result<Vec<ProcessUsage>, String> {
    if processes.is_empty() {
        return Ok(Vec::new());
    }

    #[cfg(unix)]
    {
        inspect_unix(processes)
    }

    #[cfg(windows)]
    {
        inspect_windows(processes)
    }

    #[cfg(not(any(unix, windows)))]
    {
        Ok(processes
            .iter()
            .map(|process| ProcessUsage {
                workspace_id: process.workspace_id.clone(),
                component_id: process.component_id.clone(),
                pid: process.pid,
                cpu_percent: None,
                memory_bytes: None,
            })
            .collect())
    }
}

#[cfg(unix)]
fn inspect_unix(processes: &[ProcessStatus]) -> Result<Vec<ProcessUsage>, String> {
    let ps = if std::path::Path::new("/bin/ps").is_file() {
        "/bin/ps"
    } else {
        "/usr/bin/ps"
    };
    let output = Command::new(ps)
        .args(["-axo", "pid=,ppid=,%cpu=,rss="])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not inspect process resource usage: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Process resource inspection exited with status {}.",
            output.status
        ));
    }

    #[derive(Clone, Copy)]
    struct Sample {
        parent: u32,
        cpu: f32,
        rss_kib: u64,
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut samples = HashMap::<u32, Sample>::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(parent), Some(cpu), Some(rss)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (Ok(pid), Ok(parent), Ok(cpu), Ok(rss_kib)) = (
            pid.parse::<u32>(),
            parent.parse::<u32>(),
            cpu.parse::<f32>(),
            rss.parse::<u64>(),
        ) else {
            continue;
        };
        samples.insert(
            pid,
            Sample {
                parent,
                cpu,
                rss_kib,
            },
        );
    }

    Ok(processes
        .iter()
        .map(|process| {
            let mut family = HashSet::from([process.pid]);
            loop {
                let before = family.len();
                for (pid, sample) in &samples {
                    if family.contains(&sample.parent) {
                        family.insert(*pid);
                    }
                }
                if family.len() == before {
                    break;
                }
            }

            let mut cpu = 0.0_f32;
            let mut memory_bytes = 0_u64;
            let mut observed = false;
            for pid in family {
                if let Some(sample) = samples.get(&pid) {
                    observed = true;
                    cpu += sample.cpu;
                    memory_bytes = memory_bytes.saturating_add(sample.rss_kib.saturating_mul(1024));
                }
            }

            ProcessUsage {
                workspace_id: process.workspace_id.clone(),
                component_id: process.component_id.clone(),
                pid: process.pid,
                cpu_percent: observed.then_some(cpu),
                memory_bytes: observed.then_some(memory_bytes),
            }
        })
        .collect())
}

#[cfg(windows)]
fn inspect_windows(processes: &[ProcessStatus]) -> Result<Vec<ProcessUsage>, String> {
    let output = Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not inspect process memory usage: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Process resource inspection exited with status {}.",
            output.status
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut memory = HashMap::<u32, u64>::new();
    for line in text.lines() {
        let fields = parse_csv_line(line);
        if fields.len() < 5 {
            continue;
        }
        let Ok(pid) = fields[1].parse::<u32>() else {
            continue;
        };
        let digits = fields[4]
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>();
        if let Ok(kib) = digits.parse::<u64>() {
            memory.insert(pid, kib.saturating_mul(1024));
        }
    }

    Ok(processes
        .iter()
        .map(|process| ProcessUsage {
            workspace_id: process.workspace_id.clone(),
            component_id: process.component_id.clone(),
            pid: process.pid,
            cpu_percent: None,
            memory_bytes: memory.get(&process.pid).copied(),
        })
        .collect())
}

#[cfg(windows)]
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    fields.push(current.trim().to_string());
    fields
}
