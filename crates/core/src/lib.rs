//! Pure domain types shared across all CHM crates. No I/O.

/// Wires an enum as its stable string form using existing
/// `as_str()` / `parse_str()` methods — never an externally-tagged object.
#[macro_export]
macro_rules! wire_serializable_enum {
    ($t:ident) => {
        impl serde::Serialize for $t {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }
        impl<'de> serde::Deserialize<'de> for $t {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                Ok(<$t>::parse_str(&s))
            }
        }
    };
}

pub mod domain;

pub use domain::*;

/// Parses a stored RFC3339 timestamp. Corrupt values are surfaced as an
/// error rather than silently rewritten.
pub fn parse_ts(s: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| format!("invalid timestamp {s:?}: {e}"))
}
