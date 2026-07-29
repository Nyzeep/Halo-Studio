//! Pi RPC 适配器：`<exe> --rpc` 后 stdio JSONL。
//! 传输抽象为“读写对”（生产 = ChildStdin/ChildStdout，测试 = 内存管道），
//! 便于不 spawn 真进程即可测分帧、乱序响应、EOF、坏帧与就绪超时。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::{bounded, Sender};
use serde_json::{json, Value};

use crate::process::{probe_version, wait_exit, ChildProcess, RealChild};
use crate::{
    lock, map_trace_event, LaunchCmd, RunTaskSpec, RuntimeError, RuntimeEvent, RuntimeState,
    StopOutcome, Timeouts,
};

pub struct PiRuntime;

impl PiRuntime {
    /// 探测 `<exe> --version`，返回首行中的 semver。
    pub fn probe(exe: &str) -> Result<String, RuntimeError> {
        probe_version(exe, "Pi")
    }

    /// 启动 `<exe> --rpc`（stdio JSONL），完成 get_state 就绪检查后返回句柄。
    pub fn start(
        cmd: LaunchCmd,
        tx: Sender<RuntimeEvent>,
        opts: Timeouts,
    ) -> Result<PiHandle, RuntimeError> {
        let mut command = Command::new(&cmd.exe);
        command.arg("--rpc");
        // 子进程环境 = halo-config 构好的白名单环境，宿主其余变量一律不继承
        command.env_clear();
        command.envs(&cmd.env);
        if !cmd.cwd.is_empty() {
            command.current_dir(&cmd.cwd);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|e| RuntimeError::Spawn(format!("无法启动 Pi 进程：{e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RuntimeError::Spawn("无法获取 Pi 的标准输入通道".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RuntimeError::Spawn("无法获取 Pi 的标准输出通道".to_string()))?;
        start_with_transport(
            Box::new(stdin),
            Box::new(BufReader::new(stdout)),
            Box::new(RealChild::new(child)),
            tx,
            opts,
        )
    }
}

/// 挂起请求路由表：响应按 id 匹配，天然容忍乱序；未知 id 一律忽略。
enum Pending {
    /// 同步等待方（就绪检查）
    Reply(Sender<Value>),
    RunTask,
    Cancel,
}

struct PiShared {
    state: Mutex<RuntimeState>,
    tx: Sender<RuntimeEvent>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    pending: Mutex<HashMap<u64, Pending>>,
    next_id: AtomicU64,
    child: Mutex<Box<dyn ChildProcess>>,
}

impl PiShared {
    fn set_state(&self, s: RuntimeState) {
        *lock(&self.state) = s.clone();
        let _ = self.tx.send(RuntimeEvent::State(s));
    }

    /// 失败收口：关输入通道 + 丢弃全部等待方（让同步等待者立即感知），再广播 Failed。
    fn set_failed(&self, reason: &str, recovery_hint: &str) {
        lock(&self.writer).take();
        lock(&self.pending).clear();
        self.set_state(RuntimeState::Failed {
            reason: reason.to_string(),
            recovery_hint: recovery_hint.to_string(),
        });
    }

    fn is_shutting_down(&self) -> bool {
        matches!(
            *lock(&self.state),
            RuntimeState::Stopping | RuntimeState::Stopped
        )
    }

    fn send_frame(&self, frame: &Value) -> Result<(), RuntimeError> {
        let mut guard = lock(&self.writer);
        let writer = guard
            .as_mut()
            .ok_or_else(|| RuntimeError::Io("Pi 输入通道已关闭".to_string()))?;
        let mut line = frame.to_string();
        line.push('\n');
        writer
            .write_all(line.as_bytes())
            .and_then(|_| writer.flush())
            .map_err(|e| RuntimeError::Io(format!("向 Pi 写入请求失败：{e}")))
    }

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }
}

/// 以注入的读写对与子进程监督启动 Pi 适配器（生产与测试共用的唯一入口）。
pub(crate) fn start_with_transport(
    writer: Box<dyn Write + Send>,
    reader: Box<dyn BufRead + Send>,
    child: Box<dyn ChildProcess>,
    tx: Sender<RuntimeEvent>,
    opts: Timeouts,
) -> Result<PiHandle, RuntimeError> {
    let shared = Arc::new(PiShared {
        state: Mutex::new(RuntimeState::Starting),
        tx,
        writer: Mutex::new(Some(writer)),
        pending: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        child: Mutex::new(child),
    });
    let _ = shared.tx.send(RuntimeEvent::State(RuntimeState::Starting));

    {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || reader_loop(&shared, reader));
    }

    // 就绪检查：get_state 必须在 opts.ready 内返回 result.state == "idle"
    let id = shared.next_id();
    let (reply_tx, reply_rx) = bounded::<Value>(1);
    lock(&shared.pending).insert(id, Pending::Reply(reply_tx));
    if let Err(e) = shared.send_frame(&json!({"id": id, "method": "get_state"})) {
        shared.set_failed(
            "无法向 Pi 发送就绪检查请求",
            "请确认 Pi 进程仍在运行后重新启动运行时",
        );
        lock(&shared.child).kill();
        return Err(e);
    }

    match reply_rx.recv_timeout(opts.ready) {
        Ok(v) => {
            let state_str = v
                .pointer("/result/state")
                .and_then(Value::as_str)
                .unwrap_or("<缺失>");
            if state_str == "idle" {
                // 仅允许从 Starting 迁移到 Ready：若读取线程已抢先标记 Failed（EOF/坏帧），
                // 就绪结论必须让位于失败结论，避免把已死的运行时报告为可用。
                let became_ready = {
                    let mut st = lock(&shared.state);
                    if matches!(*st, RuntimeState::Starting) {
                        *st = RuntimeState::Ready;
                        true
                    } else {
                        false
                    }
                };
                if became_ready {
                    let _ = shared.tx.send(RuntimeEvent::State(RuntimeState::Ready));
                    Ok(PiHandle { shared })
                } else {
                    let reason = match &*lock(&shared.state) {
                        RuntimeState::Failed { reason, .. } => reason.clone(),
                        _ => "Pi 在就绪检查完成前已失败".to_string(),
                    };
                    lock(&shared.child).kill();
                    Err(RuntimeError::NotReady(reason))
                }
            } else {
                let reason = format!("Pi 就绪检查返回非 idle 状态：{state_str}");
                shared.set_failed(&reason, "请检查 Pi 是否存在残留会话，重启运行时后重试");
                lock(&shared.child).kill();
                Err(RuntimeError::NotReady(reason))
            }
        }
        Err(_) => {
            // 超时，或读取线程已先行进入 Failed（EOF/坏帧会清空 pending 使等待方断开）
            let reason = match &*lock(&shared.state) {
                RuntimeState::Failed { reason, .. } => reason.clone(),
                _ => String::new(),
            };
            let reason = if reason.is_empty() {
                let r = "Pi 就绪检查超时：未在限定时间内收到 get_state 响应".to_string();
                shared.set_failed(&r, "请确认可执行文件支持 --rpc 模式，或调大就绪超时后重试");
                r
            } else {
                reason
            };
            lock(&shared.child).kill();
            Err(RuntimeError::NotReady(reason))
        }
    }
}

fn reader_loop(shared: &Arc<PiShared>, mut reader: Box<dyn BufRead + Send>) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                if !shared.is_shutting_down() {
                    shared.set_failed(
                        "Pi 进程输出流意外结束（EOF）",
                        "Pi 可能已崩溃或被外部终止；请查看 Pi 日志后重新启动运行时",
                    );
                }
                return;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(trimmed) {
                    Ok(frame) => handle_frame(shared, frame),
                    Err(_) => {
                        shared.set_failed(
                            "Pi 输出了无法解析的协议帧（非法 JSON）",
                            "请重启 Pi 运行时；若问题持续，请确认 Pi 版本与 Halo Studio 兼容",
                        );
                        return;
                    }
                }
            }
            Err(e) => {
                if !shared.is_shutting_down() {
                    shared.set_failed(
                        &format!("读取 Pi 输出失败：{e}"),
                        "请重新启动 Pi 运行时",
                    );
                }
                return;
            }
        }
    }
}

