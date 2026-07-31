//! JSON-RPC version definitions
use serde::{Deserialize, Serialize};

/// Represents the different JSON-RPC versions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Version {
    /// JSON-RPC 1.0
    #[serde(rename = "1.0")]
    V1_0,
    /// JSON-RPC 1.1
    #[serde(rename = "1.1")]
    V1_1,
    /// JSON-RPC 2.0
    #[serde(rename = "2.0")]
    V2_0,
}

impl Version {
    /// Create a Version from a string
    pub fn from_str(s: &str) -> Option<Version> {
        match s {
            "1.0" => Some(Version::V1_0),
            "1.1" => Some(Version::V1_1),
            "2.0" => Some(Version::V2_0),
            _ => None,
        }
    }

    /// Get the string representation of the version
    pub fn as_str(&self) -> &'static str {
        match self {
            Version::V1_0 => "1.0",
            Version::V1_1 => "1.1",
            Version::V2_0 => "2.0",
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}