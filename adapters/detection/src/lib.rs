//! Adapters for the additional harnesses outside the five first-class set.
//!
//! These tools do not share one configuration format, so this crate keeps a
//! small format-aware core and exposes one adapter type per harness.  The
//! adapters deliberately report only the native surfaces that are actually
//! represented on disk: a tool without a model registry is not presented as
//! if it can accept arbitrary model routes.

mod parser;
mod writer;

use chm_core::domain::harness::HarnessInstallation;
use chm_harness_sdk::adapter::plan::ReconciliationPlan;
use chm_harness_sdk::adapter::types::{
    AdapterError, ApplyResult, HarnessAdapter, HarnessCapabilities, NativePlan, ParsedState,
    ValidationReport,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigFormat {
    Json,
    Jsonc,
    Toml,
    Yaml,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DetectionSpec {
    pub id: &'static str,
    pub dot_dir: &'static str,
    pub config_rel: &'static str,
    pub alternate_config_rels: &'static [&'static str],
    pub format: ConfigFormat,
    pub mcp_rels: &'static [&'static str],
    pub skill_rels: &'static [&'static str],
    pub supports_models: bool,
    pub supports_providers: bool,
    pub supports_profiles: bool,
    pub supports_mcp: bool,
    pub supports_runtime_env: bool,
    pub allow_missing_primary: bool,
}

const NONE: &[&str] = &[];

// Roo Code and Cline are also VS Code extensions. Their global MCP files live
// in the editor's globalStorage directory rather than under the CLI dot-dir.
// Keep the known macOS/Linux paths here; the parser also checks APPDATA for
// the equivalent Windows location.
const ROO_GLOBAL_CONFIG_RELS: &[&str] = &[
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
];

const CLINE_GLOBAL_CONFIG_RELS: &[&str] = &[
    "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
    ".config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
    "Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
];

const GOOSE_WINDOWS_CONFIG_RELS: &[&str] = &[
    "APPDATA/Block/goose/config/config.yaml",
    "APPDATA/Block/goose/config/config.yml",
];

pub(crate) static KIMI: DetectionSpec = DetectionSpec {
    id: "kimi-cli",
    dot_dir: ".kimi",
    config_rel: ".kimi/config.toml",
    alternate_config_rels: &[
        ".kimi/config.json",
        ".kimi-code/config.toml",
        ".kimi-code/config.json",
    ],
    format: ConfigFormat::Toml,
    mcp_rels: &[".kimi/mcp.json", ".kimi-code/mcp.json"],
    skill_rels: &[".kimi/skills", ".kimi-code/skills"],
    supports_models: true,
    supports_providers: true,
    supports_profiles: true,
    supports_mcp: true,
    supports_runtime_env: true,
    allow_missing_primary: false,
};

pub(crate) static GEMINI: DetectionSpec = DetectionSpec {
    id: "gemini-cli",
    dot_dir: ".gemini",
    config_rel: ".gemini/settings.json",
    alternate_config_rels: &[".gemini/settings.json"],
    format: ConfigFormat::Json,
    mcp_rels: &[".gemini/mcp_config.json"],
    skill_rels: &[".gemini/skills"],
    supports_models: true,
    supports_providers: false,
    supports_profiles: true,
    supports_mcp: true,
    supports_runtime_env: true,
    allow_missing_primary: false,
};

pub(crate) static QWEN: DetectionSpec = DetectionSpec {
    id: "qwen-code",
    dot_dir: ".qwen",
    config_rel: ".qwen/settings.json",
    alternate_config_rels: &[".qwen/settings.json"],
    format: ConfigFormat::Json,
    mcp_rels: NONE,
    skill_rels: NONE,
    supports_models: false,
    supports_providers: false,
    supports_profiles: true,
    supports_mcp: true,
    supports_runtime_env: true,
    allow_missing_primary: false,
};

pub(crate) static CURSOR: DetectionSpec = DetectionSpec {
    id: "cursor",
    dot_dir: ".cursor",
    config_rel: ".cursor/cli-config.json",
    alternate_config_rels: &[".cursor/mcp.json"],
    format: ConfigFormat::Json,
    mcp_rels: &[".cursor/mcp.json"],
    skill_rels: NONE,
    supports_models: false,
    supports_providers: false,
    supports_profiles: true,
    supports_mcp: true,
    supports_runtime_env: true,
    allow_missing_primary: false,
};