fn handle_frame(shared: &Arc<PiShared>, frame: Value) {
    // 通知帧：{"method":"event","params":{TraceItem 同构}}
    if frame.get("method").and_then(Value::as_str) == Some("event") {
        let params = frame
            .get("params")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        let _ = shared.tx.send(map_trace_event(&params));
        return;
    }
    // 响应帧：按 id 路由；乱序与未知 id 一律容忍
    if let Some(id) = frame.get("id").and_then(Value::as_u64) {
        let pending = lock(&shared.pending).remove(&id);
        match pending {
            Some(Pending::Reply(sender)) => {
                let _ = sender.send(frame);
            }
            Some(Pending::RunTask) => {
                if let Some(result) = frame.get("result") {
                    let outcome = result
                        .get("outcome")
                        .and_then(Value::as_str)
                        .unwrap_or("failed")
                        .to_string();
                    let summary = result
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let _ = shared.tx.send(RuntimeEvent::TaskDone { outcome, summary });
                } else {
                    let summary = frame
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("Pi 返回了任务错误")
                        .to_string();
                    let _ = shared.tx.send(RuntimeEvent::TaskDone {
                        outcome: "failed".to_string(),
                        summary,
                    });
                }
            }
            Some(Pending::Cancel) | None => {}
        }
    }
    // 其余合法 JSON 但未知形状的帧：容忍忽略，不视为协议失败
}

