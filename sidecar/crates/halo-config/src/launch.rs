use std::collections::HashMap;
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
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
        }
    }
}

impl FromStr for AgentKind {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pi" => Ok(Self::Pi),
            "opencode" => Ok(Self::OpenCode),
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
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl FromStr for ThinkingLevel {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "off" => Ok(Self::Off),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(ConfigError::InvalidField {
                field: "thinking_level".to_string(),
                reason: format!("不支持的取值：{other}（仅允许 off / low / medium / high）"),
            }),
        }
    }
}

/// 受管启动配置。凭据只以引用名存在；不接受任意启动参数或环境覆盖。
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchConfig {
    pub id: String,
    pub name: String,
    pub agent: AgentKind,
    pub executable_path: String,
    pub model: String,
    pub thinking_level: ThinkingLevel,
    pub credential_ref: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("启动配置字段无效：{field}：{reason}")]
    InvalidField { field: String, reason: String },
}

/// 子进程环境白名单；宿主其余环境变量一律不继承。
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

/// Pi 仍经由既有受管运行时适配器连接；其凭据变量不会由启动配置档开放配置。
pub const PI_CREDENTIAL_ENV_VAR: &str = "HALO_PROVIDER_API_KEY";

/// 解析受管子进程唯一可接收的凭据变量。
///
/// OpenCode 使用自身识别的 Provider 专用变量。模型标识中的 Provider 前缀是明确持久化的
/// 模型选择，而不是任意环境变量输入。未知 Provider 必须失败关闭，避免拼写错误静默产生未认证会话。
pub fn credential_env_var_for(agent: AgentKind, model: &str) -> Result<&'static str, ConfigError> {
    match agent {
        AgentKind::Pi => Ok(PI_CREDENTIAL_ENV_VAR),
        AgentKind::OpenCode => opencode_credential_env_var(model),
    }
}

fn opencode_credential_env_var(model: &str) -> Result<&'static str, ConfigError> {
    let Some((provider, model_name)) = model.trim().split_once('/') else {
        return Err(invalid_opencode_model());
    };
    if provider.is_empty() || model_name.trim().is_empty() {
        return Err(invalid_opencode_model());
    }

    match provider {
        "openai" => Ok("OPENAI_API_KEY"),
        "anthropic" => Ok("ANTHROPIC_API_KEY"),
        "deepseek" => Ok("DEEPSEEK_API_KEY"),
        "groq" => Ok("GROQ_API_KEY"),
        "mistral" => Ok("MISTRAL_API_KEY"),
        "openrouter" => Ok("OPENROUTER_API_KEY"),
        "xai" => Ok("XAI_API_KEY"),
        _ => Err(invalid_opencode_model()),
    }
}

fn invalid_opencode_model() -> ConfigError {
    ConfigError::InvalidField {
        field: "model".to_string(),
        reason: "OpenCode 模型必须使用受支持的 provider/model 形式".to_string(),
    }
}

pub fn validate_launch_config(config: &LaunchConfig) -> Result<(), ConfigError> {
    validate_required("executable_path", &config.executable_path)?;
    validate_required("model", &config.model)?;
    if let Some(credential_ref) = &config.credential_ref {
        validate_required("credential_ref", credential_ref)?;
    }
    if config.agent == AgentKind::OpenCode {
        opencode_credential_env_var(&config.model)?;
    }
    Ok(())
}

fn validate_required(field: &str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError::InvalidField {
            field: field.to_string(),
            reason: "不能为空".to_string(),
        });
    }
    Ok(())
}

