//! Pure domain types shared across all CHM crates. No I/O.

pub mod domain;

pub use domain::*;

/// Parses a stored RFC3339 timestamp. Corrupt values are surfaced as an
/// error rather than silently rewritten.
pub fn parse_ts(s: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| format!("invalid timestamp {s:?}: {e}"))
}
