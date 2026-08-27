//! Version detection: run the harness binary, parse a semver-like token.

use std::process::Command;

use crate::definition::HarnessDefinition;

pub fn version_args_for(_def: &HarnessDefinition) -> &'static [&'static str] {
    // Per-harness overrides recorded in docs/harnesses/detection.md.
    // Default is universal: `--version`.
    &["--version"]
}

pub fn detect_version(executable_path: &str, version_args: &[&str]) -> Option<String> {
    let out = Command::new(executable_path)
        .args(version_args)
        .output()
        .ok()?;
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
