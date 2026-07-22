# Halo Studio Native Agent Workspace Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 寤虹珛涓€涓彲杩愯銆佸彲娴嬭瘯鐨勫師鐢熸闈?Phase 1 绾靛垏鐗囷紝璁?Halo Studio 浠?Web/Electron 缁堢澹宠浆鍚?Agent 宸ヤ綔娴佹闈㈠３銆?
**Architecture:** Phase 1 閲囩敤 `PySide6/QML` 浣滀负鍘熺敓妗岄潰 UI 澹筹紝`Rust` 浣滀负鏈潵楂樺苟鍙?runtime 鐨勬牳蹇冨熀纭€銆傜湡瀹?CLI銆侀厤缃啓鍏ャ€丮CP 鍐欏叆鏆備笉鎺ュ叆锛屽厛鐢?fake runtime 鍜屽唴缃?manifest 楠岃瘉 Agent 宸ヤ綔娴併€佸懡浠よˉ鍏ㄣ€佸苟鍙戜簨浠跺拰杞婚噺 UI 甯冨眬銆?
**Tech Stack:** Rust workspace銆丳ython 3.13銆丳ySide6/QML銆丳ython `unittest`銆丷ust `cargo test`锛屼笉鏂板 Electron/React/Web UI 璺嚎銆?
---

## 鏂囦欢缁撴瀯

- Create: `Cargo.toml`
  瀹氫箟 Rust workspace锛屽寘鍚?`crates/halo-protocol` 涓?`crates/halo-core`銆?- Create: `crates/halo-protocol/Cargo.toml`
  Rust protocol crate 閰嶇疆銆?- Create: `crates/halo-protocol/src/lib.rs`
  瀹氫箟 Agent銆丷un銆丷untimeEvent銆乄orkflowKind銆丼lashCommand 绛夌ǔ瀹氬崗璁被鍨嬨€?- Create: `crates/halo-core/Cargo.toml`
  Rust core crate 閰嶇疆銆?- Create: `crates/halo-core/src/lib.rs`
  瀵煎嚭 runtime銆乻cheduler銆乧ompletion 妯″潡銆?- Create: `crates/halo-core/src/completion.rs`
  瀹炵幇 slash 鍛戒护琛ュ叏鎺掑簭銆?- Create: `crates/halo-core/src/runtime.rs`
  瀹炵幇 fake runtime锛屾敮鎸?4/16/32 agent 骞跺彂浜嬩欢妯℃嫙銆?- Create: `crates/halo-core/src/scheduler.rs`
  瀹炵幇杞婚噺璋冨害绛栫暐鍜屽苟鍙戦檺鍒躲€?- Create: `apps/desktop/pyproject.toml`
  瀹氫箟鍘熺敓妗岄潰 Python 鍖呫€?- Create: `apps/desktop/requirements.txt`
  璁板綍 PySide6 杩愯渚濊禆銆?- Create: `apps/desktop/halo_desktop/__init__.py`
  Python 鍖呭叆鍙ｃ€?- Create: `apps/desktop/halo_desktop/main.py`
  PySide6/QML 妗岄潰鍚姩鍏ュ彛銆?- Create: `apps/desktop/halo_desktop/app_controller.py`
  UI 鎺у埗鍣ㄤ笌 demo 鏁版嵁妗ユ帴銆?- Create: `apps/desktop/halo_desktop/completion.py`
  Python 渚у懡浠よˉ鍏紝渚?QML composer 浣跨敤銆?- Create: `apps/desktop/halo_desktop/demo_runtime.py`
  Python fake runtime锛屼緵 UI 鍒濈増婕旂ず涓庡苟鍙戞祴璇曘€?- Create: `apps/desktop/halo_desktop/plugin_registry.py`
  璇诲彇鍐呯疆 Agent manifest銆?- Create: `apps/desktop/halo_desktop/models.py`
  Python dataclass 妯″瀷銆?- Create: `apps/desktop/halo_desktop/qml/Main.qml`
  鍘熺敓妗岄潰涓荤獥鍙ｃ€?- Create: `apps/desktop/halo_desktop/qml/components/*.qml`
  涓夋爮甯冨眬銆佹秷鎭祦銆両nspector銆佸懡浠よ緭鍏ョ粍浠躲€?- Create: `apps/desktop/halo_desktop/qml/styles/Theme.qml`
  杞婚噺鏆楄壊鐜荤拑瑙嗚 token銆?- Create: `apps/desktop/tests/*.py`
  Python 鍗曞厓娴嬭瘯銆佸苟鍙戞祴璇曘€丵ML 闈欐€佹€ц兘绾︽潫娴嬭瘯銆?- Create: `plugins/agents/*/agent.toml`
  Claude Code銆丆odex CLI銆丱penCode銆丳i 鐨勫唴缃?Agent manifest銆?- Modify: `README.md`
  鏇存柊涓哄師鐢熸闈紭鍏堢殑涓枃璇存槑锛屼繚鐣欐棫 Electron 璇存槑涓?legacy 鐘舵€併€?
