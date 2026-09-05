//! Contract-test stand-in for `dsh --profile acp|sdk`.
//!
//! Speaks the same LF-delimited JSON-RPC wire the adapter consumes, driven by
//! `HALO_DSH_FIXTURE_MODE`. Launch-validation modes fail with exit code 2 so
//! a wrong controlled launch can never pass a handshake.

use std::env;
use std::io::{self, BufRead, BufWriter, Write};
use std::time::Duration;

use serde_json::{json, Value};

fn main() {
    let mode = env::var("HALO_DSH_FIXTURE_MODE").unwrap_or_else(|_| "happy".to_string());
    let channel = env::var("HALO_DSH_FIXTURE_CHANNEL").unwrap_or_else(|_| "acp".to_string());
    let stdin = io::stdin();
    let mut stdout = BufWriter::new(io::stdout().lock());

    // Pending prompt request id, kept open across frames for the
    // permission and cancel flows.
    let mut pending_prompt: Option<Value> = None;
    let mut cancelled_prompt = false;

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else { return };
        let id = message.get("id").cloned();
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);

        match (method, id) {
            (Some(method), Some(id)) => {
                match method.as_str() {
                    "initialize" => {
                        if mode == "env_check" && !launch_is_valid() {
                            return_code(2);
                        }
                        if channel == "sdk" {
                            respond(
                                &mut stdout,
                                &id,
                                json!({
                                    "serverInfo": {
                                        "name": "deepseek-harness-sdk-runtime",
                                        "version": "0.1.3-alpha.1"
                                    }
                                }),
                            );
                        } else if mode == "unsupported_agent" {
                            respond(
                                &mut stdout,
                                &id,
                                json!({
                                    "protocolVersion": 1,
                                    "agentInfo": { "name": "future-harness-agent", "version": "0.0.1" },
                                    "authMethods": []
                                }),
                            );
                        } else if mode == "wrong_protocol" {
                            respond(
                                &mut stdout,
                                &id,
                                json!({
                                    "protocolVersion": 2,
                                    "agentInfo": { "name": "deepseek-harness-acp", "version": "0.0.1" },
                                    "authMethods": []
                                }),
                            );
                        } else {
                            respond(
                                &mut stdout,
                                &id,
                                json!({
                                    "protocolVersion": 1,
                                    "agentInfo": { "name": "deepseek-harness-acp", "version": "0.0.1" },
                                    "authMethods": [],
                                    "sessionCapabilities": { "close": true, "list": true, "resume": true },
                                    "promptCapabilities": { "image": false, "audio": false }
                                }),
                            );
                        }
                    }
                    "session/new" => {
                        if mode == "env_check" {
                            // session/new must carry the same cwd the child
                            // actually runs in.
                            let cwd_matches = message
                                .pointer("/params/cwd")
                                .and_then(Value::as_str)
                                .map(|cwd| {
                                    env::current_dir()
                                        .map(|current| current == std::path::PathBuf::from(cwd))
                                        .unwrap_or(false)
                                })
                                .unwrap_or(false);
                            if !cwd_matches {
                                return_code(2);
                            }
                        }
                        respond(
                            &mut stdout,
                            &id,
                            json!({
                                "sessionId": "native-fixture-session",
                                "configOptions": []
                            }),
                        );
                    }
                    "session/prompt" => match mode.as_str() {
                        "eof_after_initialize" => return,
                        "hang_prompt" => {
                            // Never settles, never exits: forces the reclaim
                            // ladder through force termination.
                            pending_prompt = Some(id);
                        }
                        "cancel" => {
                            send_update(
                                &mut stdout,
                                json!({
                                    "sessionUpdate": "agent_message_chunk",
                                    "content": { "type": "text", "text": "before cancel" }
                                }),
                            );
                            pending_prompt = Some(id);
                        }
                        "permission" => {
                            // One-shot machine decision: the adapter must
                            // answer before the prompt can settle.
                            request(
                                &mut stdout,
                                "perm-1",
                                "session/request_permission",
                                json!({
                                    "sessionId": "native-fixture-session",
                                    "toolCall": {
                                        "toolCallId": "raw-permission-tool-call",
                                        "title": "write",
                                        "rawInput": { "path": "notes.txt" }
                                    },
                                    "options": [
                                        { "optionId": "opt-allow", "name": "Allow", "kind": "allow_once" },
                                        { "optionId": "opt-reject", "name": "Reject", "kind": "reject_once" }
                                    ]
                                }),
                            );
                            pending_prompt = Some(id);
                        }
                        _ if channel == "sdk" => {
                            respond(&mut stdout, &id, json!({ "messageId": "m-1" }));
                            send_notification(
                                &mut stdout,
                                "session/event",
                                json!({
                                    "sessionId": "sdk-fixture-session",
                                    "event": {
                                        "type": "assistant/message",
                                        "content": [ { "type": "text", "text": "canary reply" } ]
                                    }
                                }),
                            );
                            send_notification(
                                &mut stdout,
                                "session/event",
                                json!({
                                    "sessionId": "sdk-fixture-session",
                                    "event": {
                                        "type": "tool/call",
                                        "toolCallId": "raw-sdk-tool-call",
                                        "name": "write"
                                    }
                                }),
                            );
                            send_notification(
                                &mut stdout,
                                "session/event",
                                json!({
                                    "sessionId": "sdk-fixture-session",
                                    "event": {
                                        "type": "tool/result",
                                        "toolCallId": "raw-sdk-tool-call",
                                        "isError": false
                                    }
                                }),
                            );
                            send_notification(
                                &mut stdout,
                                "session/status",
                                json!({ "sessionId": "sdk-fixture-session", "status": "idle" }),
                            );
                        }
                        _ => {
                            send_update(
                                &mut stdout,
                                json!({
                                    "sessionUpdate": "agent_message_chunk",
                                    "content": { "type": "text", "text": "fixture reply" }
                                }),
                            );
                            send_update(
                                &mut stdout,
                                json!({
                                    "sessionUpdate": "tool_call",
                                    "toolCallId": "raw-fixture-tool-call",
                                    "title": "write",
                                    "kind": "other",
                                    "status": "in_progress",
                                    "rawInput": { "path": "notes.txt" }
                                }),
                            );
                            send_update(
                                &mut stdout,
                                json!({
                                    "sessionUpdate": "tool_call_update",
                                    "toolCallId": "raw-fixture-tool-call",
                                    "status": "completed"
                                }),
                            );
                            if mode == "unknown_update" {
                                send_update(
                                    &mut stdout,
                                    json!({ "sessionUpdate": "future_update_kind", "detail": {} }),
                                );
                            }
                            respond(&mut stdout, &id, json!({ "stopReason": "end_turn" }));
                        }
                    },
                    _ => {
                        // Unknown client request: refuse without breaking the
                        // session, matching the upstream -32601 path.
                        respond(&mut stdout, &id, Value::Null);
                    }
                }
            }
            (None, Some(id)) => {
                // Client response to a server-to-client request.
                if id == "perm-1" {
                    let outcome = message.pointer("/result/outcome");
                    let reply = match outcome.and_then(|outcome| outcome.get("outcome")).and_then(Value::as_str) {
                        Some("cancelled") => "no decision",
                        _ => {
                            let option = outcome
                                .and_then(|outcome| outcome.get("optionId"))
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if option == "opt-allow" {
                                "approved reply"
                            } else if option == "opt-reject" {
                                "rejected reply"
                            } else {
                                "no decision"
                            }
                        }
                    };
                    if let Some(prompt_id) = pending_prompt.take() {
                        send_update(
                            &mut stdout,
                            json!({
                                "sessionUpdate": "agent_message_chunk",
                                "content": { "type": "text", "text": reply }
                            }),
                        );
                        respond(&mut stdout, &prompt_id, json!({ "stopReason": "end_turn" }));
                    }
                }
            }
            (Some(method), None) => {
                if (method == "session/cancel" || method == "$/cancelRequest")
                    && cancelled_prompt
                {
                    continue;
                }
                if method == "session/cancel" || method == "$/cancelRequest" {
                    cancelled_prompt = true;
                    if mode == "hang_prompt" {
                        continue;
                    }
                    if let Some(prompt_id) = pending_prompt.take() {
                        respond(&mut stdout, &prompt_id, json!({ "stopReason": "cancelled" }));
                    }
                }
                // Unknown notifications are filtered, never fatal.
            }
            (None, None) => {}
        }
    }

    if mode == "hang_prompt" {
        // The forced-reclaim contract needs a non-cooperative fake child:
        // closing stdin must not make this fixture exit on its own.
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
    write_sentinel();
}

