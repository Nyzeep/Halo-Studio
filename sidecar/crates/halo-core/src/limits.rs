//! 交付历史与出口的大小上限（ipc-protocol.md 3.7）。
//! store 与 sidecar 出口双重使用（防御纵深）。

/// 摘要上限：16 KiB。
pub const SUMMARY_MAX: usize = 16 * 1024;
/// 单文件 diff 上限：256 KiB。
pub const FILE_DIFF_MAX: usize = 256 * 1024;
/// 单个证据版本总量上限：4 MiB。
pub const VERSION_TOTAL_MAX: usize = 4 * 1024 * 1024;
/// 单条运行轨迹文本上限：4 KiB。
pub const TRACE_TEXT_MAX: usize = 4 * 1024;
