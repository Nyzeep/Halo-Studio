use std::fmt;

/// Provider 凭据明文的唯一载体。
///
/// 约束（凭据红线）：
/// - 不实现 `Display`；
/// - `Debug` 恒定输出 `Secret(***)`，嵌套派生 Debug 时同样不泄露；
/// - 不实现任何 serde trait——本 crate 甚至不依赖 serde，且孤儿规则阻止
///   下游为外部类型补实现 `Serialize`，序列化路径在类型层面被截断。
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// 仅限受管应用启动注入点调用；返回值不得写入日志、错误 message、
    /// IPC 消息或任何持久化介质。
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // 尽力而为的内存加固：析构时清零内部字节，缩短明文在已释放内存中的驻留窗口。
        // 0x00 是合法 UTF-8，因此不破坏 String 的编码不变量。
        // 这是 best-effort：不使用 volatile 写/编译器屏障（不引入新依赖），
        // 不承诺对抗编译器把"死存储"优化掉，也覆盖不了 String 曾扩容遗留的旧缓冲。
        unsafe { self.0.as_mut_vec() }.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_is_fixed_and_never_contains_plaintext() {
        let secret = Secret::new("sk-super-plain-text-123");
        let rendered = format!("{:?}", secret);
        assert_eq!(rendered, "Secret(***)");
        assert!(!rendered.contains("sk-super-plain-text-123"));
    }

    #[test]
    fn debug_of_containing_struct_never_leaks_plaintext() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Holder {
            name: String,
            secret: Secret,
        }
        let holder = Holder {
            name: "cfg".to_string(),
            secret: Secret::new("AKIAIOSFODNN7EXAMPLE"),
        };
        let rendered = format!("{:?}", holder);
        assert!(rendered.contains("Secret(***)"));
        assert!(!rendered.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn expose_returns_original_value() {
        let secret = Secret::new("token-value");
        assert_eq!(secret.expose(), "token-value");
    }

    #[test]
    fn drop_zeroize_does_not_panic_and_copied_plaintext_is_independent() {
        // Drop 的清零本身无法在安全代码中直接观测（读已释放内存是 UB）；
        // 这里锁定可测部分：作用域结束触发 Drop 不 panic，且 expose 复制出的
        // 明文与原对象无内存共享——原对象清零后副本必须完好。
        let copied;
        {
            let secret = Secret::new("sk-drop-scope-secret-1234");
            copied = secret.expose().to_string();
        } // secret 在此 Drop 并清零
        assert_eq!(copied, "sk-drop-scope-secret-1234");

        // 空值与多字节（CJK/emoji）值同样安全清零，不破坏 UTF-8 不变量
        drop(Secret::new(""));
        drop(Secret::new("密钥值🚀"));
    }
}
