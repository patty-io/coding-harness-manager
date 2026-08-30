//! Registry of harness definitions. Data comes from the official harness
//! documentation and is kept in lockstep with the adapter registry.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

#[derive(Debug, Clone)]
pub struct HarnessDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub executable_names: &'static [&'static str],
    pub config_paths: &'static [&'static str],
    pub skill_paths: &'static [&'static str],
    pub mcp_paths: &'static [&'static str],
    pub platforms: &'static [Platform],
    pub detection_only: bool,
}

const fn def(
    id: &'static str,
    name: &'static str,
    executables: &'static [&'static str],
    config: &'static [&'static str],
    skills: &'static [&'static str],
    mcp: &'static [&'static str],
    detection_only: bool,
) -> HarnessDefinition {
    HarnessDefinition {
        id,
        name,
        executable_names: executables,
        config_paths: config,
        skill_paths: skills,
        mcp_paths: mcp,
        platforms: &[Platform::MacOs, Platform::Windows, Platform::Linux],
        detection_only,
    }
}

pub fn tier1_definitions() -> Vec<HarnessDefinition> {
    vec![
        def(
            "claude-code",
            "Claude Code",
            &["claude", "claude-code"],
            &[".claude/settings.json", ".claude.json"],
            &[".claude/skills"],
            &[".claude.json"],
            false,
        ),
        def(
            "codex",
            "Codex",
            &["codex"],
            // per-provider files (<id>.config.toml) verified on 0.150; main config is the anchor
            &[".codex/config.toml"],
            &[".codex/skills"],
            &[".codex/config.toml"],
            false,
        ),
        def(
            "opencode",
            "OpenCode",
            &["opencode"],
            // real file is opencode.jsonc (1.18 observed); legacy .json kept as fallback
            &[
                ".config/opencode/opencode.jsonc",
                ".config/opencode/opencode.json",
            ],
            &[".config/opencode/skills"],
            &[".config/opencode/opencode.jsonc"],
            false,
        ),
        def(
            "pi",
            "Pi",
            &["pi"],
            // 0.84 uses JSON (models.json/mcp.json/settings.json); legacy config.toml fallback
            &[
                ".pi/agent/models.json",
                ".pi/agent/mcp.json",
                ".pi/agent/config.toml",
            ],
            &[".pi/agent/skills"],
            &[".pi/agent/mcp.json"],
            false,
        ),
        def(
            "reasonix",
            "Reasonix",
            &["reasonix"],
            &[".reasonix/config.toml"],
            &[".reasonix/skills"],
            &[".reasonix/mcp-state"],
            false,
        ),
    ]
}

