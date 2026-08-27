//! Version detection: run the harness binary, parse a semver-like token.

use std::process::Command;

const VERSION_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

use crate::definition::HarnessDefinition;

pub fn version_args_for(_def: &HarnessDefinition) -> &'static [&'static str] {
    // Per-harness overrides recorded in docs/harnesses/detection.md.
    // Default is universal: `--version`.
    &["--version"]
}

pub fn detect_version(executable_path: &str, version_args: &[&str]) -> Option<String> {
    let mut child = Command::new(executable_path)
        .args(version_args)
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + VERSION_PROBE_TIMEOUT;
    let status = loop {
        match child.try_wait().ok().flatten() {
            Some(st) => break Some(st),
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    }?;
    if !status.success() {
        return None;
    }
    let out = child.wait_with_output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let all = format!("{stdout}\n{stderr}");
    all.split_whitespace()
        .map(|tok| tok.trim_matches(|c: char| !c.is_ascii_digit() && c != '.'))
        .find(|tok| {
            let mut parts = tok.split('.');
            let major = parts.next().and_then(|p| p.parse::<u64>().ok());
            let minor = parts.next().and_then(|p| p.parse::<u64>().ok());
            matches!((major, minor), (Some(_), Some(_)))
        })
        .map(|tok| tok.to_string())
}
