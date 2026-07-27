//! stdio JSONL 服务：stdin 读循环 + stdout 唯一写线程 + EventBus。
//!
//! 顺序保证：seq 分配、环形缓冲写入与出站信道投递在同一把锁内完成，
//! 因此写线程看到的顺序 == seq 顺序 == 缓冲顺序；响应与事件共用同一条
//! 出站信道，全部经唯一写线程落盘，杜绝交错写坏 JSONL。

use std::collections::VecDeque;
use std::io::BufRead;
use std::sync::Mutex;

use crossbeam_channel::{Receiver, Sender};
use serde_json::Value;

use halo_protocol::{
    read_message, write_message, ErrorBody, ErrorCode, Event, Inbound, ProtocolError, Response,
    PROTOCOL_VERSION,
};

use crate::mapping::now_ts;

/// 环形缓冲容量：契约要求至少保留最近 1024 条事件以支持界面恢复。
pub const EVENT_RING_CAPACITY: usize = 1024;

/// 出站消息：响应与事件统一走写线程。
#[derive(Debug, Clone)]
pub enum Outbound {
    Response(Response),
    Event(Event),
}

/// 缓冲不足以覆盖 after_seq：UI 应整体重建视图。
#[derive(Debug, thiserror::Error)]
#[error("事件缓冲不足以覆盖 seq {after_seq}，最早可用 seq 为 {oldest}")]
pub struct EventGapError {
    pub after_seq: u64,
    pub oldest: u64,
}

struct BusInner {
    next_seq: u64,
    ring: VecDeque<Event>,
}

pub struct EventBus {
    tx: Sender<Outbound>,
    inner: Mutex<BusInner>,
}

impl EventBus {
    pub fn new(tx: Sender<Outbound>) -> Self {
        EventBus {
            tx,
            inner: Mutex::new(BusInner {
                next_seq: 1,
                ring: VecDeque::with_capacity(EVENT_RING_CAPACITY),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BusInner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// 分配全局 seq、写入环形缓冲并投递给写线程；返回分配的 seq。
    pub fn emit(&self, task_id: Option<&str>, event: &str, payload: Value) -> u64 {
        let mut inner = self.lock();
        let seq = inner.next_seq;
        inner.next_seq += 1;
        let ev = Event {
            v: PROTOCOL_VERSION,
            seq,
            ts: now_ts(),
            task_id: task_id.map(str::to_string),
            event: event.to_string(),
            payload,
        };
        inner.ring.push_back(ev.clone());
        while inner.ring.len() > EVENT_RING_CAPACITY {
            inner.ring.pop_front();
        }
        let _ = self.tx.send(Outbound::Event(ev));
        seq
    }

    /// 响应与事件共用出站信道，保持全序。
    pub fn respond(&self, resp: Response) {
        let _ = self.tx.send(Outbound::Response(resp));
    }

    /// 生产路径经 events_after 取 last_seq；单测断言使用本方法。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn last_seq(&self) -> u64 {
        self.lock().next_seq - 1
    }

    /// 返回 (last_seq, after_seq 之后的全部事件)；缓冲不足覆盖时返回 EventGapError。
    pub fn events_after(&self, after_seq: u64) -> Result<(u64, Vec<Event>), EventGapError> {
        let inner = self.lock();
        let last = inner.next_seq - 1;
        if after_seq >= last {
            return Ok((last, Vec::new()));
        }
        let oldest = match inner.ring.front() {
            Some(e) => e.seq,
            None => {
                // 有事件被分配过（last > after_seq）但缓冲为空：全部丢失
                return Err(EventGapError {
                    after_seq,
                    oldest: last + 1,
                });
            }
        };
        if after_seq + 1 < oldest {
            return Err(EventGapError { after_seq, oldest });
        }
        let events = inner
            .ring
            .iter()
            .filter(|e| e.seq > after_seq)
            .cloned()
            .collect();
        Ok((last, events))
    }
}

/// 唯一写线程：串行消费出站信道并写 stdout。写失败（UI 端关闭）即退出。
pub fn spawn_writer<W: std::io::Write + Send + 'static>(
    mut w: W,
    rx: Receiver<Outbound>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for msg in rx {
            let result = match &msg {
                Outbound::Response(r) => write_message(&mut w, r),
                Outbound::Event(e) => write_message(&mut w, e),
            };
            if result.is_err() {
                return;
            }
        }
    })
}

