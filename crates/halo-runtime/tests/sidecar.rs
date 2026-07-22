use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn sidecar_emits_ordered_events_and_snapshot() {
    let mut child = Command::new(runtime_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start halo-runtime");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(
            stdin,
            r#"{{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"hello"}}"#
        )
        .unwrap();
        writeln!(stdin, r#"{{"type":"getSnapshot","runId":"run-1"}}"#).unwrap();
        writeln!(stdin, r#"{{"type":"shutdown"}}"#).unwrap();
    }

    let output = child.wait_with_output().expect("sidecar output");
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(output.status.success());
    assert!(
        stdout.contains(r#""type":"runtimeEvent","runId":"run-1","agentId":"codex-cli","seq":1"#)
    );
    assert!(
        stdout.contains(r#""type":"runtimeEvent","runId":"run-1","agentId":"codex-cli","seq":4"#)
    );
    assert!(stdout.contains(
        r#""type":"snapshot","runId":"run-1","agentId":"codex-cli","state":"completed","lastSeq":4"#
    ));
}

#[test]
fn sidecar_reports_decode_errors_without_crashing() {
    let mut child = Command::new(runtime_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start halo-runtime");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, r#"{{"type":"launch"}}"#).unwrap();
        writeln!(stdin, r#"{{"type":"shutdown"}}"#).unwrap();
    }

    let output = child.wait_with_output().expect("sidecar output");
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(output.status.success());
    assert!(stdout.contains(r#"{"type":"error","message":"unknown command: launch"}"#));
}

#[test]
fn sidecar_reports_duplicate_run_id_once() {
    let mut child = Command::new(runtime_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start halo-runtime");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(
            stdin,
            r#"{{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"hello"}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"again"}}"#
        )
        .unwrap();
        writeln!(stdin, r#"{{"type":"shutdown"}}"#).unwrap();
    }

    let output = child.wait_with_output().expect("sidecar output");
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(output.status.success());
    assert!(stdout.contains(r#"{"type":"error","message":"duplicate run id: run-1"}"#));
    assert_eq!(stdout.matches(r#"duplicate run id: run-1"#).count(), 1);
}

fn runtime_path() -> String {
    std::env::var("CARGO_BIN_EXE_halo-runtime").expect("halo-runtime binary path")
}