---

### Task 1: Rust Protocol And Completion Core

**Files:**
- Create: `Cargo.toml`
- Create: `crates/halo-protocol/Cargo.toml`
- Create: `crates/halo-protocol/src/lib.rs`
- Create: `crates/halo-core/Cargo.toml`
- Create: `crates/halo-core/src/lib.rs`
- Create: `crates/halo-core/src/completion.rs`

- [ ] **Step 1: Write the failing Rust completion tests**

```rust
#[test]
fn ranks_prefix_and_current_agent_above_plain_fuzzy_matches() {
    let commands = default_commands();
    let result = complete_commands(&commands, "/co", Some("codex-cli"), &["/review"], &["/codex"]);
    assert_eq!(result[0].name, "/codex");
    assert!(result[0].score > result[1].score);
}

#[test]
fn suggests_arguments_after_command_name() {
    let commands = default_commands();
    let result = complete_commands(&commands, "/codex --", Some("codex-cli"), &[], &[]);
    let names: Vec<_> = result.iter().map(|item| item.name.as_str()).collect();
    assert!(names.contains(&"--continue"));
    assert!(names.contains(&"--model"));
    assert!(names.contains(&"--sandbox"));
}
```

- [ ] **Step 2: Run the Rust tests and verify they fail**

Run: `cargo test -p halo-core completion`

Expected: FAIL because the workspace and completion functions do not exist yet.

- [ ] **Step 3: Implement protocol types and completion logic**

Implement:
- `AgentProfile`
- `AgentProvider`
- `AgentCapability`
- `SlashCommand`
- `CompletionCandidate`
- `RuntimeEvent`
- `WorkflowKind`
- `complete_commands`
- `default_commands`

Scoring rules:
- prefix match: `40`
- fuzzy continuity: `20`
- current agent: `20`
- recent usage: `10`
- favorite: `10`

- [ ] **Step 4: Run the Rust tests and verify they pass**

Run: `cargo test -p halo-core completion`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/halo-protocol crates/halo-core
git commit -m "鏍稿績锛氬缓绔?Rust 鍗忚涓庡懡浠よˉ鍏?
```

---

### Task 2: Rust Fake Runtime And Scheduler

**Files:**
- Modify: `crates/halo-core/src/lib.rs`
- Create: `crates/halo-core/src/runtime.rs`
- Create: `crates/halo-core/src/scheduler.rs`

- [ ] **Step 1: Write the failing runtime and scheduler tests**

```rust
#[test]
fn fake_runtime_emits_ordered_events_for_each_run() {
    let runtime = FakeAgentRuntime::default();
    let events = runtime.run_scripted_agents(4);
    assert_eq!(events.len(), 4 * 7);
    for run_index in 0..4 {
        let run_id = format!("run-{}", run_index + 1);
        let seq: Vec<u64> = events
            .iter()
            .filter(|event| event.run_id == run_id)
            .map(|event| event.seq)
            .collect();
        assert_eq!(seq, vec![1, 2, 3, 4, 5, 6, 7]);
    }
}

