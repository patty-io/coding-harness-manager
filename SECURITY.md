# Security Policy

## Reporting

Report vulnerabilities via GitHub's private vulnerability reporting on this
repository. Please do not open public issues for security problems.

## Design guarantees

- API keys are stored only in OS-native secret stores (Keychain / Credential
  Manager / libsecret) or referenced by environment variable name.
- SQLite holds credential *references*, never values.
- No telemetry, no prompts read, no source uploaded.
- Diagnostic bundles pass through a redaction pass before export.
