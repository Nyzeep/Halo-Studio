//! Compatibility re-export for skill DTOs and parsing.
//!
//! The provider-neutral owner lives in `halo-agent-runtime`.

pub use halo_agent_runtime::skills::{
    render_loaded_skill_for_assistant, ModeSkillInfo, ModeSkillStateReason, SkillData, SkillInfo,
    SkillLocation, SkillParseError,
};
