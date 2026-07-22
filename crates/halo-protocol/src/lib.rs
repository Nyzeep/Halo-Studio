use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub provider: AgentProvider,
    pub capabilities: Vec<AgentCapability>,
    pub commands: Vec<SlashCommand>,
}

impl AgentProfile {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        provider: AgentProvider,
        capabilities: Vec<AgentCapability>,
        commands: Vec<SlashCommand>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            provider,
            capabilities,
            commands,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentProvider {
    ClaudeCode,
    CodexCli,
    OpenCode,
    Pi,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentCapability {
    Chat,
    CodeReview,
    FileEdit,
    Shell,
    Planning,
    ToolUse,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub agent_id: Option<String>,
    pub workflow: WorkflowKind,
    pub arguments: Vec<String>,
}

impl SlashCommand {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        agent_id: Option<impl Into<String>>,
        workflow: WorkflowKind,
        arguments: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            agent_id: agent_id.map(Into::into),
            workflow,
            arguments,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub name: String,
    pub description: String,
    pub score: u32,
    pub agent_id: Option<String>,
}

impl CompletionCandidate {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        score: u32,
        agent_id: Option<impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            score,
            agent_id: agent_id.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeEvent {
    pub run_id: String,
    pub agent_id: String,
    pub seq: u64,
    pub kind: String,
    pub message: String,
}

impl RuntimeEvent {
    pub fn new(
        run_id: impl Into<String>,
        agent_id: impl Into<String>,
        seq: u64,
        kind: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            agent_id: agent_id.into(),
            seq,
            kind: kind.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl RunState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSnapshot {
    run_id: String,
    agent_id: String,
    state: RunState,
    event_capacity: usize,
    last_seq: u64,
    events: VecDeque<RuntimeEvent>,
}

impl RunSnapshot {
    pub fn new(
        run_id: impl Into<String>,
        agent_id: impl Into<String>,
        event_capacity: usize,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            agent_id: agent_id.into(),
            state: RunState::Queued,
            event_capacity,
            last_seq: 0,
            events: VecDeque::with_capacity(event_capacity),
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn push_event(&mut self, event: RuntimeEvent) {
        self.last_seq = self.last_seq.max(event.seq);
        self.apply_state_event(&event);

        if self.event_capacity == 0 {
            return;
        }

        self.events.push_back(event);
        while self.events.len() > self.event_capacity {
            self.events.pop_front();
        }
    }

    pub fn events(&self) -> Vec<RuntimeEvent> {
        self.events.iter().cloned().collect()
    }

    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    pub fn state(&self) -> &RunState {
        &self.state
    }

    pub fn set_state(&mut self, state: RunState) {
        self.state = state;
    }

    fn apply_state_event(&mut self, event: &RuntimeEvent) {
        if event.kind != "run.state" {
            return;
        }

        self.state = match event.message.as_str() {
            "queued" => RunState::Queued,
            "running" => RunState::Running,
            "completed" => RunState::Completed,
            "failed" => RunState::Failed,
            "cancelled" => RunState::Cancelled,
            _ => self.state.clone(),
        };
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeCommand {
    CreateRun {
        run_id: String,
        agent_id: String,
        prompt: String,
    },
    GetSnapshot {
        run_id: String,
    },
    Shutdown,
}

impl RuntimeCommand {
    pub fn create_run(
        run_id: impl Into<String>,
        agent_id: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self::CreateRun {
            run_id: run_id.into(),
            agent_id: agent_id.into(),
            prompt: prompt.into(),
        }
    }

    pub fn get_snapshot(run_id: impl Into<String>) -> Self {
        Self::GetSnapshot {
            run_id: run_id.into(),
        }
    }

    pub fn run_id(&self) -> Option<&str> {
        match self {
            Self::CreateRun { run_id, .. } | Self::GetSnapshot { run_id } => Some(run_id),
            Self::Shutdown => None,
        }
    }

    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::CreateRun { agent_id, .. } => Some(agent_id),
            Self::GetSnapshot { .. } | Self::Shutdown => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowKind {
    Chat,
    Review,
    Shell,
    Planning,
    Custom(String),
}
