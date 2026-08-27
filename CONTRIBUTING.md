# Contributing to Coding Harness Manager

## Development setup

Prerequisites: Rust stable, Node 22, and the platform dependencies for
Tauri 2 (see docs/development.md).

```bash
git clone <repo>
cd coding-harness-manager
npm install --prefix apps/desktop
cargo test --workspace
npm run build --prefix apps/desktop
```

Run the desktop app in dev mode:

```bash
npm run tauri dev --prefix apps/desktop
```

## Adding a harness adapter

1. Research the harness's native config first — fill in
   `docs/harnesses/<id>.md` from `docs/harnesses/_template.md`.
2. Collect real config snapshots into `fixtures/<id>/<version>/`
   (redact secrets, keep structure identical).
3. Implement a read-only parser + golden tests against those fixtures.
4. Only then implement write support with atomic edits.
5. All tests must pass: fixture goldens lock behavior; never edit fixtures.

## Rules

- Secrets never touch SQLite or fixtures (only credential references).
- Every mutating action is previewed before apply.
- Clippy runs with `-D warnings` in CI — keep it clean.
