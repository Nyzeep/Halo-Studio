//! 脱敏与限长：store 入库前与 sidecar 出口双重使用。

use regex::Regex;
use std::sync::OnceLock;

const REDACTED: &str = "[REDACTED]";

struct Rule {
    pattern: Regex,
    replacement: &'static str,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        // 内置字面量正则，编译失败属启动期不变量破坏
        let rule = |pat: &str, replacement: &'static str| Rule {
            pattern: Regex::new(pat).expect("内置脱敏正则必须合法"),
            replacement,
        };
        vec![
            // PEM 私钥完整块（BEGIN 到 END 含正文），先于头部兜底规则
            (rule(
                r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
                REDACTED,
            )),
            // 被截断的 PEM：只剩头部时同样不得放行
            rule(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----", REDACTED),
            // Bearer 令牌（先于通用赋值规则；要求 8+ 个令牌字符，避免误伤普通句子）
            rule(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{8,}", "Bearer [REDACTED]"),
            // OpenAI/Anthropic 风格：sk-...（含 sk-proj-、sk-ant- 等）
            rule(r"\bsk-[A-Za-z0-9_-]{8,}", REDACTED),
            // AWS Access Key ID
            rule(r"\bAKIA[0-9A-Z]{16}\b", REDACTED),
            // GitHub token
            rule(r"\bghp_[A-Za-z0-9]{20,}\b", REDACTED),
            // Slack token
            rule(r"\bxox[baprs]-[A-Za-z0-9-]{10,}", REDACTED),
            // 键值赋值：password= / passwd: / api_key= / aws_secret_access_key= 等；
            // 允许 snake/kebab 前缀与引号包裹的值，替换后保留键名便于诊断
            rule(
                r#"(?i)\b((?:[A-Za-z0-9]+[_-])*(?:password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key))\b["']?\s*[=:]\s*("[^"\n]*"|'[^'\n]*'|[^\s"',;]+)"#,
                "${1}=[REDACTED]",
            ),
        ]
    })
}

/// 把常见密钥样式替换为 [REDACTED]。对已脱敏文本幂等；普通中文/英文叙述不受影响。
pub fn sanitize(text: &str) -> String {
    let mut out = text.to_string();
    for rule in rules() {
        if let std::borrow::Cow::Owned(replaced) = rule.pattern.replace_all(&out, rule.replacement)
        {
            out = replaced;
        }
    }
    out
}

