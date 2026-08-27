//! Registry of harness definitions. Data comes from docs/harnesses/detection.md
//! (verified 2026-08-27 against real installs).

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

pub fn detection_only_definitions() -> Vec<HarnessDefinition> {
    vec![
        def(
            "gemini-cli",
            "Gemini CLI",
            &["gemini"],
            &[".gemini"],
            &[],
            &[],
            true,
        ),
        def(
            "qwen-code",
            "Qwen Code",
            &["qwen-code", "qwen"],
            &[".qwen"],
            &[],
            &[],
            true,
        ),
        def(
            "kimi-cli",
            "Kimi CLI",
            &["kimi"],
            &[".kimi-code", ".kimi"],
            &[],
            &[],
            true,
        ),
        def(
            "cursor",
            "Cursor",
            &["cursor"],
            &[".cursor"],
            &[],
            &[],
            true,
        ),
        def("cline", "Cline", &["cline"], &[".cline"], &[], &[], true),
        def("roo-code", "Roo Code", &["roo"], &[".roo"], &[], &[], true),
        def(
            "aider",
            "Aider",
            &["aider"],
            &[".aider.conf.yml"],
            &[],
            &[],
            true,
        ),
        def("amp", "Amp", &["amp"], &[".config/amp"], &[], &[], true),
        def(
            "goose",
            "Goose",
            &["goose"],
            &[".config/goose"],
            &[],
            &[],
            true,
        ),
        def(
            "continue",
            "Continue",
            &["continue"],
            &[".continue"],
            &[],
            &[],
            true,
        ),
    ]
}

pub fn all_definitions() -> Vec<HarnessDefinition> {
    let mut all = tier1_definitions();
    all.extend(detection_only_definitions());
    all
}
