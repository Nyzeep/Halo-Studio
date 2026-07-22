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
pub enum WorkflowKind {
    Chat,
    Review,
    Shell,
    Planning,
    Custom(String),
}