/// Launch validation for the credential/env contract: the controlled launch
/// carries exactly `--profile acp`, a managed `DSH_HOME`, the credential only
/// in the environment, and never a `.env` file or argv credential.
fn launch_is_valid() -> bool {
    let arguments: Vec<String> = env::args().collect();
    let argv_clean = arguments.len() == 3
        && arguments[1] == "--profile"
        && arguments[2] == "acp"
        && !arguments.iter().any(|argument| argument.contains("synthetic-dsh-credential-canary"));
    let Some(dsh_home) = env::var_os("DSH_HOME").map(std::path::PathBuf::from) else {
        return false;
    };
    let home_is_managed = dsh_home.is_dir();
    let credential_injected =
        env::var("DEEPSEEK_API_KEY").as_deref() == Ok("synthetic-dsh-credential-canary");
    let no_env_file_injection = !dsh_home.join(".env").exists()
        && env::current_dir()
            .ok()
            .map(|cwd| !cwd.join(".env").exists())
            .unwrap_or(false);
    let cwd_matches = env::var_os("HALO_DSH_EXPECT_CWD")
        .map(|expected| {
            env::current_dir()
                .map(|cwd| cwd == std::path::PathBuf::from(&expected))
                .unwrap_or(false)
        })
        .unwrap_or(true);
    argv_clean && home_is_managed && credential_injected && no_env_file_injection && cwd_matches
}

