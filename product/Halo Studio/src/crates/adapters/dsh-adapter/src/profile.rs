//! DSH version-profile anchoring (ADR-0078).
//!
//! Halo anchors the managed DSH channel to `0.1.3-alpha.1` exactly like the pi
//! compatibility profile: a successful process start is never evidence that the
//! wire schema is known. `SUPPORTED_DSH_PROFILES` is the only accepted
//! version/channel table, and the `initialize` result is validated against the
//! anchored wire facts before anything else happens. Any drift — unknown
//! version, wrong protocol version, unexpected agent identity — fails closed
//! with [`DshFailureKind::UnsupportedVersion`]; upstream minor upgrades require
//! a new profile entry plus a full contract-test pass (research document,
//! section 7; ADR-0078 "版本档案机制").

use serde_json::Value;

use crate::DshFailureKind;

/// ACP v1 wire protocol version (`@agentclientprotocol/sdk` PROTOCOL_VERSION).
pub(crate) const ACP_PROTOCOL_VERSION: i64 = 1;

/// The ACP profile's declared agent identity (research section 2.2).
pub(crate) const DSH_ACP_AGENT_NAME: &str = "deepseek-harness-acp";

/// The SDK profile's declared server identity (research section 2.1).
pub(crate) const DSH_SDK_SERVER_NAME: &str = "deepseek-harness-sdk-runtime";

/// Which managed DSH wire a session uses.
///
/// `Acp` is the production channel: it carries the one-shot
/// `session/request_permission` decision flow that is structurally isomorphic
/// to Halo's one-time decisions (ADR-0012/0078). `Sdk` has no wire-level
/// approval channel and no wire-level cancel, so it only ever runs as the
/// protocol canary / degraded channel (ADR-0078: 降级通道事件面映射到统一事实
/// 词汇，证据不断链). `session/resume` is not consumed on either channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DshChannelKind {
    Acp,
    Sdk,
}

impl DshChannelKind {
    /// The launcher profile argument (`dsh --profile <name>`).
    pub const fn profile_arg(self) -> &'static str {
        match self {
            Self::Acp => "acp",
            Self::Sdk => "sdk",
        }
    }

    /// The degraded canary channel never claims an approval wire.
    pub const fn is_degraded_canary(self) -> bool {
        matches!(self, Self::Sdk)
    }
}

/// One anchored Halo-side DSH compatibility profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DshProfile {
    pub version: &'static str,
    pub channel: DshChannelKind,
    /// Wire facts the `initialize` result must repeat exactly.
    pub protocol_version: i64,
    pub agent_name: &'static str,
}

/// The only accepted DSH profiles. Upstream is a developer preview with high
/// drift velocity; anything not listed here fails closed before a child is
/// ever spawned.
pub const SUPPORTED_DSH_PROFILES: &[DshProfile] = &[
    DshProfile {
        version: "0.1.3-alpha.1",
        channel: DshChannelKind::Acp,
        protocol_version: ACP_PROTOCOL_VERSION,
        agent_name: DSH_ACP_AGENT_NAME,
    },
    DshProfile {
        version: "0.1.3-alpha.1",
        channel: DshChannelKind::Sdk,
        // The SDK profile's private protocol carries no protocol-version
        // member; the ACP constant is only meaningful on the ACP channel.
        protocol_version: ACP_PROTOCOL_VERSION,
        agent_name: DSH_SDK_SERVER_NAME,
    },
];

/// Resolves the anchored profile for a channel + declared version, failing
/// closed when the combination is not exactly the reviewed anchor.
pub fn supported_profile(
    channel: DshChannelKind,
    version: &str,
) -> Result<&'static DshProfile, DshFailureKind> {
    SUPPORTED_DSH_PROFILES
        .iter()
        .find(|profile| profile.channel == channel && profile.version == version)
        .ok_or(DshFailureKind::UnsupportedVersion)
}