pub struct PiHandle {
    shared: Arc<PiShared>,
}

impl std::fmt::Debug for PiHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PiHandle")
            .field("state", &*lock(&self.shared.state))
            .finish_non_exhaustive()
    }
}

impl PiHandle {
    /// 发送 run_task 请求；任务过程与终局经事件通道（Trace/ActionRequest/Verification/TaskDone）送出。
    pub fn run_task(&self, spec: &RunTaskSpec) -> Result<(), RuntimeError> {
        if *lock(&self.shared.state) != RuntimeState::Ready {
            return Err(RuntimeError::InvalidState);
        }
        let params = serde_json::to_value(spec)
            .map_err(|e| RuntimeError::Io(format!("任务参数序列化失败：{e}")))?;
        let id = self.shared.next_id();
        lock(&self.shared.pending).insert(id, Pending::RunTask);
        let frame = json!({"id": id, "method": "run_task", "params": params});
        if let Err(e) = self.shared.send_frame(&frame) {
            lock(&self.shared.pending).remove(&id);
            return Err(e);
        }
        Ok(())
    }

    /// 经原生通道请求取消（尽力而为，不阻塞等待结果）。
    pub fn cancel_native(&self) {
        let id = self.shared.next_id();
        lock(&self.shared.pending).insert(id, Pending::Cancel);
        let _ = self
            .shared
            .send_frame(&json!({"id": id, "method": "cancel"}));
    }

    /// 温和停止：发原生 cancel + 关 stdin，等 grace；超时强杀 → Forced。
    pub fn stop(&self, grace: Duration) -> StopOutcome {
        if matches!(*lock(&self.shared.state), RuntimeState::Stopped) {
            return StopOutcome::Graceful;
        }
        self.shared.set_state(RuntimeState::Stopping);
        let id = self.shared.next_id();
        lock(&self.shared.pending).insert(id, Pending::Cancel);
        let _ = self
            .shared
            .send_frame(&json!({"id": id, "method": "cancel"}));
        lock(&self.shared.writer).take();
        let exited = {
            let mut child = lock(&self.shared.child);
            wait_exit(child.as_mut(), grace)
        };
        let outcome = if exited {
            StopOutcome::Graceful
        } else {
            lock(&self.shared.child).kill();
            StopOutcome::Forced
        };
        self.shared.set_state(RuntimeState::Stopped);
        outcome
    }