/// Additional harness definitions beyond the five first-class harnesses.
/// Every definition in this list has a registered adapter; the adapters may
/// expose different native surfaces depending on what the harness persists
/// on disk (for example, model selection rather than a model registry).
pub fn additional_definitions() -> Vec<HarnessDefinition> {
    vec![
        def(
            "gemini-cli",
            "Gemini CLI",
            &["gemini"],
            &[".gemini/settings.json"],
            &[".gemini/skills"],
            &[".gemini/settings.json", ".gemini/mcp_config.json"],
            false,
        ),
        def(
            "qwen-code",
            "Qwen Code",
            &["qwen-code", "qwen"],
            &[".qwen/settings.json"],
            &[],
            &[".qwen/settings.json"],
            false,
        ),
        def(
            "kimi-cli",
            "Kimi CLI",
            &["kimi"],
            &[
                ".kimi/config.toml",
                ".kimi/config.json",
                ".kimi-code/config.toml",
                ".kimi-code/config.json",
            ],
            &[".kimi/skills", ".kimi-code/skills"],
            &[".kimi/mcp.json", ".kimi-code/mcp.json"],
            false,
        ),
        def(
            "cursor",
            "Cursor",
            // Cursor's current CLI installs as `agent`; keep the older
            // `cursor`/`cursor-agent` names for existing installations.
            &["agent", "cursor", "cursor-agent"],
            &[".cursor/cli-config.json", ".cursor/mcp.json"],
            &[],
            &[".cursor/mcp.json"],
            false,
        ),
        def(
            "cline",
            "Cline",
            &["cline"],
            &[
                ".cline/data/settings/providers.json",
                ".cline/data/settings/models.json",
                ".cline/mcp.json",
                "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
                ".config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
                "Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
            ],
            &[".cline/data/settings/skills", ".cline/skills"],
            &[
                ".cline/mcp.json",
                ".cline/data/settings/cline_mcp_settings.json",
                "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
                ".config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
                "Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
            ],
            false,
        ),
        def(
            "roo-code",
            "Roo Code",
            &["roo"],
            &[
                ".roo/mcp.json",
                ".roo/settings.json",
                "Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json",
                "Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json",
                "Library/Application Support/Code/User/globalStorage/roocode.roo-cline/settings/mcp_settings.json",
                "Library/Application Support/Code/User/globalStorage/roocode.roo-cline/settings/cline_mcp_settings.json",
                ".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json",
                ".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json",
                ".config/Code/User/globalStorage/roocode.roo-cline/settings/mcp_settings.json",
                ".config/Code/User/globalStorage/roocode.roo-cline/settings/cline_mcp_settings.json",
                "Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json",
                "Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json",
                "Code/User/globalStorage/roocode.roo-cline/settings/mcp_settings.json",
                "Code/User/globalStorage/roocode.roo-cline/settings/cline_mcp_settings.json",
            ],
            &[".roo/skills"],
            &[
                ".roo/mcp.json",
                "Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json",
                "Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json",
                "Library/Application Support/Code/User/globalStorage/roocode.roo-cline/settings/mcp_settings.json",
                "Library/Application Support/Code/User/globalStorage/roocode.roo-cline/settings/cline_mcp_settings.json",
                ".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json",
                ".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json",
                ".config/Code/User/globalStorage/roocode.roo-cline/settings/mcp_settings.json",
                ".config/Code/User/globalStorage/roocode.roo-cline/settings/cline_mcp_settings.json",
                "Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json",
                "Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json",
                "Code/User/globalStorage/roocode.roo-cline/settings/mcp_settings.json",
                "Code/User/globalStorage/roocode.roo-cline/settings/cline_mcp_settings.json",
            ],
            false,
        ),
        def(
            "aider",
            "Aider",
            &["aider"],
            &[".aider.conf.yml"],
            &[],
            &[],
            false,
        ),
        def(
            "amp",
            "Amp",
            &["amp"],
            &[
                ".config/amp/settings.json",
                ".config/amp/settings.jsonc",
                ".amp/settings.json",
            ],
            &[".config/amp/skills"],
            &[
                ".config/amp/settings.json",
                ".config/amp/settings.jsonc",
                ".amp/settings.json",
            ],
            false,
        ),
        def(
            "goose",
            "Goose",
            &["goose"],
            &[".config/goose/config.yaml", ".config/goose/config.yml"],
            &[".config/goose/skills"],
            &[],
            false,
        ),
        def(
            "continue",
            "Continue",
            &["continue"],
            &[
                ".continue/config.yaml",
                ".continue/config.yml",
                ".continue/config.json",
            ],
            &[],
            &[
                ".continue/config.yaml",
                ".continue/config.yml",
                ".continue/config.json",
            ],
            false,
        ),
    ]
}

/// Compatibility alias for clients compiled against the V1 registry.  These
/// definitions are no longer detection-only; they are backed by adapters.
pub fn detection_only_definitions() -> Vec<HarnessDefinition> {
    additional_definitions()
}

pub fn all_definitions() -> Vec<HarnessDefinition> {
    let mut all = tier1_definitions();
    all.extend(additional_definitions());
    all
}
