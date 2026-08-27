# Adapter Fixtures

Static, real config snapshots used by adapter golden tests.
Rules:
- Path: fixtures/<harness-id>/<version>/<description>.<ext>
  e.g. fixtures/codex/0.24.0/config-toml-full.toml
- Copies of real configs; contents redacted only for secrets
  (API keys → sk-EXAMPLE..., tokens → REDACTED). Keep structure identical.
- Never edit values to "make tests pass" — if a fixture breaks a test,
  the adapter is wrong, not the fixture.
- Each fixture file may start with a comment line: `# fixture: <what it covers>`
- Version dir `unknown/` is allowed when version could not be observed.