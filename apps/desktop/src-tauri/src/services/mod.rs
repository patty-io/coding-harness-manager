pub mod credential_deployment;
pub mod import;
pub mod transactions;

/// Normalize provider endpoint bases for identity comparisons. Endpoint
/// matching is case-insensitive and treats a trailing slash as formatting,
/// not a distinct gateway.
pub(crate) fn normalize_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_lowercase()
}