#[test]
fn scheduler_limits_global_and_agent_concurrency() {
    let mut scheduler = RunScheduler::new(4, 2);
    for id in 0..8 {
        scheduler.enqueue("codex-cli", format!("task-{id}"));
    }
    let started = scheduler.start_ready();
    assert_eq!(started.len(), 2);
    assert_eq!(scheduler.running_count(), 2);
}
```

- [ ] **Step 2: Run the Rust tests and verify they fail**

Run: `cargo test -p halo-core runtime scheduler`

Expected: FAIL because runtime and scheduler are not implemented.

- [ ] **Step 3: Implement fake runtime and scheduler**

Implement:
- deterministic scripted event sequence:
  `run.state -> message.created -> thinking.delta -> tool.started -> tool.completed -> message.completed -> token.updated`
- `run_scripted_agents(agent_count)` with stable run ids and per-run sequence numbers
- `RunScheduler::new(max_global_runs, max_per_agent_runs)`
- `enqueue`, `start_ready`, `finish`, `running_count`, `queued_count`

- [ ] **Step 4: Run Rust tests and stress checks**

Run:
- `cargo test -p halo-core runtime`
- `cargo test -p halo-core scheduler`
- `cargo test --workspace`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/halo-core
git commit -m "鏍稿績锛氬姞鍏ュ亣杩愯鏃朵笌骞跺彂璋冨害"
```

---

### Task 3: Python Desktop Backend

**Files:**
- Create: `apps/desktop/pyproject.toml`
- Create: `apps/desktop/requirements.txt`
- Create: `apps/desktop/halo_desktop/__init__.py`
- Create: `apps/desktop/halo_desktop/models.py`
- Create: `apps/desktop/halo_desktop/completion.py`
- Create: `apps/desktop/halo_desktop/demo_runtime.py`
- Create: `apps/desktop/halo_desktop/plugin_registry.py`
- Create: `apps/desktop/halo_desktop/app_controller.py`
- Create: `apps/desktop/halo_desktop/main.py`
- Create: `apps/desktop/tests/test_completion.py`
- Create: `apps/desktop/tests/test_demo_runtime.py`
- Create: `apps/desktop/tests/test_plugin_registry.py`

- [ ] **Step 1: Write failing Python tests**

```python
def test_slash_completion_prioritizes_current_agent():
    commands = default_commands()
    result = complete_commands("/co", commands, current_agent_id="codex-cli", recent=(), favorites=("/codex",))
    assert result[0].name == "/codex"

def test_demo_runtime_emits_events_for_32_agents():
    events = run_demo_agents(agent_count=32)
    assert len(events) == 32 * 7
    assert events[0].kind == "run.state"
    assert events[-1].kind == "token.updated"
```

- [ ] **Step 2: Run Python tests and verify they fail**

Run: `python -m unittest discover -s apps/desktop/tests -v`

Expected: FAIL because package files do not exist.

- [ ] **Step 3: Implement Python backend**

Implement:
- immutable dataclasses for AgentProfile, SlashCommand, CompletionCandidate, WorkflowEvent
- command completion with the same scoring model as Rust
- demo runtime that generates deterministic events for 4/16/32 agents
- plugin registry that loads `plugins/agents/*/agent.toml` via stdlib `tomllib`
- app controller that exposes demo agents and completions without requiring PySide6 at import time
- `main.py` that launches QML only when PySide6 is installed and prints a clear install message otherwise

- [ ] **Step 4: Run Python tests and verify they pass**

Run: `python -m unittest discover -s apps/desktop/tests -v`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop
git commit -m "妗岄潰绔細寤虹珛 Python 鍘熺敓澹冲悗绔?
```

---

### Task 4: Native QML Workspace UI

**Files:**
- Create: `apps/desktop/halo_desktop/qml/Main.qml`
- Create: `apps/desktop/halo_desktop/qml/components/AgentSidebar.qml`
- Create: `apps/desktop/halo_desktop/qml/components/CommandComposer.qml`
- Create: `apps/desktop/halo_desktop/qml/components/GlowPanel.qml`
- Create: `apps/desktop/halo_desktop/qml/components/InspectorPanel.qml`
- Create: `apps/desktop/halo_desktop/qml/components/WorkflowTimeline.qml`
- Create: `apps/desktop/halo_desktop/qml/styles/Theme.qml`
- Create: `apps/desktop/tests/test_qml_static.py`

- [ ] **Step 1: Write failing QML static tests**

```python
def test_qml_avoids_expensive_animation_patterns():
    qml_text = read_all_qml()
    banned = ["ParticleSystem", "ShaderEffect", "DropShadow", "FastBlur", "NumberAnimation on x", "NumberAnimation on y"]
    for token in banned:
        assert token not in qml_text

def test_main_qml_contains_required_workspace_regions():
    main_qml = read_main_qml()
    for token in ["AgentSidebar", "WorkflowTimeline", "InspectorPanel", "CommandComposer"]:
        assert token in main_qml
