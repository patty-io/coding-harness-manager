//! Adapter facade: compiles all Tier-1 adapters into one registry.

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

pub fn all_adapters() -> Vec<Box<dyn HarnessAdapter>> {
    vec![
        Box::new(claude_code::ClaudeCodeAdapter),
        Box::new(codex::CodexAdapter),
        Box::new(opencode::OpenCodeAdapter),
        Box::new(pi::PiAdapter),
        Box::new(reasonix::ReasonixAdapter),
    ]
}
