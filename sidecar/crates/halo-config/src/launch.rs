use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::secret::Secret;

/// 受管应用取值；与 IPC 文档的 `"pi" | "opencode"` 同构。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    Pi,
    OpenCode,
}

impl AgentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentKind::Pi => "pi",
            AgentKind::OpenCode => "opencode",
        }
    }
}

impl FromStr for AgentKind {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pi" => Ok(AgentKind::Pi),
            "opencode" => Ok(AgentKind::OpenCode),
            other => Err(ConfigError::InvalidField {
                field: "agent".to_string(),
                reason: format!("不支持的取值：{other}（仅允许 pi / opencode）"),
            }),
        }
    }
}

/// 思考级别；与 IPC 文档的 `"off" | "low" | "medium" | "high"` 同构。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Low,
    Medium,
    High,
}

impl ThinkingLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThinkingLevel::Off => "off",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
        }
    }
}

impl FromStr for ThinkingLevel {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(ThinkingLevel::Off),
            "low" => Ok(ThinkingLevel::Low),
            "medium" => Ok(ThinkingLevel::Medium),
            "high" => Ok(ThinkingLevel::High),
            other => Err(ConfigError::InvalidField {
                field: "thinking_level".to_string(),
                reason: format!("不支持的取值：{other}（仅允许 off / low / medium / high）"),
            }),
        }
    }
}

/// 受管启动配置（config 自有类型，字段与 IPC 文档 LaunchConfigInput 同构，
/// 由 halo-sidecar 负责与协议 DTO 互转）。`credential_ref` 只存引用名，
/// 永不携带凭据明文。
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchConfig {
    pub id: String,
    pub name: String,
    pub agent: AgentKind,
    pub executable_path: String,
    pub model: String,
    pub thinking_level: ThinkingLevel,
    pub credential_ref: Option<String>,
    pub extra_args: Vec<String>,
    pub env_overrides: HashMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("启动配置字段无效：{field}：{reason}")]
    InvalidField { field: String, reason: String },
    #[error("环境变量不在白名单内：{name}")]
    EnvNotWhitelisted { name: String },
}

/// 子进程环境白名单（IPC 文档 3.2 节锁定）；宿主其余环境变量一律不继承。
pub const ENV_WHITELIST: &[&str] = &[
    "SYSTEMROOT",
    "WINDIR",
    "PATH",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "COMSPEC",
    "PATHEXT",
    "SystemDrive",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
];

/// Windows 环境变量名不区分大小写，命中后统一使用白名单中的规范拼写。
fn canonical_whitelist_name(name: &str) -> Option<&'static str> {
    ENV_WHITELIST
        .iter()
        .copied()
        .find(|w| w.eq_ignore_ascii_case(name))
}

pub fn validate_launch_config(cfg: &LaunchConfig) -> Result<(), ConfigError> {
    if cfg.executable_path.trim().is_empty() {
        return Err(ConfigError::InvalidField {
            field: "executable_path".to_string(),
            reason: "不能为空".to_string(),
        });
    }
    validate_env_override_names(&cfg.env_overrides)?;
    Ok(())
}

/// overrides 名称校验：违白名单 => EnvNotWhitelisted；
/// 大小写归一化后重复 => InvalidField（避免覆盖结果不确定）。
/// 按字典序遍历，保证同一输入产生确定的错误。
fn validate_env_override_names(overrides: &HashMap<String, String>) -> Result<(), ConfigError> {
    let mut names: Vec<&String> = overrides.keys().collect();
    names.sort();
    let mut seen: HashSet<&'static str> = HashSet::new();
    for name in names {
        match canonical_whitelist_name(name) {
            None => {
                return Err(ConfigError::EnvNotWhitelisted { name: name.clone() });
            }
            Some(canon) => {
                if !seen.insert(canon) {
                    return Err(ConfigError::InvalidField {
                        field: "env_overrides".to_string(),
                        reason: format!("变量 {canon} 在大小写归一化后重复出现"),
                    });
                }
            }
        }
    }
    Ok(())
}