```

- [ ] **Step 2: Run Python tests and verify they fail**

Run: `python -m unittest apps.desktop.tests.test_qml_static -v`

Expected: FAIL because QML files do not exist.

- [ ] **Step 3: Implement QML UI**

Implement:
- three-pane layout: 260px navigation, center workflow, 340px inspector
- dark native desktop surface with static gradient, no starfield or particle loop
- message/workflow cards: user, assistant, thinking, tool, shell, diff, summary
- command composer with `/` popup and keyboard selection
- debug terminal drawer collapsed by default
- font stack: Segoe UI, Microsoft YaHei UI, Cascadia Mono for code-like text

- [ ] **Step 4: Run QML static tests**

Run: `python -m unittest apps.desktop.tests.test_qml_static -v`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/halo_desktop/qml apps/desktop/tests/test_qml_static.py
git commit -m "鐣岄潰锛氬畬鎴愬師鐢熶笁鏍?Agent 宸ヤ綔鍙?
```

---

### Task 5: Built-In Agent Manifests

**Files:**
- Create: `plugins/agents/claude-code/agent.toml`
- Create: `plugins/agents/codex-cli/agent.toml`
- Create: `plugins/agents/opencode/agent.toml`
- Create: `plugins/agents/pi/agent.toml`
- Modify: `apps/desktop/tests/test_plugin_registry.py`

- [ ] **Step 1: Write failing manifest tests**

```python
def test_builtin_agents_are_loaded():
    registry = PluginRegistry(project_root())
    agents = registry.load_agents()
    assert {agent.id for agent in agents} == {"claude-code", "codex-cli", "opencode", "pi"}
    assert all(agent.transport == "pty" for agent in agents)
```

- [ ] **Step 2: Run manifest tests and verify they fail**

Run: `python -m unittest apps.desktop.tests.test_plugin_registry -v`

Expected: FAIL because manifests do not exist.

- [ ] **Step 3: Add built-in manifests**

Each manifest includes:
- `id`
- `name`
- `provider`
- `transport`
- `command`
- `capabilities`
- `commands`
- default permissions with shell/file write disabled for Phase 1

- [ ] **Step 4: Run manifest tests and verify they pass**

Run: `python -m unittest apps.desktop.tests.test_plugin_registry -v`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add plugins/agents apps/desktop/tests/test_plugin_registry.py
git commit -m "鎻掍欢锛氬姞鍏ュ唴缃?Agent 娓呭崟"
```

---

### Task 6: README And Verification

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Write verification checklist into README**

Document:
- project vision
- native desktop architecture
- Windows quick start
- PySide6 install
- Rust test commands
- Python test commands
- Phase 1 scope and known limitations
- legacy Electron status

- [ ] **Step 2: Run full verification**

Run:
- `cargo test --workspace`
- `python -m unittest discover -s apps/desktop/tests -v`

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add README.md docs/superpowers/plans/2026-07-22-native-agent-workspace-phase-1.md
git commit -m "鏂囨。锛氭洿鏂板師鐢熸闈㈠紑鍙戣鏄?
```

---

## Phase 1 楠屾敹鏍囧噯

- `cargo test --workspace` 閫氳繃銆?- `python -m unittest discover -s apps/desktop/tests -v` 閫氳繃銆?- 鏂板鍘熺敓妗岄潰鍏ュ彛 `apps/desktop/halo_desktop/main.py`銆?- QML 涓荤晫闈㈠叿澶?Agent 鍒囨崲銆佸伐浣滄祦鏃堕棿绾裤€両nspector銆佸懡浠よ緭鍏ヤ笌 `/` 琛ュ叏銆?- 涓嶅紩鍏ユ柊鐨?Electron銆丷eact銆乂ue銆乄ebView銆佹祻瑙堝櫒 UI銆?- fake runtime 鍙敓鎴?4/16/32 agent 鐨勭‘瀹氭€т簨浠躲€?- QML 闈欐€佹祴璇曠‘璁ゆ病鏈夌矑瀛愩€丼haderEffect銆丏ropShadow銆丗astBlur 鍜屾寔缁潗鏍囧姩鐢汇€?- README 浣跨敤涓枃璇存槑 Phase 1 濡備綍杩愯銆佸浣曟祴璇曘€佸摢浜涘姛鑳借繕娌℃湁鎺ョ湡瀹?CLI銆?