/// Validates the `initialize` result against the anchored profile.
///
/// On the ACP channel the answer must repeat the anchored protocol version and
/// the `deepseek-harness-acp` agent identity. On the SDK channel the private
/// protocol answers with `serverInfo.name`. Anything else is unsupported.
pub fn validate_initialize_result(
    result: &Value,
    profile: &DshProfile,
) -> Result<(), DshFailureKind> {
    match profile.channel {
        DshChannelKind::Acp => {
            if result.get("protocolVersion").and_then(Value::as_i64)
                != Some(profile.protocol_version)
            {
                return Err(DshFailureKind::UnsupportedVersion);
            }
            let agent_name = result
                .pointer("/agentInfo/name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if agent_name != profile.agent_name {
                return Err(DshFailureKind::UnsupportedVersion);
            }
        }
        DshChannelKind::Sdk => {
            let server_name = result
                .pointer("/serverInfo/name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if server_name != profile.agent_name {
                return Err(DshFailureKind::UnsupportedVersion);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        supported_profile, validate_initialize_result, ACP_PROTOCOL_VERSION, DSH_ACP_AGENT_NAME,
        DSH_SDK_SERVER_NAME, DshChannelKind, SUPPORTED_DSH_PROFILES,
    };
    use crate::DshFailureKind;
    use serde_json::json;

    #[test]
    fn anchor_table_pins_the_reviewed_version() {
        assert_eq!(SUPPORTED_DSH_PROFILES.len(), 2);
        for profile in SUPPORTED_DSH_PROFILES {
            assert_eq!(profile.version, "0.1.3-alpha.1");
        }
        let acp = supported_profile(DshChannelKind::Acp, "0.1.3-alpha.1")
            .expect("acp profile is anchored");
        assert_eq!(acp.protocol_version, ACP_PROTOCOL_VERSION);
        assert_eq!(acp.agent_name, DSH_ACP_AGENT_NAME);
    }

    #[test]
    fn unknown_versions_fail_closed() {
        assert_eq!(
            supported_profile(DshChannelKind::Acp, "0.2.0"),
            Err(DshFailureKind::UnsupportedVersion)
        );
        assert!(supported_profile(DshChannelKind::Sdk, "0.1.3-alpha.1").is_ok());
    }

    #[test]
    fn acp_initialize_validation_accepts_and_rejects() {
        let profile =
            supported_profile(DshChannelKind::Acp, "0.1.3-alpha.1").expect("anchored profile");
        let good = json!({
            "protocolVersion": 1,
            "agentInfo": { "name": DSH_ACP_AGENT_NAME, "version": "0.0.1" },
            "authMethods": []
        });
        assert_eq!(validate_initialize_result(&good, profile), Ok(()));

        let wrong_version = json!({
            "protocolVersion": 2,
            "agentInfo": { "name": DSH_ACP_AGENT_NAME }
        });
        assert_eq!(
            validate_initialize_result(&wrong_version, profile),
            Err(DshFailureKind::UnsupportedVersion)
        );

        let wrong_agent = json!({
            "protocolVersion": 1,
            "agentInfo": { "name": "future-harness-agent" }
        });
        assert_eq!(
            validate_initialize_result(&wrong_agent, profile),
            Err(DshFailureKind::UnsupportedVersion)
        );
    }

    #[test]
    fn sdk_initialize_validation_uses_server_info() {
        let profile =
            supported_profile(DshChannelKind::Sdk, "0.1.3-alpha.1").expect("anchored profile");
        let good = json!({ "serverInfo": { "name": DSH_SDK_SERVER_NAME, "version": "0.1.3-alpha.1" } });
        assert_eq!(validate_initialize_result(&good, profile), Ok(()));
        let bad = json!({ "serverInfo": { "name": "something-else" } });
        assert_eq!(
            validate_initialize_result(&bad, profile),
            Err(DshFailureKind::UnsupportedVersion)
        );
    }
}