fn respond(stdout: &mut BufWriter<io::StdoutLock<'_>>, id: &Value, result: Value) {
    write_json(
        stdout,
        json!({ "jsonrpc": "2.0", "id": id.clone(), "result": result }),
    );
}

fn request(
    stdout: &mut BufWriter<io::StdoutLock<'_>>,
    id: &str,
    method: &str,
    params: Value,
) {
    write_json(
        stdout,
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
    );
}

fn send_notification(
    stdout: &mut BufWriter<io::StdoutLock<'_>>,
    method: &str,
    params: Value,
) {
    write_json(stdout, json!({ "jsonrpc": "2.0", "method": method, "params": params }));
}

fn send_update(stdout: &mut BufWriter<io::StdoutLock<'_>>, update: Value) {
    send_notification(stdout, "session/update", json!({ "update": update }));
}

fn write_json(stdout: &mut BufWriter<io::StdoutLock<'_>>, value: Value) {
    let mut encoded = serde_json::to_vec(&value).expect("fixture JSON serializes");
    encoded.push(b'\n');
    stdout
        .write_all(&encoded)
        .expect("fixture stdout is available");
    stdout.flush().expect("fixture stdout flushes");
}

/// Publishes the graceful-exit marker inside the managed DSH_HOME: reached
/// only when stdin EOF ends the fixture naturally (exit 0), never after a
/// forced reclaim.
fn write_sentinel() {
    if let Some(home) = env::var_os("DSH_HOME") {
        let _ = std::fs::write(
            std::path::PathBuf::from(home).join(".halo-fixture-exit-marker"),
            "exit-0",
        );
    }
}

fn return_code(code: i32) -> ! {
    std::process::exit(code)
}
