# Development

## Prerequisites

- Rust stable (1.85+; edition 2024)
- Node.js 22 + npm
- macOS: Xcode CLT
- Windows: VS Build Tools + WebView2
- Linux: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libsecret-1-dev`

## Commands

```bash
cargo test --workspace            # backend tests (68+)
cargo clippy --workspace --all-targets -- -D warnings
npm run tauri dev --prefix apps/desktop   # run the app
npm run lint --prefix apps/desktop        # TS typecheck
npm run build --prefix apps/desktop       # frontend build
```

## Adding a fixture

Fixtures are real config snapshots under `fixtures/<harness>/<version>/`.
Copy the file from a machine where the harness is configured, redact only
secret values (structure stays identical), and never edit a fixture to make a
test pass — the adapter is wrong, not the fixture.

## Adding an adapter

See CONTRIBUTING.md. The adapter contract lives in
`crates/harness-sdk/src/adapter/` (`HarnessAdapter` trait); shared detection
and parsing helpers are in `crates/harness-sdk/src/detect/` and
`adapter/helpers.rs` so adapters stay thin.
