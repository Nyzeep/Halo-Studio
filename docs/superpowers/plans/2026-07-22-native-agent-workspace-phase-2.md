# Halo Studio Native Agent Workspace Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立可测试的 Rust Runtime 事件总线、Run Snapshot、ring buffer 和 stdio JSONL IPC sidecar，让 Phase 1 的原生桌面壳可以开始对接真实 runtime 边界。

**Architecture:** Phase 2 仍不接真实 CLI，不启动 PTY，不写配置文件。Rust 侧新增 `halo-ipc` crate 与 `halo-runtime` 二进制，负责接收 JSONL command、驱动 fake runtime、输出 RuntimeEvent JSONL；Python 侧新增 `IpcClient`，负责以 `.venv` 中的 Python 后端启动 sidecar、发送命令、读取事件。UI 仍使用 demo 数据，但 controller 可切换到 IPC client 快照。

**Tech Stack:** Rust workspace、std-only JSONL encoder/decoder、Python 3.13 `.venv`、Python `unittest`、Rust `cargo test`。本阶段避免新增 Rust 外部依赖，继续保持网络无关测试。

---

## 文件结构

- Modify: `Cargo.toml`
  加入 `crates/halo-ipc` 和 `crates/halo-runtime`。
- Modify: `crates/halo-protocol/src/lib.rs`
  扩展 RuntimeEvent、RunState、RunSnapshot、RuntimeCommand。
- Modify: `crates/halo-core/src/lib.rs`
  导出 event_bus，并重导出协议层 runtime 类型。
- Create: `crates/halo-core/src/event_bus.rs`
  实现 per-run event append、subscribe snapshot、seq 校验。
- Note: `RunSnapshot` 最终保留在 `halo-protocol`，便于 `halo-core` 与 `halo-ipc` 复用同一个协议 DTO。
- Create: `crates/halo-core/tests/event_bus.rs`
  覆盖事件顺序、ring buffer 截断、snapshot 恢复。
- Create: `crates/halo-ipc/Cargo.toml`
  IPC crate 配置。
- Create: `crates/halo-ipc/src/lib.rs`
  实现无外部依赖 JSONL command/event 编解码。
- Create: `crates/halo-ipc/tests/jsonl.rs`
  覆盖 command parse、event encode、错误输入。
- Create: `crates/halo-runtime/Cargo.toml`
  runtime sidecar 二进制 crate。
- Create: `crates/halo-runtime/src/main.rs`
  stdio JSONL loop，支持 `createRun`、`getSnapshot`、`shutdown`。
- Create: `crates/halo-runtime/tests/sidecar.rs`
  启动二进制并验证 JSONL 输入输出。
- Create: `apps/desktop/halo_desktop/ipc_client.py`
  Python sidecar client。
- Create: `apps/desktop/tests/test_ipc_client.py`
  Python client 单元测试，使用 fake process。
- Modify: `apps/desktop/halo_desktop/app_controller.py`
  增加 runtime mode seam，不默认启动 sidecar。
- Modify: `README.md`
  增加 Phase 2 runtime/IPC 开发命令。

---

### Task 1: Protocol And Snapshot Model

**Files:**
- Modify: `crates/halo-protocol/src/lib.rs`
- Create: `crates/halo-core/src/snapshot.rs`
- Modify: `crates/halo-core/src/lib.rs`

- [x] **Step 1: Write failing tests**

Add tests in `crates/halo-core/tests/event_bus.rs`:

```rust
#[test]
fn snapshot_keeps_latest_ring_buffer_events() {
    let mut snapshot = RunSnapshot::new("run-1", "codex-cli", 3);
    for seq in 1..=5 {
        snapshot.push_event(RuntimeEvent::new("run-1", "codex-cli", seq, "message.delta", format!("event-{seq}")));
    }
    let seqs: Vec<u64> = snapshot.events().iter().map(|event| event.seq).collect();
    assert_eq!(seqs, vec![3, 4, 5]);
}
```

- [x] **Step 2: Verify red**

Run: `cargo test -p halo-core snapshot`

Expected: FAIL because `RunSnapshot` does not exist.

- [x] **Step 3: Implement**

Implement:
- `RunState`
- `RuntimeCommand`
- `RunSnapshot::new(run_id, agent_id, event_capacity)`
- `push_event`
- `events`
- `last_seq`
- `state`

- [x] **Step 4: Verify green**

Run: `cargo test -p halo-core snapshot`

Expected: PASS.

---

### Task 2: Event Bus

**Files:**
- Create: `crates/halo-core/src/event_bus.rs`
- Modify: `crates/halo-core/src/lib.rs`
- Modify: `crates/halo-core/tests/event_bus.rs`