    pub fn state(&self) -> RuntimeState {
        lock(&self.shared.state).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::testchild::TestChild;
    use crossbeam_channel::{unbounded, Receiver};
    use std::collections::VecDeque;
    use std::io::Read;
    use std::sync::atomic::AtomicBool;
    use std::sync::Condvar;
    use std::time::Instant;

    // ---- 内存管道：模拟 ChildStdin/ChildStdout ----

    struct PipeInner {
        buf: VecDeque<u8>,
        closed: bool,
    }

    #[derive(Clone)]
    struct Pipe(Arc<(Mutex<PipeInner>, Condvar)>);

    struct PipeWriter(Pipe);
    struct PipeReader(Pipe);

    fn pipe() -> (PipeWriter, PipeReader) {
        let shared = Pipe(Arc::new((
            Mutex::new(PipeInner {
                buf: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        )));
        (PipeWriter(shared.clone()), PipeReader(shared))
    }

    impl Write for PipeWriter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            let (m, cv) = &*(self.0).0;
            let mut g = m.lock().unwrap();
            g.buf.extend(data.iter().copied());
            cv.notify_all();
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Drop for PipeWriter {
        fn drop(&mut self) {
            let (m, cv) = &*(self.0).0;
            if let Ok(mut g) = m.lock() {
                g.closed = true;
                cv.notify_all();
            }
        }
    }

    impl Read for PipeReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let (m, cv) = &*(self.0).0;
            let mut g = m.lock().unwrap();
            loop {
                if !g.buf.is_empty() {
                    let n = out.len().min(g.buf.len());
                    for slot in out.iter_mut().take(n) {
                        *slot = g.buf.pop_front().unwrap();
                    }
                    return Ok(n);
                }
                if g.closed {
                    return Ok(0);
                }
                g = cv.wait(g).unwrap();
            }
        }
    }

    // ---- 假 Pi：脚本化对端 ----

    struct FakePi {
        reader: BufReader<PipeReader>,
        writer: Option<PipeWriter>,
        /// 收到的请求 method 序列，主测试线程可断言
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl FakePi {
        fn read_frame(&mut self) -> Option<Value> {
            let mut line = String::new();
            loop {
                line.clear();
                match self.reader.read_line(&mut line) {
                    Ok(0) => return None,
                    Ok(_) => {
                        let t = line.trim();
                        if t.is_empty() {
                            continue;
                        }
                        let v: Value = serde_json::from_str(t).expect("假 Pi 收到非法 JSON");
                        if let Some(m) = v.get("method").and_then(Value::as_str) {
                            self.seen.lock().unwrap().push(m.to_string());
                        }
                        return Some(v);
                    }
                    Err(_) => return None,
                }
            }
        }

        fn send(&mut self, v: &Value) {
            if let Some(w) = self.writer.as_mut() {
                let mut s = v.to_string();
                s.push('\n');
                let _ = w.write_all(s.as_bytes());
            }
        }

        fn send_raw(&mut self, s: &str) {
            if let Some(w) = self.writer.as_mut() {
                let _ = w.write_all(s.as_bytes());
            }
        }

        fn close_output(&mut self) {
            self.writer = None;
        }
    }

    struct Setup {
        events: Receiver<RuntimeEvent>,
        child: TestChild,
        seen: Arc<Mutex<Vec<String>>>,
        result: std::thread::JoinHandle<Result<PiHandle, RuntimeError>>,
    }

    /// 启动 start_with_transport（后台线程）并返回假 Pi 对端。
    fn setup(opts: Timeouts, script: impl FnOnce(FakePi) + Send + 'static) -> Setup {
        let (h2f_w, h2f_r) = pipe(); // 句柄 → 假 Pi
        let (f2h_w, f2h_r) = pipe(); // 假 Pi → 句柄
        let (tx, rx) = unbounded();
        let child = TestChild::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let fake = FakePi {
            reader: BufReader::new(h2f_r),
            writer: Some(f2h_w),
            seen: Arc::clone(&seen),
        };
        std::thread::spawn(move || script(fake));
        let child_for_start = child.clone();
        let result = std::thread::spawn(move || {
            start_with_transport(
                Box::new(h2f_w),
                Box::new(BufReader::new(f2h_r)),
                Box::new(child_for_start),
                tx,
                opts,
            )
        });
        Setup {
            events: rx,
            child,
            seen,
            result,
        }
    }

    fn short_opts() -> Timeouts {
        Timeouts {
            ready: Duration::from_secs(3),
            cancel_grace: Duration::from_secs(3),
            shutdown_grace: Duration::from_secs(3),
        }
    }

    fn wait_event(rx: &Receiver<RuntimeEvent>, pred: impl Fn(&RuntimeEvent) -> bool) -> RuntimeEvent {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("等待目标事件超时");
            let ev = rx.recv_timeout(remaining).expect("事件通道超时或断开");
            if pred(&ev) {
                return ev;
            }
        }
    }

    /// 让假 Pi 完成标准就绪应答
    fn reply_ready(fake: &mut FakePi) {
        let req = fake.read_frame().expect("未收到就绪检查请求");
        assert_eq!(req["method"], "get_state");
        let id = req["id"].as_u64().unwrap();
        fake.send(&json!({"id": id, "result": {"state": "idle"}}));
    }

    #[test]
    fn ready_then_run_task_happy_flow() {
        let done = Arc::new(AtomicBool::new(false));
        let done_for_fake = Arc::clone(&done);
        let s = setup(short_opts(), move |mut fake| {
            reply_ready(&mut fake);
            // run_task：断言分帧后逐条发通知，最后发结果
            let req = fake.read_frame().expect("未收到 run_task");
            assert_eq!(req["method"], "run_task");
            assert_eq!(req["params"]["instructions"], "修复登录超时");
            assert_eq!(req["params"]["files"][0], "src/auth.rs");
            let id = req["id"].as_u64().unwrap();
            fake.send(&json!({"method":"event","params":{"kind":"phase","text":"planning","detail":{}}}));
            fake.send(&json!({"method":"event","params":{"kind":"action_request","text":"","detail":{"request_id":"ar-1","kind":"permission","prompt":"允许写入吗？"}}}));
            fake.send(&json!({"method":"event","params":{"kind":"verification","detail":{"status":"passed","detail":"测试通过"}}}));
            fake.send(&json!({"id": id, "result": {"outcome":"finished","summary":"已完成"}}));
            done_for_fake.store(true, Ordering::SeqCst);
            // 保持对端存活直到句柄关闭 stdin
            while fake.read_frame().is_some() {}
        });

        let handle = s.result.join().unwrap().expect("启动应成功");
        assert_eq!(handle.state(), RuntimeState::Ready);
        wait_event(&s.events, |e| matches!(e, RuntimeEvent::State(RuntimeState::Ready)));

        handle
            .run_task(&RunTaskSpec {
                instructions: "修复登录超时".into(),
                files: vec!["src/auth.rs".into()],
                base_diff: None,
                notes: Some("注意保持 API 兼容".into()),
            })
            .expect("run_task 应成功");

        let ev = wait_event(&s.events, |e| matches!(e, RuntimeEvent::Trace(_)));
        match ev {
            RuntimeEvent::Trace(item) => assert_eq!(item.kind, "phase"),
            _ => unreachable!(),
        }
        let ev = wait_event(&s.events, |e| matches!(e, RuntimeEvent::ActionRequest { .. }));
        match ev {
            RuntimeEvent::ActionRequest { request_id, kind, prompt } => {
                assert_eq!(request_id, "ar-1");
                assert_eq!(kind, "permission");
                assert_eq!(prompt, "允许写入吗？");
            }
            _ => unreachable!(),
        }
        let ev = wait_event(&s.events, |e| matches!(e, RuntimeEvent::Verification { .. }));
        match ev {
            RuntimeEvent::Verification { status, detail } => {
                assert_eq!(status, "passed");
                assert_eq!(detail, "测试通过");
            }
            _ => unreachable!(),
        }
        let ev = wait_event(&s.events, |e| matches!(e, RuntimeEvent::TaskDone { .. }));
        match ev {
            RuntimeEvent::TaskDone { outcome, summary } => {
                assert_eq!(outcome, "finished");
                assert_eq!(summary, "已完成");
            }
            _ => unreachable!(),
        }
        assert!(done.load(Ordering::SeqCst));
    }

    #[test]
    fn out_of_order_and_unknown_ids_are_tolerated() {
        let s = setup(short_opts(), move |mut fake| {
            let req = fake.read_frame().expect("未收到就绪检查请求");
            let id = req["id"].as_u64().unwrap();
            // 先回一个未知 id 的响应（乱序/陈旧响应），再回真正的就绪响应
            fake.send(&json!({"id": 9999, "result": {"state": "busy"}}));
            fake.send(&json!({"id": id, "result": {"state": "idle"}}));
            while fake.read_frame().is_some() {}
        });
        let handle = s.result.join().unwrap().expect("乱序响应不应影响就绪");
        assert_eq!(handle.state(), RuntimeState::Ready);
    }

    #[test]
    fn eof_marks_failed_with_chinese_reason_and_hint() {
        let s = setup(short_opts(), move |mut fake| {
            reply_ready(&mut fake);
            // 先让就绪结论落地，再模拟 Pi 崩溃（输出流 EOF），保证测试确定性
            std::thread::sleep(Duration::from_millis(150));
            fake.close_output();
            while fake.read_frame().is_some() {}
        });
        let handle = s.result.join().unwrap().expect("启动应成功");
        let ev = wait_event(&s.events, |e| {
            matches!(e, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
        match ev {
            RuntimeEvent::State(RuntimeState::Failed { reason, recovery_hint }) => {
                assert!(reason.contains("EOF") || reason.contains("结束"), "reason={reason}");
                assert!(!recovery_hint.is_empty());
            }
            _ => unreachable!(),
        }
        assert!(matches!(handle.state(), RuntimeState::Failed { .. }));
    }

    #[test]
    fn bad_json_frame_marks_failed() {
        let s = setup(short_opts(), move |mut fake| {
            reply_ready(&mut fake);
            let _ = fake.read_frame(); // run_task
            fake.send_raw("###这不是JSON###\n");
            while fake.read_frame().is_some() {}
        });
        let handle = s.result.join().unwrap().expect("启动应成功");
        handle
            .run_task(&RunTaskSpec {
                instructions: "x".into(),
                files: vec![],
                base_diff: None,
                notes: None,
            })
            .unwrap();
        let ev = wait_event(&s.events, |e| {
            matches!(e, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
        match ev {
            RuntimeEvent::State(RuntimeState::Failed { reason, recovery_hint }) => {
                assert!(reason.contains("协议帧"), "reason={reason}");
                assert!(!recovery_hint.is_empty());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn ready_timeout_fails_and_kills_child() {
        let opts = Timeouts {
            ready: Duration::from_millis(200),
            ..short_opts()
        };
        let s = setup(opts, move |mut fake| {
            // 收到 get_state 但永不应答
            let _ = fake.read_frame();
            while fake.read_frame().is_some() {}
        });
        let err = s.result.join().unwrap().expect_err("就绪超时应失败");
        assert!(matches!(err, RuntimeError::NotReady(_)));
        assert!(format!("{err}").contains("就绪"));
        assert!(s.child.killed.load(Ordering::SeqCst), "超时后应强杀子进程");
        let ev = wait_event(&s.events, |e| {
            matches!(e, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
        match ev {
            RuntimeEvent::State(RuntimeState::Failed { reason, .. }) => {
                assert!(reason.contains("超时"), "reason={reason}");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn ready_non_idle_state_fails() {
        let s = setup(short_opts(), move |mut fake| {
            let req = fake.read_frame().expect("未收到就绪检查请求");
            let id = req["id"].as_u64().unwrap();
            fake.send(&json!({"id": id, "result": {"state": "busy"}}));
            while fake.read_frame().is_some() {}
        });
        let err = s.result.join().unwrap().expect_err("非 idle 应失败");
        assert!(matches!(err, RuntimeError::NotReady(_)));
        assert!(format!("{err}").contains("idle"));
    }

    #[test]
    fn cancel_native_sends_cancel_method() {
        let s = setup(short_opts(), move |mut fake| {
            reply_ready(&mut fake);
            while fake.read_frame().is_some() {}
        });
        let handle = s.result.join().unwrap().expect("启动应成功");
        handle.cancel_native();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if s.seen.lock().unwrap().iter().any(|m| m == "cancel") {
                break;
            }
            assert!(Instant::now() < deadline, "假 Pi 未收到 cancel 请求");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn stop_graceful_when_child_exits_in_grace() {
        let s = setup(short_opts(), move |mut fake| {
            reply_ready(&mut fake);
            // 模拟温和退出：看到 stdin EOF 后进程退出
            while fake.read_frame().is_some() {}
        });
        let handle = s.result.join().unwrap().expect("启动应成功");
        // 假子进程在对端读到 EOF 后退出：用另一线程在 stdin 关闭后置 exited
        let exited = Arc::clone(&s.child.exited);
        let seen = Arc::clone(&s.seen);
        std::thread::spawn(move || {
            // 等 cancel 送达后模拟进程自行退出
            loop {
                if seen.lock().unwrap().iter().any(|m| m == "cancel") {
                    std::thread::sleep(Duration::from_millis(30));
                    exited.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let outcome = handle.stop(Duration::from_secs(2));
        assert_eq!(outcome, StopOutcome::Graceful);
        assert_eq!(handle.state(), RuntimeState::Stopped);
        assert!(!s.child.killed.load(Ordering::SeqCst), "温和退出不应强杀");
        assert!(s.seen.lock().unwrap().iter().any(|m| m == "cancel"), "停止前应先发原生 cancel");
    }

    #[test]
    fn stop_forced_when_child_ignores_grace() {
        let s = setup(short_opts(), move |mut fake| {
            reply_ready(&mut fake);
            while fake.read_frame().is_some() {}
            // 永不设置 exited：模拟挂死进程
        });
        let handle = s.result.join().unwrap().expect("启动应成功");
        let outcome = handle.stop(Duration::from_millis(150));
        assert_eq!(outcome, StopOutcome::Forced);
        assert!(s.child.killed.load(Ordering::SeqCst), "超时后必须强杀");
        assert_eq!(handle.state(), RuntimeState::Stopped);
    }

    #[test]
    fn run_task_rejected_when_not_ready() {
        let s = setup(short_opts(), move |mut fake| {
            reply_ready(&mut fake);
            while fake.read_frame().is_some() {}
        });
        let handle = s.result.join().unwrap().expect("启动应成功");
        handle.stop(Duration::from_millis(100));
        let err = handle
            .run_task(&RunTaskSpec {
                instructions: "x".into(),
                files: vec![],
                base_diff: None,
                notes: None,
            })
            .expect_err("停止后 run_task 应被拒绝");
        assert!(matches!(err, RuntimeError::InvalidState));
    }
}
