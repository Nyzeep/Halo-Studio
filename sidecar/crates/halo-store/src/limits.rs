/// 存储层大小上限（单位：字节）。
/// 默认值与 halo-core::limits 保持一致；两个 crate 相互零依赖，因此各自锁定同一组常量。
/// `version_total_max_bytes` 只约束单个证据版本（或单个交接包）内 Diff 正文的累计总量，
/// summary 由 `summary_max_bytes` 独立约束。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreLimits {
    pub summary_max_bytes: usize,
    pub file_diff_max_bytes: usize,
    pub version_total_max_bytes: usize,
    pub trace_text_max_bytes: usize,
}

impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            summary_max_bytes: 16 * 1024,
            file_diff_max_bytes: 256 * 1024,
            version_total_max_bytes: 4 * 1024 * 1024,
            trace_text_max_bytes: 4 * 1024,
        }
    }
}

/// 按字节上限截断文本并保证 UTF-8 字符边界；返回（截断后文本, 是否发生截断）。
pub(crate) fn cap_text(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_owned(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_text_within_limit_untouched() {
        let (out, truncated) = cap_text("hello", 8);
        assert_eq!(out, "hello");
        assert!(!truncated);
    }

    #[test]
    fn cap_text_respects_utf8_boundary() {
        // “你好世界” 每字 3 字节；上限 8 落在字符中间，必须回退到 6 字节边界
        let (out, truncated) = cap_text("你好世界", 8);
        assert_eq!(out, "你好");
        assert!(truncated);
    }
}