- [x] **Step 1: Write failing tests**

```rust
#[test]
fn event_bus_rejects_out_of_order_events_for_run() {
    let mut bus = EventBus::new(8);
    bus.append(RuntimeEvent::new("run-1", "codex-cli", 1, "run.state", "running")).unwrap();
    let error = bus.append(RuntimeEvent::new("run-1", "codex-cli", 3, "message.delta", "gap")).unwrap_err();
    assert_eq!(error.to_string(), "expected seq 2 for run run-1, got 3");
}
```

- [x] **Step 2: Verify red**

Run: `cargo test -p halo-core event_bus`

Expected: FAIL because `EventBus` does not exist.

- [x] **Step 3: Implement**

Implement:
- `EventBus::new(event_capacity)`
- `append`
- `snapshot(run_id)`
- `snapshots`
- ordered seq validation

- [x] **Step 4: Verify green**

Run: `cargo test -p halo-core event_bus`

Expected: PASS.

---

### Task 3: JSONL IPC Codec

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/halo-ipc/Cargo.toml`
- Create: `crates/halo-ipc/src/lib.rs`
- Create: `crates/halo-ipc/tests/jsonl.rs`

- [x] **Step 1: Write failing tests**

```rust
#[test]
fn parses_create_run_command() {
    let command = decode_command(r#"{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"hello"}"#).unwrap();
    assert_eq!(command.run_id(), Some("run-1"));
}
```

- [x] **Step 2: Verify red**

Run: `cargo test -p halo-ipc`

Expected: FAIL because crate does not exist.

- [x] **Step 3: Implement**

Implement std-only codec:
- `decode_command(line)`
- `encode_event(event)`
- `encode_snapshot(snapshot)`
- reject unknown command type

- [x] **Step 4: Verify green**

Run: `cargo test -p halo-ipc`

Expected: PASS.

---

### Task 4: Runtime Sidecar

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/halo-runtime/Cargo.toml`
- Create: `crates/halo-runtime/src/main.rs`
- Create: `crates/halo-runtime/tests/sidecar.rs`

- [x] **Step 1: Write failing sidecar test**

Test starts `halo-runtime`, writes:

```json
{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"hello"}
{"type":"shutdown"}
```

Expected stdout contains ordered JSONL events for `run-1`.

- [x] **Step 2: Verify red**

Run: `cargo test -p halo-runtime`

Expected: FAIL because binary does not exist.

- [x] **Step 3: Implement**

Implement:
- stdin JSONL loop
- `createRun` emits fake runtime events through EventBus
- `getSnapshot` emits snapshot JSON
- `shutdown` exits 0

- [x] **Step 4: Verify green**

Run: `cargo test -p halo-runtime`

Expected: PASS.

---

### Task 5: Python IPC Client

**Files:**
- Create: `apps/desktop/halo_desktop/ipc_client.py`
- Create: `apps/desktop/tests/test_ipc_client.py`
- Modify: `apps/desktop/halo_desktop/app_controller.py`

- [x] **Step 1: Write failing Python tests**

```python
def test_ipc_client_serializes_create_run_command():
    process = FakeProcess()
    client = IpcClient(process)
    client.create_run("run-1", "codex-cli", "hello")
    assert process.stdin_lines[0] == '{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"hello"}'
```

- [x] **Step 2: Verify red**

Run: `..\..\.venv\Scripts\python.exe -m unittest apps.desktop.tests.test_ipc_client -v`

Expected: FAIL because client does not exist.

- [x] **Step 3: Implement**

Implement:
- `IpcClient`
- `create_run`
- `get_snapshot`
- `shutdown`
- `read_events_until`
- app controller seam `runtime_mode="demo" | "ipc"`

- [x] **Step 4: Verify green**

Run: `..\..\.venv\Scripts\python.exe -m unittest discover -s apps/desktop/tests -v`

Expected: PASS.

---

## Phase 2 验收标准

- `cargo test --workspace` 通过。
- `..\..\.venv\Scripts\python.exe -m unittest discover -s apps/desktop/tests -v` 通过。
- Rust `EventBus` 能按 run 保存 ordered event，并能生成 snapshot。
- Ring buffer 能限制每个 run 的内存增长。
- `halo-runtime` sidecar 能通过 stdio JSONL 接收 `createRun/getSnapshot/shutdown`。
- Python `IpcClient` 可测试，不在 import 时启动真实 sidecar。
- UI 仍默认使用 demo runtime，不因 Phase 2 runtime 未启动而阻塞。
