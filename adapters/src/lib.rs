//! Adapter facade: compiles every supported harness adapter into one registry.

use chm_harness_sdk::adapter::types::HarnessAdapter;

pub mod claude_code {
    pub use claude_code_adapter::*;
}
pub mod codex {
    pub use codex_adapter::*;
}
pub mod opencode {
    pub use opencode_adapter::*;
}
pub mod pi {
    pub use pi_adapter::*;
}
pub mod reasonix {
    pub use reasonix_adapter::*;
}
pub mod detection {
    pub use detection_adapter::*;
}

pub fn all_adapters() -> Vec<Box<dyn HarnessAdapter>> {
    vec![
        Box::new(claude_code::ClaudeCodeAdapter),
        Box::new(codex::CodexAdapter),
        Box::new(opencode::OpenCodeAdapter),
        Box::new(pi::PiAdapter),
        Box::new(reasonix::ReasonixAdapter),
        Box::new(detection::KimiAdapter),
        Box::new(detection::GeminiAdapter),
        Box::new(detection::QwenAdapter),
        Box::new(detection::CursorAdapter),
        Box::new(detection::ClineAdapter),
        Box::new(detection::RooAdapter),
        Box::new(detection::AiderAdapter),
        Box::new(detection::AmpAdapter),
        Box::new(detection::GooseAdapter),
        Box::new(detection::ContinueAdapter),
    ]
}
