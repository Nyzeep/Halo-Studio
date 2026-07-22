use halo_ipc::{decode_command, encode_event, encode_snapshot, IpcError};
use halo_protocol::{RunSnapshot, RuntimeCommand, RuntimeEvent};

#[test]
fn parses_create_run_command() {
    let command = decode_command(
        r#"{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"hello"}"#,
    )
    .unwrap();

    assert_eq!(command.run_id(), Some("run-1"));
    assert_eq!(command.agent_id(), Some("codex-cli"));
    assert_eq!(
        command,
        RuntimeCommand::create_run("run-1", "codex-cli", "hello")
    );
}

#[test]
fn parses_escaped_prompt_text() {
    let command = decode_command(
        r#"{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"say \"hi\" \\ 再见\nnext"}"#,
    )
    .unwrap();

    assert_eq!(
        command,
        RuntimeCommand::create_run("run-1", "codex-cli", "say \"hi\" \\ 再见\nnext")
    );
}

#[test]
fn parses_get_snapshot_and_shutdown_commands() {
    assert_eq!(
        decode_command(r#"{"type":"getSnapshot","runId":"run-1"}"#).unwrap(),
        RuntimeCommand::get_snapshot("run-1")
    );
    assert_eq!(
        decode_command(r#"{"type":"shutdown"}"#).unwrap(),
        RuntimeCommand::Shutdown
    );
}

#[test]
fn rejects_unknown_command_type() {
    let error = decode_command(r#"{"type":"launch"}"#).unwrap_err();

    assert_eq!(error, IpcError::UnknownCommand("launch".to_string()));
}

#[test]
fn encodes_event_as_jsonl_envelope() {
    let event = RuntimeEvent::new(
        "run-1",
        "codex-cli",
        1,
        "message.delta",
        "say \"hi\" \\ 再见\nnext",
    );

    assert_eq!(
        encode_event(&event),
        r#"{"type":"runtimeEvent","runId":"run-1","agentId":"codex-cli","seq":1,"kind":"message.delta","message":"say \"hi\" \\ 再见\nnext"}"#
    );
}

#[test]
fn encodes_snapshot_with_ring_buffer_events() {
    let mut snapshot = RunSnapshot::new("run-1", "codex-cli", 2);
    snapshot.push_event(RuntimeEvent::new(
        "run-1",
        "codex-cli",
        1,
        "run.state",
        "running",
    ));
    snapshot.push_event(RuntimeEvent::new(
        "run-1",
        "codex-cli",
        2,
        "message.delta",
        "hello",
    ));

    assert_eq!(
        encode_snapshot(&snapshot),
        r#"{"type":"snapshot","runId":"run-1","agentId":"codex-cli","state":"running","lastSeq":2,"events":[{"type":"runtimeEvent","runId":"run-1","agentId":"codex-cli","seq":1,"kind":"run.state","message":"running"},{"type":"runtimeEvent","runId":"run-1","agentId":"codex-cli","seq":2,"kind":"message.delta","message":"hello"}]}"#
    );
}