/// UTF-8 安全截断：在不超过 max_bytes 的最大字符边界处截断，返回 (文本, 是否截断)。
pub fn cap(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- sanitize：全模式命中 ----------

    #[test]
    fn redacts_sk_style_keys() {
        let s = sanitize("密钥是 sk-proj-Abc123DEF456ghi789，请妥善保管。");
        assert!(!s.contains("sk-proj-Abc123DEF456ghi789"), "{s}");
        assert!(s.contains(REDACTED));
        assert!(s.contains("请妥善保管"));
    }

    #[test]
    fn redacts_aws_access_key_id() {
        let s = sanitize("使用 AKIAIOSFODNN7EXAMPLE 访问");
        assert!(!s.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(s.contains(REDACTED));
    }

    #[test]
    fn redacts_bearer_tokens_case_insensitive() {
        let s = sanitize("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig");
        assert!(!s.contains("eyJhbGciOiJIUzI1NiJ9"));
        assert!(s.contains("Bearer [REDACTED]"));

        let s2 = sanitize("authorization: bearer abcd1234efgh5678");
        assert!(!s2.contains("abcd1234efgh5678"));
        assert!(s2.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn redacts_password_assignments() {
        for input in [
            "password=hunter2secret",
            "password = hunter2secret",
            "PASSWORD:hunter2secret",
            "db_password=hunter2secret",
            r#"{"password": "hunter2secret"}"#,
            "passwd='hunter2secret'",
        ] {
            let s = sanitize(input);
            assert!(!s.contains("hunter2secret"), "输入 {input:?} 泄漏：{s}");
            assert!(s.contains(REDACTED), "输入 {input:?} 未命中：{s}");
        }
    }

    #[test]
    fn redacts_secret_token_and_key_assignments() {
        for input in [
            "api_key=abc123def456",
            "apikey: abc123def456",
            "aws_secret_access_key = abc123def456",
            "client-secret=abc123def456",
            "token: abc123def456",
        ] {
            let s = sanitize(input);
            assert!(!s.contains("abc123def456"), "输入 {input:?} 泄漏：{s}");
        }
    }

    #[test]
    fn redacts_pem_private_key_block_including_body() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA7bq\nqqqBODY\n-----END RSA PRIVATE KEY-----";
        let s = sanitize(&format!("前文\n{pem}\n后文"));
        assert!(!s.contains("MIIEowIBAAKCAQEA7bq"));
        assert!(!s.contains("BEGIN RSA PRIVATE KEY"));
        assert!(s.contains("前文"));
        assert!(s.contains("后文"));
        assert!(s.contains(REDACTED));
    }

    #[test]
    fn redacts_truncated_pem_header() {
        let s = sanitize("日志被截断：-----BEGIN PRIVATE KEY-----\nMIIEvQ");
        assert!(!s.contains("BEGIN PRIVATE KEY"));
        assert!(s.contains(REDACTED));
    }

    #[test]
    fn redacts_github_and_slack_tokens() {
        let s = sanitize("ghp_ABCDEFGHIJKLMNOPQRSTuvwxyz012345 和 xoxb-123456789012-abcdefgh");
        assert!(!s.contains("ghp_ABCDEFGHIJKLMNOPQRST"));
        assert!(!s.contains("xoxb-123456789012"));
    }

    #[test]
    fn redacts_multiple_secrets_in_one_text() {
        let input = "sk-abcdefgh12345678 与 AKIAIOSFODNN7EXAMPLE 与 password=p@ss 与 Bearer tok12345678";
        let s = sanitize(input);
        assert!(!s.contains("sk-abcdefgh12345678"));
        assert!(!s.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!s.contains("p@ss"));
        assert!(!s.contains("tok12345678"));
        assert_eq!(s.matches(REDACTED).count(), 4, "{s}");
    }

    // ---------- sanitize：不误伤普通文本 ----------

    #[test]
    fn leaves_plain_text_untouched() {
        for input in [
            "这是一段普通中文说明，包含 password 单词但没有赋值。",
            "任务 task-1234567890 已进入 review_ready 状态。",
            "the bearer of good news arrived",
            "keyboard shortcuts: press Ctrl+K",
            "变更文件 src/auth.rs，共 42 行；seq=42，v=1。",
            "参见 https://example.com/docs?page=2 第 3 节。",
            "tokenizer=BPE 是模型配置而不是密钥。",
        ] {
            assert_eq!(sanitize(input), input, "普通文本被误伤：{input:?}");
        }
    }

    #[test]
    fn sanitize_is_idempotent() {
        let input = "password=hunter2 且 sk-abcdefgh12345678";
        let once = sanitize(input);
        let twice = sanitize(&once);
        assert_eq!(once, twice);
    }

    // ---------- cap：UTF-8 安全截断 ----------

    #[test]
    fn cap_under_and_at_limit_unchanged() {
        assert_eq!(cap("hello", 10), ("hello".to_string(), false));
        assert_eq!(cap("hello", 5), ("hello".to_string(), false));
        assert_eq!(cap("", 0), (String::new(), false));
    }

    #[test]
    fn cap_truncates_ascii_with_flag() {
        assert_eq!(cap("hello world", 5), ("hello".to_string(), true));
    }

    #[test]
    fn cap_respects_cjk_char_boundaries() {
        // 每个汉字 3 字节
        let s = "你好世界";
        assert_eq!(cap(s, 4), ("你".to_string(), true));
        assert_eq!(cap(s, 6), ("你好".to_string(), true));
        assert_eq!(cap(s, 12), ("你好世界".to_string(), false));
    }

    #[test]
    fn cap_respects_emoji_boundaries() {
        // 每个 emoji 4 字节
        let s = "😀😀";
        assert_eq!(cap(s, 3), (String::new(), true));
        assert_eq!(cap(s, 4), ("😀".to_string(), true));
        assert_eq!(cap(s, 7), ("😀".to_string(), true));
        assert_eq!(cap(s, 8), ("😀😀".to_string(), false));
    }

    #[test]
    fn cap_result_is_prefix_and_within_limit() {
        let s = "混合 mixed 文本 with 🚀 emoji 和中文";
        for max in 0..=s.len() + 2 {
            let (out, truncated) = cap(s, max);
            assert!(out.len() <= max || !truncated);
            assert!(s.starts_with(&out));
            assert_eq!(truncated, out.len() < s.len());
        }
    }
}