/// 协议层错误 → 失败响应。id 尽力从原始行提取；错误文案不回显行内容。
pub fn protocol_error_response(line: &str, err: &ProtocolError) -> Response {
    let id = if line.len() <= halo_protocol::MAX_LINE_BYTES {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let body = match err {
        ProtocolError::LineTooLong { .. } => ErrorBody::new(ErrorCode::LineTooLong, err.to_string()),
        ProtocolError::UnsupportedVersion { .. } => ErrorBody::with_details(
            ErrorCode::ProtocolVersionUnsupported,
            err.to_string(),
            serde_json::json!({"sidecar_protocol_versions": [PROTOCOL_VERSION]}),
        ),
        ProtocolError::Parse { .. } | ProtocolError::UnexpectedKind { .. } => {
            ErrorBody::new(ErrorCode::ParseError, err.to_string())
        }
        ProtocolError::Serialize { .. } | ProtocolError::Io(_) => {
            ErrorBody::new(ErrorCode::Internal, err.to_string())
        }
    };
    Response::failure(id, body)
}

/// stdin 读循环：一行一请求，经 dispatcher 得到响应后统一投递写线程。
/// EOF（UI 关闭）返回；非 UTF-8 行回 PARSE_ERROR 后继续。
pub fn read_loop<R: BufRead>(reader: R, dispatcher: &mut crate::dispatch::Dispatcher) {
    for line in reader.lines() {
        match line {
            Ok(text) => {
                // 兼容带 UTF-8 BOM 的写端（如 PowerShell 管道），剥掉行首 BOM
                let text = text.trim_start_matches('\u{feff}').to_string();
                if text.trim().is_empty() {
                    continue;
                }
                match read_message(&text) {
                    Ok(Inbound::Request(req)) => {
                        let resp = dispatcher.dispatch(req);
                        dispatcher.bus().respond(resp);
                    }
                    Err(err) => {
                        dispatcher
                            .bus()
                            .respond(protocol_error_response(&text, &err));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                dispatcher.bus().respond(Response::failure(
                    "",
                    ErrorBody::new(ErrorCode::ParseError, "请求行不是合法的 UTF-8 文本"),
                ));
            }
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use serde_json::json;

    fn bus() -> (EventBus, Receiver<Outbound>) {
        let (tx, rx) = unbounded();
        (EventBus::new(tx), rx)
    }

    #[test]
    fn seq_starts_at_one_and_is_strictly_increasing() {
        let (bus, rx) = bus();
        for i in 1..=5u64 {
            let seq = bus.emit(None, "task.phase", json!({"n": i}));
            assert_eq!(seq, i);
        }
        assert_eq!(bus.last_seq(), 5);
        let mut prev = 0;
        for _ in 0..5 {
            match rx.recv().unwrap() {
                Outbound::Event(e) => {
                    assert!(e.seq > prev, "seq 必须严格递增");
                    prev = e.seq;
                }
                other => panic!("应为事件：{other:?}"),
            }
        }
    }

    #[test]
    fn ring_keeps_exactly_last_1024_events() {
        let (bus, _rx) = bus();
        for _ in 0..1500 {
            bus.emit(None, "trace.item", json!({}));
        }
        // 最近 1024 条：seq 477..=1500
        let (last, events) = bus.events_after(476).unwrap();
        assert_eq!(last, 1500);
        assert_eq!(events.len(), 1024);
        assert_eq!(events.first().map(|e| e.seq), Some(477));
        assert_eq!(events.last().map(|e| e.seq), Some(1500));
    }

    #[test]
    fn events_after_gap_when_buffer_no_longer_covers() {
        let (bus, _rx) = bus();
        for _ in 0..1500 {
            bus.emit(None, "trace.item", json!({}));
        }
        let err = bus.events_after(0).expect_err("超出缓冲应报 EVENT_GAP");
        assert_eq!(err.oldest, 477);
        // 刚好覆盖边界：after_seq = oldest-1 = 476 可用
        assert!(bus.events_after(476).is_ok());
        assert!(bus.events_after(475).is_err());
    }

    #[test]
    fn events_after_tail_and_empty_bus() {
        let (bus, _rx) = bus();
        assert_eq!(bus.events_after(0).unwrap(), (0, vec![]));
        bus.emit(None, "a", json!({}));
        bus.emit(None, "b", json!({}));
        let (last, evs) = bus.events_after(1).unwrap();
        assert_eq!(last, 2);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].event, "b");
        // after_seq 超过 last：容忍返回空
        assert_eq!(bus.events_after(99).unwrap(), (2, vec![]));
    }

    #[test]
    fn responses_and_events_share_channel_in_order() {
        let (bus, rx) = bus();
        bus.emit(None, "e1", json!({}));
        bus.respond(Response::success("r-1", json!({"ok": true})));
        bus.emit(Some("task-1"), "e2", json!({}));
        match rx.recv().unwrap() {
            Outbound::Event(e) => assert_eq!(e.event, "e1"),
            other => panic!("{other:?}"),
        }
        match rx.recv().unwrap() {
            Outbound::Response(r) => assert_eq!(r.id, "r-1"),
            other => panic!("{other:?}"),
        }
        match rx.recv().unwrap() {
            Outbound::Event(e) => {
                assert_eq!(e.event, "e2");
                assert_eq!(e.task_id.as_deref(), Some("task-1"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn protocol_error_response_maps_codes() {
        let resp = protocol_error_response(
            "{}",
            &ProtocolError::LineTooLong { actual: 2_000_000 },
        );
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some(ErrorCode::LineTooLong));

        let resp = protocol_error_response("not json", &ProtocolError::Parse { detail: "x".into() });
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some(ErrorCode::ParseError));

        let resp = protocol_error_response(
            r#"{"v":2,"kind":"request","id":"r-9","method":"m","params":{}}"#,
            &ProtocolError::UnsupportedVersion { found: 2 },
        );
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, ErrorCode::ProtocolVersionUnsupported);
        assert_eq!(resp.id, "r-9", "应尽力提取原请求 id");
        assert_eq!(err.details["sidecar_protocol_versions"][0], 1);
    }

    #[test]
    fn writer_thread_serializes_jsonl_lines() {
        let (tx, rx) = unbounded();
        let buf: std::sync::Arc<Mutex<Vec<u8>>> = Default::default();
        struct SharedWriter(std::sync::Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for SharedWriter {
            fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(data);
                Ok(data.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let handle = spawn_writer(SharedWriter(buf.clone()), rx);
        let bus = EventBus::new(tx);
        bus.emit(None, "sidecar.state", json!({"state": "ready", "protocol_version": 1}));
        bus.respond(Response::success("r-1", json!({})));
        drop(bus); // 关闭信道让写线程退出
        handle.join().unwrap();

        let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let ev: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(ev["kind"], "event");
        assert_eq!(ev["seq"], 1);
        let resp: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(resp["kind"], "response");
        assert_eq!(resp["ok"], true);
    }
}