/// 构造子进程环境：
/// 1. 宿主环境只透传固定白名单变量（键统一为规范拼写）；
/// 2. `injected` 仅在启动瞬间加入凭据变量；
/// 3. 任何配置层的任意环境覆盖均不存在。
///
/// 返回值中含凭据明文，仅限启动注入点使用，不得日志化或持久化。
pub fn build_child_env(
    host: &HashMap<String, String>,
    injected: Vec<(String, Secret)>,
) -> HashMap<String, String> {
    let mut env = HashMap::new();

    for canonical_name in ENV_WHITELIST {
        if let Some(value) = host.get(*canonical_name) {
            env.insert((*canonical_name).to_string(), value.clone());
            continue;
        }
        let mut matches: Vec<(&String, &String)> = host
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(canonical_name))
            .collect();
        matches.sort_by(|left, right| left.0.cmp(right.0));
        if let Some((_, value)) = matches.first() {
            env.insert((*canonical_name).to_string(), (*value).clone());
        }
    }

    for (name, secret) in injected {
        env.insert(name, secret.expose().to_string());
    }

    env
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
            created_at: "2026-07-26T08:00:00Z".to_string(),
            updated_at: "2026-07-26T08:00:00Z".to_string(),
        }
    }

    #[test]
    fn validate_accepts_reference_only_launch_config() {
        assert!(validate_launch_config(&sample_config()).is_ok());
    }

    #[test]
    fn validate_rejects_empty_required_values() {
        for field in ["executable_path", "model", "credential_ref"] {
            let mut config = sample_config();
            match field {
                "executable_path" => config.executable_path = "   ".to_string(),
                "model" => config.model = "   ".to_string(),
                "credential_ref" => config.credential_ref = Some("   ".to_string()),
                _ => unreachable!(),
            }
            let error = validate_launch_config(&config).expect_err("空字段必须拒绝");
            assert!(matches!(error, ConfigError::InvalidField { field: ref actual, .. } if actual == field));
        }
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
    fn build_child_env_passes_only_whitelist_and_injected_credentials() {
        let mut host = HashMap::new();
        host.insert("PATH".to_string(), "C:\\Windows".to_string());
        host.insert("USERPROFILE".to_string(), "C:\\Users\\dev".to_string());
        host.insert("OPENAI_API_KEY".to_string(), "host-leak".to_string());

        let env = build_child_env(
            &host,
            vec![("OPENAI_API_KEY".to_string(), Secret::new("sk-live-42"))],
        );
        assert_eq!(env.get("PATH").map(String::as_str), Some("C:\\Windows"));
        assert_eq!(
            env.get("USERPROFILE").map(String::as_str),
            Some("C:\\Users\\dev")
        );
        assert_eq!(
            env.get("OPENAI_API_KEY").map(String::as_str),
            Some("sk-live-42")
        );
        assert_eq!(env.len(), 3);
    }

    #[test]
    fn build_child_env_normalizes_host_key_casing() {
        let mut host = HashMap::new();
        host.insert("Path".to_string(), "C:\\Windows".to_string());
        host.insert("SystemRoot".to_string(), "C:\\Windows".to_string());

        let env = build_child_env(&host, vec![]);
        assert_eq!(env.get("PATH").map(String::as_str), Some("C:\\Windows"));
        assert_eq!(
            env.get("SYSTEMROOT").map(String::as_str),
            Some("C:\\Windows")
        );
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn opencode_provider_prefix_selects_a_fixed_credential_variable() {
        for (model, env_var) in [
            ("openai/gpt-5", "OPENAI_API_KEY"),
            ("anthropic/claude-sonnet-4", "ANTHROPIC_API_KEY"),
            ("openrouter/auto", "OPENROUTER_API_KEY"),
        ] {
            assert_eq!(
                credential_env_var_for(AgentKind::OpenCode, model).unwrap(),
                env_var
            );
        }
        assert_eq!(
            credential_env_var_for(AgentKind::Pi, "gpt-5").unwrap(),
            PI_CREDENTIAL_ENV_VAR
        );
    }

    #[test]
    fn opencode_unknown_or_ambiguous_provider_fails_closed() {
        for model in ["gpt-5", "unknown/gpt-5", "openai/"] {
            let error = credential_env_var_for(AgentKind::OpenCode, model)
                .expect_err("OpenCode 凭据变量必须来自受控 provider 映射");
            assert!(matches!(error, ConfigError::InvalidField { field, .. } if field == "model"));
        }
    }

    #[test]
    fn validate_rejects_opencode_configs_without_a_known_provider() {
        let mut config = sample_config();
        config.agent = AgentKind::OpenCode;
        config.model = "gpt-5".to_string();

        let error = validate_launch_config(&config)
            .expect_err("OpenCode 配置必须在保存前选择受支持的 provider/model");
        assert!(matches!(error, ConfigError::InvalidField { field, .. } if field == "model"));
    }
}
