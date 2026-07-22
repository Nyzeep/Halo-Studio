use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use halo_protocol::{RunSnapshot, RuntimeEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventBusError {
    OutOfOrder {
        run_id: String,
        expected: u64,
        got: u64,
    },
    AgentMismatch {
        run_id: String,
        expected: String,
        got: String,
    },
}

impl fmt::Display for EventBusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfOrder {
                run_id,
                expected,
                got,
            } => write!(f, "expected seq {expected} for run {run_id}, got {got}"),
            Self::AgentMismatch {
                run_id,
                expected,
                got,
            } => write!(f, "expected agent {expected} for run {run_id}, got {got}"),
        }
    }
}

impl Error for EventBusError {}

#[derive(Debug, Clone)]
pub struct EventBus {
    event_capacity: usize,
    snapshots: BTreeMap<String, RunSnapshot>,
}

impl EventBus {
    pub fn new(event_capacity: usize) -> Self {
        Self {
            event_capacity,
            snapshots: BTreeMap::new(),
        }
    }

    pub fn append(&mut self, event: RuntimeEvent) -> Result<(), EventBusError> {
        let expected = self
            .snapshots
            .get(&event.run_id)
            .map_or(1, |snapshot| snapshot.last_seq() + 1);

        if event.seq != expected {
            return Err(EventBusError::OutOfOrder {
                run_id: event.run_id,
                expected,
                got: event.seq,
            });
        }

        if let Some(snapshot) = self.snapshots.get(&event.run_id) {
            if snapshot.agent_id() != event.agent_id {
                return Err(EventBusError::AgentMismatch {
                    run_id: event.run_id,
                    expected: snapshot.agent_id().to_string(),
                    got: event.agent_id,
                });
            }
        }

        let snapshot = self
            .snapshots
            .entry(event.run_id.clone())
            .or_insert_with(|| {
                RunSnapshot::new(
                    event.run_id.clone(),
                    event.agent_id.clone(),
                    self.event_capacity,
                )
            });
        snapshot.push_event(event);
        Ok(())
    }

    pub fn snapshot(&self, run_id: &str) -> Option<RunSnapshot> {
        self.snapshots.get(run_id).cloned()
    }

    pub fn snapshots(&self) -> Vec<RunSnapshot> {
        self.snapshots.values().cloned().collect()
    }
}
