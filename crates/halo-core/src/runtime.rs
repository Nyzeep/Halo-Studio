use halo_protocol::RuntimeEvent;

const SCRIPTED_EVENT_KINDS: [&str; 7] = [
    "run.state",
    "message.created",
    "thinking.delta",
    "tool.started",
    "tool.completed",
    "message.completed",
    "token.updated",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeAgentRuntime {
    agent_ids: Vec<String>,
}

impl Default for FakeAgentRuntime {
    fn default() -> Self {
        Self {
            agent_ids: vec![
                "codex-cli".to_string(),
                "claude-code".to_string(),
                "opencode".to_string(),
                "pi".to_string(),
            ],
        }
    }
}

impl FakeAgentRuntime {
    pub fn new(agent_ids: Vec<String>) -> Self {
        Self { agent_ids }
    }

    pub fn run_scripted_agents(&self, agent_count: usize) -> Vec<RuntimeEvent> {
        let mut events = Vec::with_capacity(agent_count * SCRIPTED_EVENT_KINDS.len());

        for sequence_index in 0..SCRIPTED_EVENT_KINDS.len() {
            for run_index in 0..agent_count {
                let run_id = format!("run-{}", run_index + 1);
                let agent_id = self.agent_id_for(run_index);
                let seq = (sequence_index + 1) as u64;
                let kind = SCRIPTED_EVENT_KINDS[sequence_index];
                let message = format!("{kind} for {run_id}");

                events.push(RuntimeEvent::new(run_id, agent_id, seq, kind, message));
            }
        }

        events
    }

    fn agent_id_for(&self, run_index: usize) -> String {
        if self.agent_ids.is_empty() {
            return format!("agent-{}", run_index + 1);
        }

        self.agent_ids[run_index % self.agent_ids.len()].clone()
    }
}