pub(crate) static CLINE: DetectionSpec = DetectionSpec {
    id: "cline",
    dot_dir: ".cline",
    config_rel: ".cline/data/settings/providers.json",
    alternate_config_rels: &[
        ".cline/data/settings/models.json",
        ".cline/data/settings/global-settings.json",
        ".cline/mcp.json",
        CLINE_GLOBAL_CONFIG_RELS[0],
        CLINE_GLOBAL_CONFIG_RELS[1],
        CLINE_GLOBAL_CONFIG_RELS[2],
    ],
    format: ConfigFormat::Json,
    mcp_rels: &[
        ".cline/mcp.json",
        ".cline/data/settings/cline_mcp_settings.json",
        CLINE_GLOBAL_CONFIG_RELS[0],
        CLINE_GLOBAL_CONFIG_RELS[1],
        CLINE_GLOBAL_CONFIG_RELS[2],
    ],
    skill_rels: &[".cline/data/settings/skills", ".cline/skills"],
    supports_models: true,
    supports_providers: true,
    supports_profiles: true,
    supports_mcp: true,
    supports_runtime_env: true,
    allow_missing_primary: true,
};

pub(crate) static ROO: DetectionSpec = DetectionSpec {
    id: "roo-code",
    dot_dir: ".roo",
    config_rel: ".roo/mcp.json",
    alternate_config_rels: &[
        ".roo/settings.json",
        ROO_GLOBAL_CONFIG_RELS[0],
        ROO_GLOBAL_CONFIG_RELS[1],
        ROO_GLOBAL_CONFIG_RELS[2],
        ROO_GLOBAL_CONFIG_RELS[3],
        ROO_GLOBAL_CONFIG_RELS[4],
        ROO_GLOBAL_CONFIG_RELS[5],
        ROO_GLOBAL_CONFIG_RELS[6],
        ROO_GLOBAL_CONFIG_RELS[7],
        ROO_GLOBAL_CONFIG_RELS[8],
        ROO_GLOBAL_CONFIG_RELS[9],
        ROO_GLOBAL_CONFIG_RELS[10],
        ROO_GLOBAL_CONFIG_RELS[11],
    ],
    format: ConfigFormat::Json,
    mcp_rels: &[
        ".roo/mcp.json",
        ROO_GLOBAL_CONFIG_RELS[0],
        ROO_GLOBAL_CONFIG_RELS[1],
        ROO_GLOBAL_CONFIG_RELS[2],
        ROO_GLOBAL_CONFIG_RELS[3],
        ROO_GLOBAL_CONFIG_RELS[4],
        ROO_GLOBAL_CONFIG_RELS[5],
        ROO_GLOBAL_CONFIG_RELS[6],
        ROO_GLOBAL_CONFIG_RELS[7],
        ROO_GLOBAL_CONFIG_RELS[8],
        ROO_GLOBAL_CONFIG_RELS[9],
        ROO_GLOBAL_CONFIG_RELS[10],
        ROO_GLOBAL_CONFIG_RELS[11],
    ],
    skill_rels: &[".roo/skills"],
    supports_models: false,
    supports_providers: false,
    supports_profiles: true,
    supports_mcp: true,
    supports_runtime_env: true,
    allow_missing_primary: true,
};

pub(crate) static AIDER: DetectionSpec = DetectionSpec {
    id: "aider",
    dot_dir: ".aider",
    config_rel: ".aider.conf.yml",
    alternate_config_rels: NONE,
    format: ConfigFormat::Yaml,
    mcp_rels: NONE,
    skill_rels: NONE,
    supports_models: false,
    supports_providers: false,
    supports_profiles: true,
    supports_mcp: false,
    supports_runtime_env: true,
    allow_missing_primary: false,
};

pub(crate) static AMP: DetectionSpec = DetectionSpec {
    id: "amp",
    dot_dir: ".config/amp",
    config_rel: ".config/amp/settings.json",
    alternate_config_rels: &[".config/amp/settings.jsonc", ".amp/settings.json"],
    format: ConfigFormat::Json,
    mcp_rels: NONE,
    skill_rels: &[".config/amp/skills"],
    supports_models: false,
    supports_providers: false,
    supports_profiles: true,
    supports_mcp: true,
    supports_runtime_env: true,
    allow_missing_primary: false,
};

