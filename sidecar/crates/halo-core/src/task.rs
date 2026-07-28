//! 任务状态机：唯一合法迁移表。
//!
//! 需求语义（requirements-alignment/01 + ipc-protocol.md 3.4）：
//! Created→Running→(AwaitingAction⇄Running)→Finishing→ReviewReady→Accepted/Rejected；
//! 任意非终态可 Fail / CancelledNative / CancelledForced / MarkInterrupted；
//! 终态不可再迁移。

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Created,
    Running,
    WaitingDeveloper,
    AwaitingAction,
    Finishing,
    ReviewReady,
    Accepted,
    Rejected,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    Started,
    RoundCompleted,
    ActionRequested,
    ActionResolved,
    Finishing,
    EvidenceReady,
    Accept,
    Reject,
    CancelledNative,
    CancelledForced,
    Fail(String),
    MarkInterrupted,
}

/// 非法迁移错误。message 只携带状态与事件名，不携带任何任务内容。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("非法的任务状态迁移：状态 {from} 不接受事件 {event}")]
pub struct TransitionError {
    pub from: TaskState,
    pub event: &'static str,
}

impl TaskState {
    pub const ALL: [TaskState; 11] = [
        TaskState::Created,
        TaskState::Running,
        TaskState::WaitingDeveloper,
        TaskState::AwaitingAction,
        TaskState::Finishing,
        TaskState::ReviewReady,
        TaskState::Accepted,
        TaskState::Rejected,
        TaskState::Cancelled,
        TaskState::Failed,
        TaskState::Interrupted,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Created => "created",
            TaskState::Running => "running",
            TaskState::WaitingDeveloper => "waiting_developer",
            TaskState::AwaitingAction => "awaiting_action",
            TaskState::Finishing => "finishing",
            TaskState::ReviewReady => "review_ready",
            TaskState::Accepted => "accepted",
            TaskState::Rejected => "rejected",
            TaskState::Cancelled => "cancelled",
            TaskState::Failed => "failed",
            TaskState::Interrupted => "interrupted",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Accepted
                | TaskState::Rejected
                | TaskState::Cancelled
                | TaskState::Failed
                | TaskState::Interrupted
        )
    }

    /// 该状态下允许携带可审查交付（review/handoff 入口开放；是否真的存在证据由
    /// EvidenceLog 决定）。accept/reject 迁移仍只在 ReviewReady 上合法。
    pub fn is_reviewable(&self) -> bool {
        matches!(
            self,
            TaskState::ReviewReady
                | TaskState::Accepted
                | TaskState::Rejected
                | TaskState::Cancelled
                | TaskState::Failed
                | TaskState::Interrupted
        )
    }

    pub fn apply(self, ev: &TaskEvent) -> Result<TaskState, TransitionError> {
        use TaskEvent as E;
        use TaskState as S;
        let next = match (self, ev) {
            (S::Created, E::Started) => Some(S::Running),
            (S::Running, E::RoundCompleted) => Some(S::WaitingDeveloper),
            (S::Running, E::ActionRequested) => Some(S::AwaitingAction),
            (S::AwaitingAction, E::ActionResolved) => Some(S::Running),
            (S::Running, E::Finishing) => Some(S::Finishing),
            (S::Finishing, E::EvidenceReady) => Some(S::ReviewReady),
            (S::ReviewReady, E::Accept) => Some(S::Accepted),
            (S::ReviewReady, E::Reject) => Some(S::Rejected),
            (s, E::CancelledNative | E::CancelledForced) if !s.is_terminal() => Some(S::Cancelled),
            (s, E::Fail(_)) if !s.is_terminal() => Some(S::Failed),
            (s, E::MarkInterrupted) if !s.is_terminal() => Some(S::Interrupted),
            _ => None,
        };
        next.ok_or(TransitionError {
            from: self,
            event: ev.name(),
        })
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TaskEvent {
    pub fn name(&self) -> &'static str {
        match self {
            TaskEvent::Started => "started",
            TaskEvent::RoundCompleted => "round_completed",
            TaskEvent::ActionRequested => "action_requested",
            TaskEvent::ActionResolved => "action_resolved",
            TaskEvent::Finishing => "finishing",
            TaskEvent::EvidenceReady => "evidence_ready",
            TaskEvent::Accept => "accept",
            TaskEvent::Reject => "reject",
            TaskEvent::CancelledNative => "cancelled_native",
            TaskEvent::CancelledForced => "cancelled_forced",
            TaskEvent::Fail(_) => "fail",
            TaskEvent::MarkInterrupted => "mark_interrupted",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_events() -> Vec<TaskEvent> {
        vec![
            TaskEvent::Started,
            TaskEvent::RoundCompleted,
            TaskEvent::ActionRequested,
            TaskEvent::ActionResolved,
            TaskEvent::Finishing,
            TaskEvent::EvidenceReady,
            TaskEvent::Accept,
            TaskEvent::Reject,
            TaskEvent::CancelledNative,
            TaskEvent::CancelledForced,
            TaskEvent::Fail("运行时崩溃".to_string()),
            TaskEvent::MarkInterrupted,
        ]
    }

    /// 期望迁移表：与需求文本逐条对照书写，作为实现的独立对照。
    fn expected(from: TaskState, ev: &TaskEvent) -> Option<TaskState> {
        use TaskState as S;
        match ev {
            TaskEvent::Started => (from == S::Created).then_some(S::Running),
            TaskEvent::RoundCompleted => {
                (from == S::Running).then_some(S::WaitingDeveloper)
            }
            TaskEvent::ActionRequested => (from == S::Running).then_some(S::AwaitingAction),
            TaskEvent::ActionResolved => (from == S::AwaitingAction).then_some(S::Running),
            TaskEvent::Finishing => (from == S::Running).then_some(S::Finishing),
            TaskEvent::EvidenceReady => (from == S::Finishing).then_some(S::ReviewReady),
            TaskEvent::Accept => (from == S::ReviewReady).then_some(S::Accepted),
            TaskEvent::Reject => (from == S::ReviewReady).then_some(S::Rejected),
            TaskEvent::CancelledNative | TaskEvent::CancelledForced => {
                (!from.is_terminal()).then_some(S::Cancelled)
            }
            TaskEvent::Fail(_) => (!from.is_terminal()).then_some(S::Failed),
            TaskEvent::MarkInterrupted => (!from.is_terminal()).then_some(S::Interrupted),
        }
    }

    #[test]
    fn full_transition_table_legal_and_illegal() {
        // 10 状态 × 11 事件 = 110 组合全覆盖
        for from in TaskState::ALL {
            for ev in all_events() {
                let got = from.apply(&ev);
                match expected(from, &ev) {
                    Some(next) => {
                        assert_eq!(got, Ok(next), "{from} + {} 应迁移到 {next}", ev.name())
                    }
                    None => {
                        let err = got.expect_err(&format!("{from} + {} 应为非法", ev.name()));
                        assert_eq!(err.from, from);
                        assert_eq!(err.event, ev.name());
                    }
                }
            }
        }
    }

    #[test]
    fn happy_path_to_accepted() {
        let mut s = TaskState::Created;
        for ev in [
            TaskEvent::Started,
            TaskEvent::Finishing,
            TaskEvent::EvidenceReady,
            TaskEvent::Accept,
        ] {
            s = s.apply(&ev).unwrap();
        }
        assert_eq!(s, TaskState::Accepted);
    }

    #[test]
    fn completed_round_waits_for_developer_without_becoming_reviewable() {
        let running = TaskState::Created.apply(&TaskEvent::Started).unwrap();
        let waiting = running.apply(&TaskEvent::RoundCompleted).unwrap();
        assert_eq!(waiting, TaskState::WaitingDeveloper);
        assert!(!waiting.is_terminal());
        assert!(!waiting.is_reviewable());
    }

    #[test]
    fn action_request_loop_then_reject() {
        let mut s = TaskState::Created.apply(&TaskEvent::Started).unwrap();
        // AwaitingAction ⇄ Running 可以往返多次
        for _ in 0..3 {
            s = s.apply(&TaskEvent::ActionRequested).unwrap();
            assert_eq!(s, TaskState::AwaitingAction);
            s = s.apply(&TaskEvent::ActionResolved).unwrap();
            assert_eq!(s, TaskState::Running);
        }
        s = s.apply(&TaskEvent::Finishing).unwrap();
        s = s.apply(&TaskEvent::EvidenceReady).unwrap();
        s = s.apply(&TaskEvent::Reject).unwrap();
        assert_eq!(s, TaskState::Rejected);
    }

    #[test]
    fn awaiting_action_cannot_finish_directly() {
        let s = TaskState::AwaitingAction;
        assert!(s.apply(&TaskEvent::Finishing).is_err());
        assert!(s.apply(&TaskEvent::EvidenceReady).is_err());
    }

    #[test]
    fn any_non_terminal_state_can_cancel_fail_interrupt() {
        for from in TaskState::ALL.into_iter().filter(|s| !s.is_terminal()) {
            assert_eq!(from.apply(&TaskEvent::CancelledNative), Ok(TaskState::Cancelled));
            assert_eq!(from.apply(&TaskEvent::CancelledForced), Ok(TaskState::Cancelled));
            assert_eq!(
                from.apply(&TaskEvent::Fail("原生通道 EOF".to_string())),
                Ok(TaskState::Failed)
            );
            assert_eq!(from.apply(&TaskEvent::MarkInterrupted), Ok(TaskState::Interrupted));
        }
    }

    #[test]
    fn terminal_states_reject_every_event() {
        for from in TaskState::ALL.into_iter().filter(TaskState::is_terminal) {
            for ev in all_events() {
                assert!(
                    from.apply(&ev).is_err(),
                    "终态 {from} 不应接受事件 {}",
                    ev.name()
                );
            }
        }
    }

    #[test]
    fn is_terminal_flags() {
        use TaskState::*;
        for s in [Accepted, Rejected, Cancelled, Failed, Interrupted] {
            assert!(s.is_terminal(), "{s} 应为终态");
        }
        for s in [
            Created,
            Running,
            WaitingDeveloper,
            AwaitingAction,
            Finishing,
            ReviewReady,
        ] {
            assert!(!s.is_terminal(), "{s} 不应为终态");
        }
    }

    #[test]
    fn is_reviewable_flags() {
        use TaskState::*;
        for s in [ReviewReady, Accepted, Rejected, Cancelled, Failed, Interrupted] {
            assert!(s.is_reviewable(), "{s} 应开放审查入口");
        }
        for s in [Created, Running, WaitingDeveloper, AwaitingAction, Finishing] {
            assert!(!s.is_reviewable(), "{s} 不应开放审查入口");
        }
    }

    #[test]
    fn transition_error_message_is_chinese_and_names_state_and_event() {
        let err = TaskState::Accepted.apply(&TaskEvent::Started).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("非法"));
        assert!(msg.contains("accepted"));
        assert!(msg.contains("started"));
    }

    #[test]
    fn state_serde_uses_snake_case() {
        let json = serde_json::to_string(&TaskState::ReviewReady).unwrap();
        assert_eq!(json, "\"review_ready\"");
        let back: TaskState = serde_json::from_str("\"awaiting_action\"").unwrap();
        assert_eq!(back, TaskState::AwaitingAction);
    }
}