/// 构造子进程环境：
/// 1. 宿主环境只透传白名单变量（键统一为规范拼写）；
/// 2. overrides 必须全部位于白名单内，覆盖宿主同名值；
/// 3. `injected` 为启动瞬间注入的凭据变量——返回 map 为 HashMap，
///    同名键在结果中只出现一次，注入值覆盖此前任何来源的同名值。
///
/// 返回值中含凭据明文，仅限启动注入点使用，不得日志化或持久化。
pub fn build_child_env(
    host: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
    injected: Vec<(String, Secret)>,
) -> Result<HashMap<String, String>, ConfigError> {
    validate_env_override_names(overrides)?;

    let mut env: HashMap<String, String> = HashMap::new();

    for canon in ENV_WHITELIST {
        if let Some(value) = host.get(*canon) {
            env.insert((*canon).to_string(), value.clone());
            continue;
        }
        // 精确拼写未命中时做大小写不敏感匹配；取字典序最小的键保证确定性。
        let mut matches: Vec<(&String, &String)> = host
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(canon))
            .collect();
        matches.sort_by(|a, b| a.0.cmp(b.0));
        if let Some((_, value)) = matches.first() {
            env.insert((*canon).to_string(), (*value).clone());
        }
    }

    for (name, value) in overrides {
        if let Some(canon) = canonical_whitelist_name(name) {
            env.insert(canon.to_string(), value.clone());
        }
    }

    for (name, secret) in injected {
        env.insert(name, secret.expose().to_string());
    }

    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> LaunchConfig {
        LaunchConfig {
            id: "cfg-00000000-0000-4000-8000-000000000001".to_string(),
            name: "Pi + GPT".to_string(),
            agent: AgentKind::Pi,
            executable_path: "C:\\tools\\pi\\pi.exe".to_string(),
            model: "gpt-5".to_string(),
            thinking_level: ThinkingLevel::Medium,
            credential_ref: Some("halo/pi/openai".to_string()),
            extra_args: vec![],
            env_overrides: HashMap::new(),
            created_at: "2026-07-26T08:00:00Z".to_string(),
            updated_at: "2026-07-26T08:00:00Z".to_string(),
        }
    }

    #[test]
    fn validate_accepts_wellformed_config() {
        let mut cfg = sample_config();
        cfg.env_overrides
            .insert("TEMP".to_string(), "D:\\tmp".to_string());
        assert!(validate_launch_config(&cfg).is_ok());
    }

    #[test]
    fn validate_rejects_empty_executable_path() {
        let mut cfg = sample_config();
        cfg.executable_path = "   ".to_string();
        let err = validate_launch_config(&cfg).unwrap_err();
        assert!(
            matches!(err, ConfigError::InvalidField { ref field, .. } if field == "executable_path")
        );
    }

    #[test]
    fn validate_rejects_env_override_outside_whitelist() {
        let mut cfg = sample_config();
        cfg.env_overrides
            .insert("LD_PRELOAD".to_string(), "evil.dll".to_string());
        let err = validate_launch_config(&cfg).unwrap_err();
        assert!(matches!(err, ConfigError::EnvNotWhitelisted { ref name } if name == "LD_PRELOAD"));
    }

    #[test]
    fn agent_kind_parses_contract_values_only() {
        assert_eq!("pi".parse::<AgentKind>().unwrap(), AgentKind::Pi);
        assert_eq!("opencode".parse::<AgentKind>().unwrap(), AgentKind::OpenCode);
        assert!("claude".parse::<AgentKind>().is_err());
        assert!("Pi".parse::<AgentKind>().is_err());
    }

    #[test]
    fn thinking_level_parses_contract_values_only() {
        assert_eq!("off".parse::<ThinkingLevel>().unwrap(), ThinkingLevel::Off);
        assert_eq!("high".parse::<ThinkingLevel>().unwrap(), ThinkingLevel::High);
        assert!("ultra".parse::<ThinkingLevel>().is_err());
    }

    #[test]
    fn build_child_env_passes_through_whitelist_only() {
        let mut host = HashMap::new();
        host.insert("PATH".to_string(), "C:\\Windows".to_string());
        host.insert("USERPROFILE".to_string(), "C:\\Users\\dev".to_string());
        host.insert("OPENAI_API_KEY".to_string(), "host-leak".to_string());
        host.insert("HTTP_PROXY".to_string(), "http://proxy".to_string());

        let env = build_child_env(&host, &HashMap::new(), vec![]).unwrap();
        assert_eq!(env.get("PATH").map(String::as_str), Some("C:\\Windows"));
        assert_eq!(
            env.get("USERPROFILE").map(String::as_str),
            Some("C:\\Users\\dev")
        );
        assert!(!env.contains_key("OPENAI_API_KEY"));
        assert!(!env.contains_key("HTTP_PROXY"));
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn build_child_env_normalizes_host_key_casing() {
        let mut host = HashMap::new();
        host.insert("Path".to_string(), "C:\\Windows".to_string());
        host.insert("SystemRoot".to_string(), "C:\\Windows".to_string());

        let env = build_child_env(&host, &HashMap::new(), vec![]).unwrap();
        assert_eq!(env.get("PATH").map(String::as_str), Some("C:\\Windows"));
        assert_eq!(
            env.get("SYSTEMROOT").map(String::as_str),
            Some("C:\\Windows")
        );
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn build_child_env_override_wins_over_host() {
        let mut host = HashMap::new();
        host.insert("TEMP".to_string(), "C:\\host-temp".to_string());
        let mut overrides = HashMap::new();
        overrides.insert("TEMP".to_string(), "D:\\task-temp".to_string());

        let env = build_child_env(&host, &overrides, vec![]).unwrap();
        assert_eq!(env.get("TEMP").map(String::as_str), Some("D:\\task-temp"));
    }

    #[test]
    fn build_child_env_rejects_override_outside_whitelist() {
        let mut overrides = HashMap::new();
        overrides.insert("EVIL_VAR".to_string(), "1".to_string());
        let err = build_child_env(&HashMap::new(), &overrides, vec![]).unwrap_err();
        assert!(matches!(err, ConfigError::EnvNotWhitelisted { ref name } if name == "EVIL_VAR"));
    }

    #[test]
    fn build_child_env_rejects_case_duplicated_overrides() {
        let mut overrides = HashMap::new();
        overrides.insert("PATH".to_string(), "a".to_string());
        overrides.insert("Path".to_string(), "b".to_string());
        let err = build_child_env(&HashMap::new(), &overrides, vec![]).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidField { ref field, .. } if field == "env_overrides"));
    }

    #[test]
    fn build_child_env_injected_credential_appears_exactly_once() {
        let mut host = HashMap::new();
        host.insert("PATH".to_string(), "C:\\Windows".to_string());
        // 宿主里已有同名（非白名单）变量：必须被丢弃，最终只保留注入值。
        host.insert("OPENAI_API_KEY".to_string(), "stale-host-value".to_string());

        let env = build_child_env(
            &host,
            &HashMap::new(),
            vec![("OPENAI_API_KEY".to_string(), Secret::new("sk-live-42"))],
        )
        .unwrap();

        let occurrences = env.keys().filter(|k| *k == "OPENAI_API_KEY").count();
        assert_eq!(occurrences, 1);
        assert_eq!(
            env.get("OPENAI_API_KEY").map(String::as_str),
            Some("sk-live-42")
        );
        assert_eq!(env.len(), 2); // PATH + 注入变量，宿主同名值未混入
    }

    #[test]
    fn build_child_env_duplicate_injection_keeps_single_entry_last_wins() {
        let env = build_child_env(
            &HashMap::new(),
            &HashMap::new(),
            vec![
                ("HALO_OC_TOKEN".to_string(), Secret::new("first")),
                ("HALO_OC_TOKEN".to_string(), Secret::new("second")),
            ],
        )
        .unwrap();
        assert_eq!(env.len(), 1);
        assert_eq!(env.get("HALO_OC_TOKEN").map(String::as_str), Some("second"));
    }
}
