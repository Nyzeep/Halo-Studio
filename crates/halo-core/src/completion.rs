use halo_protocol::{CompletionCandidate, SlashCommand, WorkflowKind};

const PREFIX_SCORE: u32 = 40;
const FUZZY_SCORE: u32 = 20;
const CURRENT_AGENT_SCORE: u32 = 20;
const RECENT_SCORE: u32 = 10;
const FAVORITE_SCORE: u32 = 10;

pub fn complete_commands(
    commands: &[SlashCommand],
    input: &str,
    current_agent_id: Option<&str>,
    recent_usage: &[&str],
    favorites: &[&str],
) -> Vec<CompletionCandidate> {
    if let Some((command, argument_query)) = argument_context(commands, input) {
        return complete_arguments(command, argument_query);
    }

    let query = input.trim();
    let mut candidates: Vec<_> = commands
        .iter()
        .filter_map(|command| {
            score_command(command, query, current_agent_id, recent_usage, favorites).map(|score| {
                CompletionCandidate::new(
                    command.name.clone(),
                    command.description.clone(),
                    score,
                    command.agent_id.clone(),
                )
            })
        })
        .collect();

    sort_candidates(&mut candidates);
    candidates
}

pub fn default_commands() -> Vec<SlashCommand> {
    vec![
        SlashCommand::new(
            "/codex",
            "Start a Codex CLI coding run",
            Some("codex-cli"),
            WorkflowKind::Chat,
            vec![
                "--continue".to_string(),
                "--model".to_string(),
                "--sandbox".to_string(),
            ],
        ),
        SlashCommand::new(
            "/continue",
            "Continue the active agent run",
            Some("codex-cli"),
            WorkflowKind::Chat,
            vec!["--last".to_string()],
        ),
        SlashCommand::new(
            "/claude",
            "Start a Claude Code run",
            Some("claude-code"),
            WorkflowKind::Chat,
            vec!["--resume".to_string(), "--model".to_string()],
        ),
        SlashCommand::new(
            "/opencode",
            "Start an OpenCode run",
            Some("opencode"),
            WorkflowKind::Chat,
            vec!["--model".to_string()],
        ),
        SlashCommand::new(
            "/pi",
            "Ask Pi for conversational help",
            Some("pi"),
            WorkflowKind::Chat,
            Vec::new(),
        ),
        SlashCommand::new(
            "/review",
            "Run a focused code review workflow",
            None::<String>,
            WorkflowKind::Review,
            vec!["--scope".to_string()],
        ),
        SlashCommand::new(
            "/plan",
            "Create an implementation plan",
            None::<String>,
            WorkflowKind::Planning,
            vec!["--write".to_string()],
        ),
    ]
}

fn argument_context<'a>(
    commands: &'a [SlashCommand],
    input: &'a str,
) -> Option<(&'a SlashCommand, &'a str)> {
    let trimmed = input.trim_start();
    let split_at = trimmed.find(char::is_whitespace)?;
    let command_name = &trimmed[..split_at];
    let argument_query = trimmed[split_at..].trim_start();
    let command = commands.iter().find(|item| item.name == command_name)?;

    Some((command, argument_query))
}

fn complete_arguments(command: &SlashCommand, query: &str) -> Vec<CompletionCandidate> {
    let mut candidates: Vec<_> = command
        .arguments
        .iter()
        .filter_map(|argument| {
            score_text(argument, query).map(|score| {
                CompletionCandidate::new(
                    argument.clone(),
                    format!("Argument for {}", command.name),
                    score,
                    command.agent_id.clone(),
                )
            })
        })
        .collect();

    sort_candidates(&mut candidates);
    candidates
}

fn score_command(
    command: &SlashCommand,
    query: &str,
    current_agent_id: Option<&str>,
    recent_usage: &[&str],
    favorites: &[&str],
) -> Option<u32> {
    let mut score = score_text(&command.name, query)?;

    if current_agent_id.is_some_and(|agent_id| command.agent_id.as_deref() == Some(agent_id)) {
        score += CURRENT_AGENT_SCORE;
    }

    if recent_usage.iter().any(|name| *name == command.name) {
        score += RECENT_SCORE;
    }

    if favorites.iter().any(|name| *name == command.name) {
        score += FAVORITE_SCORE;
    }

    Some(score)
}

fn score_text(candidate: &str, query: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }

    let candidate = candidate.to_ascii_lowercase();
    let query = query.to_ascii_lowercase();

    if candidate.starts_with(&query) {
        return Some(PREFIX_SCORE);
    }

    if fuzzy_contiguous_match(&candidate, &query) {
        return Some(FUZZY_SCORE);
    }

    None
}

fn fuzzy_contiguous_match(candidate: &str, query: &str) -> bool {
    let mut query_chars = query.chars();
    let Some(mut expected) = query_chars.next() else {
        return true;
    };

    for candidate_char in candidate.chars() {
        if candidate_char == expected {
            match query_chars.next() {
                Some(next) => expected = next,
                None => return true,
            }
        }
    }

    false
}

fn sort_candidates(candidates: &mut [CompletionCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.name.cmp(&right.name))
    });
}
