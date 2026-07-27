//! 受控假进程（仅测试）：fake-pi / fake-opencode。
//! 线协议以 docs/module-contracts.md 第 5 节为权威；行为脚本见第 7 节。
//! 本 crate 不进入任何生产路径，只被集成测试 spawn。

use serde_json::{json, Value};

pub const DEFAULT_PI_VERSION: &str = "1.4.0";
pub const OPENCODE_VERSION: &str = "0.4.2";
pub const OPENCODE_WRONG_VERSION: &str = "9.9.9";
pub const AGENT_FILE_NAME: &str = "hello_from_agent.txt";
pub const AGENT_FILE_CONTENT: &str = "hello from agent";
pub const HAPPY_SUMMARY: &str = "已在工作区写入 hello_from_agent.txt";

/// 脚本步骤：Emit 产出一条 TraceItem 同构事件；WriteAgentFile 在当前工作目录写真实文件，
/// 让集成测试能断言"真实文件变更"而非仅凭事件文本。
pub enum ScriptStep {
    Emit(Value),
    WriteAgentFile,
}

pub fn trace_item(kind: &str, text: &str, detail: Value) -> Value {
    json!({"kind": kind, "text": text, "detail": detail})
}

pub fn write_agent_file() -> std::io::Result<()> {
    std::fs::write(AGENT_FILE_NAME, AGENT_FILE_CONTENT)
}

/// happy 固定脚本：phase planning→editing→verifying、agent_note、真实写文件、
/// file_hint、verification passed。fake-pi 与 fake-opencode 共用，保证两侧证据一致。
pub fn happy_script() -> Vec<ScriptStep> {
    vec![
        ScriptStep::Emit(trace_item("phase", "规划中", json!({"phase": "planning"}))),
        ScriptStep::Emit(trace_item("phase", "编辑中", json!({"phase": "editing"}))),
        ScriptStep::Emit(trace_item(
            "agent_note",
            "准备在工作区写入 hello_from_agent.txt",
            json!({}),
        )),
        ScriptStep::WriteAgentFile,
        ScriptStep::Emit(trace_item(
            "file_hint",
            "hello_from_agent.txt",
            json!({"path": AGENT_FILE_NAME, "change": "added"}),
        )),
        ScriptStep::Emit(trace_item("phase", "验证中", json!({"phase": "verifying"}))),
        ScriptStep::Emit(trace_item(
            "verification",
            "验证通过",
            json!({"status": "passed", "detail": "自检通过"}),
        )),
    ]
}

/// verify_fail：与 happy 相同，但验证结论为 failed；任务本身仍正常结束（outcome=finished）。
pub fn verify_fail_script() -> Vec<ScriptStep> {
    let mut steps = happy_script();
    steps.pop();
    steps.push(ScriptStep::Emit(trace_item(
        "verification",
        "验证失败",
        json!({"status": "failed", "detail": "自检未通过"}),
    )));
    steps
}

/// action_request：中途发 kind=permission 的操作请求，随后继续完成整个 happy 脚本。
pub fn action_request_script() -> Vec<ScriptStep> {
    let mut steps = happy_script();
    steps.insert(
        2,
        ScriptStep::Emit(trace_item(
            "action_request",
            "等待权限确认",
            json!({"kind": "permission", "request_id": "req-1", "prompt": "允许写入 hello_from_agent.txt？"}),
        )),
    );
    steps
}