pub(crate) static GOOSE: DetectionSpec = DetectionSpec {
    id: "goose",
    dot_dir: ".config/goose",
    config_rel: ".config/goose/config.yaml",
    alternate_config_rels: &[
        ".config/goose/config.yml",
        GOOSE_WINDOWS_CONFIG_RELS[0],
        GOOSE_WINDOWS_CONFIG_RELS[1],
    ],
    format: ConfigFormat::Yaml,
    mcp_rels: NONE,
    skill_rels: &[".config/goose/skills"],
    supports_models: true,
    supports_providers: true,
    supports_profiles: true,
    supports_mcp: false,
    supports_runtime_env: true,
    allow_missing_primary: false,
};

pub(crate) static CONTINUE: DetectionSpec = DetectionSpec {
    id: "continue",
    dot_dir: ".continue",
    config_rel: ".continue/config.yaml",
    alternate_config_rels: &[".continue/config.yml", ".continue/config.json"],
    format: ConfigFormat::Yaml,
    mcp_rels: NONE,
    skill_rels: NONE,
    supports_models: true,
    supports_providers: true,
    supports_profiles: true,
    supports_mcp: true,
    supports_runtime_env: true,
    allow_missing_primary: false,
};

macro_rules! adapter_type {
    ($name:ident, $spec:ident) => {
        pub struct $name;

        impl HarnessAdapter for $name {
            fn id(&self) -> &'static str {
                $spec.id
            }

            fn capabilities(&self) -> HarnessCapabilities {
                HarnessCapabilities::none()
                    .with_models($spec.supports_models)
                    .with_providers($spec.supports_providers)
                    .with_profiles($spec.supports_profiles)
                    .with_mcp_global($spec.supports_mcp)
                    .with_global_skills(!$spec.skill_rels.is_empty())
                    .with_runtime_env($spec.supports_runtime_env)
            }

            fn read_state(
                &self,
                install: &HarnessInstallation,
            ) -> Result<ParsedState, AdapterError> {
                parser::read_state(&$spec, install)
            }

            fn plan(
                &self,
                plan: &ReconciliationPlan,
                install: &HarnessInstallation,
            ) -> Result<NativePlan, AdapterError> {
                writer::plan(&$spec, plan, install)
            }

            fn apply(
                &self,
                _install: &HarnessInstallation,
                native_plan: &NativePlan,
            ) -> Result<ApplyResult, AdapterError> {
                chm_harness_sdk::adapter::helpers::apply_native_plan(native_plan)
                    .map_err(AdapterError::Invalid)
            }

            fn validate(
                &self,
                install: &HarnessInstallation,
            ) -> Result<ValidationReport, AdapterError> {
                parser::validate(&$spec, install)
            }

            fn rollback(
                &self,
                _install: &HarnessInstallation,
                _native_plan: &NativePlan,
            ) -> Result<(), AdapterError> {
                Ok(())
            }
        }
    };
}

adapter_type!(KimiAdapter, KIMI);
adapter_type!(GeminiAdapter, GEMINI);
adapter_type!(QwenAdapter, QWEN);
adapter_type!(CursorAdapter, CURSOR);
adapter_type!(ClineAdapter, CLINE);
adapter_type!(RooAdapter, ROO);
adapter_type!(AiderAdapter, AIDER);
adapter_type!(AmpAdapter, AMP);
adapter_type!(GooseAdapter, GOOSE);
adapter_type!(ContinueAdapter, CONTINUE);

pub fn all_adapters() -> Vec<Box<dyn HarnessAdapter>> {
    vec![
        Box::new(KimiAdapter),
        Box::new(GeminiAdapter),
        Box::new(QwenAdapter),
        Box::new(CursorAdapter),
        Box::new(ClineAdapter),
        Box::new(RooAdapter),
        Box::new(AiderAdapter),
        Box::new(AmpAdapter),
        Box::new(GooseAdapter),
        Box::new(ContinueAdapter),
    ]
}
