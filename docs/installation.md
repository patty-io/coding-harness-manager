# Installation

## Downloads

Grab the installer for your platform from the
[Releases](../../releases) page:

| Platform | Artifact |
|----------|----------|
| macOS (Apple Silicon) | `.dmg` / `.app` (`aarch64`) |
| macOS (Intel) | `.dmg` / `.app` (`x86_64`) |
| Windows | `.msi` or NSIS `.exe` |
| Linux | `.AppImage` or `.deb` |

## macOS unsigned-build warning

macOS builds are unsigned until a signing certificate is configured.
Gatekeeper will block first launch. Either right-click → **Open**, or run:

```bash
xattr -d com.apple.quarantine /Applications/Coding\ Harness\ Manager.app
```

## First run

1. Open the app and click **Scan Harnesses** — supported harnesses appear
   with versions and config paths.
2. Run the **Import Wizard** to bring existing providers/models/MCP/skills
   into the central registry. Nothing on disk is modified during import.
3. Add a provider endpoint and store its API key in your OS keychain.
4. Discover models, add them to My Models, then use **Sync** on a harness —
   preview the diff before applying.

## Uninstall

Remove the app bundle plus `~/.coding-harness-manager/` (the SQLite registry).
Harness configs remain untouched — roll back any synced changes from History
first if you want files restored.
