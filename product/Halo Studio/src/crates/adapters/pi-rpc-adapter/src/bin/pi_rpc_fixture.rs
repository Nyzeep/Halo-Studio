use std::env;
use std::io::{self, BufRead, BufWriter, Write};
use std::time::Duration;

use serde_json::{json, Value};

fn main() {
    let arguments = env::args().collect::<Vec<_>>();
    if arguments.iter().any(|argument| argument == "--version") {
        println!("0.81.1");
        return;
    }

    let mode = env::var("HALO_PI_RPC_FIXTURE_MODE").unwrap_or_else(|_| "happy".to_string());
    let stdin = io::stdin();
    let mut stdout = BufWriter::new(io::stdout().lock());
    let mut out_of_order_requests = Vec::new();
    let mut awaiting_since = false;

    for line in stdin.lock().lines() {
        let Ok(line) = line else { return };
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            return;
        };
        let Some(command) = request.get("type").and_then(Value::as_str) else {
            return;
        };

        match mode.as_str() {
            "eof" => return,
            "bad_json" => {
                write_raw(&mut stdout, b"not-json\n");
                return;
            }
            "partial_eof" => {
                write_raw(&mut stdout, br#"{"type":"response"}"#);
                return;
            }
            "unknown_response" if command == "get_state" => {
                respond(
                    &mut stdout,
                    Some("unknown-response"),
                    command,
                    true,
                    json!({}),
                );
                return;
            }
            "not_ready" if command == "get_state" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({ "isStreaming": true, "isCompacting": false }),
                );
                continue;
            }
            "env_canary" if command == "get_state" => {
                let leaked = env::var_os("HALO_PI_RPC_SECRET_CANARY").is_some();
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({
                        "isStreaming": leaked,
                        "isCompacting": false
                    }),
                );
                continue;
            }
            "unknown_event" if command == "get_state" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({ "isStreaming": false, "isCompacting": false }),
                );
                send_event(&mut stdout, json!({ "type": "future_pi_event" }));
                continue;
            }
            "bad_entries" if command == "get_entries" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({ "entries": [] }),
                );
                continue;
            }
            "require_since"
                if awaiting_since
                    && (command != "get_entries"
                        || request.get("since").and_then(Value::as_str) != Some("cursor-1")) =>
            {
                write_raw(&mut stdout, b"not-json\n");
                return;
            }
            _ => {}
        }

        match command {
            "get_state" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({ "isStreaming": false, "isCompacting": false }),
                );
            }
            "get_entries" => {
                if request.get("since").is_some() {
                    awaiting_since = false;
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        json!({ "entries": [], "leafId": Value::Null }),
                    );
                    if mode == "ready_then_eof" {
                        std::thread::sleep(Duration::from_millis(500));
                        return;
                    }
                } else {
                    awaiting_since = mode == "require_since";
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        json!({
                            "entries": [{ "id": "entry-1" }],
                            "leafId": "cursor-1"
                        }),
                    );
                }
            }
            "prompt" | "follow_up" => match mode.as_str() {
                "out_of_order" => {
                    out_of_order_requests.push(request);
                    if out_of_order_requests.len() == 2 {
                        let has_prompt = out_of_order_requests.iter().any(|request| {
                            request.get("type").and_then(Value::as_str) == Some("prompt")
                        });
                        let has_follow_up = out_of_order_requests.iter().any(|request| {
                            request.get("type").and_then(Value::as_str) == Some("follow_up")
                        });
                        if !has_prompt || !has_follow_up {
                            write_raw(&mut stdout, b"not-json\n");
                            return;
                        }
                        for request in out_of_order_requests.drain(..).rev() {
                            respond(
                                &mut stdout,
                                request.get("id").and_then(Value::as_str),
                                request
                                    .get("type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("prompt"),
                                true,
                                Value::Null,
                            );
                        }
                    }
                }
                "graceful_abort" | "hang_abort" | "hang_abort_response" => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_event(&mut stdout, json!({ "type": "agent_start" }));
                }
                "extension" | "extension_duplicate" | "extension_timeout" | "extension_error" => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_event(&mut stdout, json!({ "type": "agent_start" }));
                    if mode == "extension_error" {
                        send_event(&mut stdout, json!({ "type": "extension_error" }));
                    } else {
                        send_permission_request(&mut stdout);
                    }
                }
                _ => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_happy_events(&mut stdout);
                }
            },
            "abort" => {
                if mode == "hang_abort_response" {
                    std::thread::sleep(Duration::from_secs(5));
                    return;
                }
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    Value::Null,
                );
                if mode != "hang_abort" {
                    send_event(&mut stdout, json!({ "type": "agent_settled" }));
                }
            }
            "extension_ui_response" => {
                if mode == "extension_duplicate" {
                    send_permission_request(&mut stdout);
                } else if mode == "extension" || mode == "extension_timeout" {
                    send_event(&mut stdout, json!({ "type": "agent_settled" }));
                }
            }
            _ => {}
        }
    }
}

fn respond(
    stdout: &mut BufWriter<io::StdoutLock<'_>>,
    id: Option<&str>,
    command: &str,
    success: bool,
    data: Value,
) {
    let mut response = json!({
        "type": "response",
        "command": command,
        "success": success,
        "data": data,
    });
    if let Some(id) = id {
        response["id"] = Value::String(id.to_string());
    }
    if env::var("HALO_PI_RPC_FIXTURE_MODE").as_deref() == Ok("cr") {
        write_json(stdout, &response, b"\r\n");
    } else if env::var("HALO_PI_RPC_FIXTURE_MODE").as_deref() == Ok("idless") {
        response.as_object_mut().map(|object| object.remove("id"));
        write_json(stdout, &response, b"\n");
    } else {
        write_json(stdout, &response, b"\n");
    }
}

fn send_happy_events(stdout: &mut BufWriter<io::StdoutLock<'_>>) {
    send_event(stdout, json!({ "type": "agent_start" }));
    send_event(
        stdout,
        json!({
            "type": "message_update",
            "text": "unicode\u{2028}separator\u{2029}payload"
        }),
    );
    send_event(
        stdout,
        json!({
            "type": "tool_execution_start",
            "toolCallId": "raw-secret-tool-call-id",
            "toolName": "write"
        }),
    );
    send_event(
        stdout,
        json!({
            "type": "tool_execution_update",
            "toolCallId": "raw-secret-tool-call-id",
            "toolName": "write"
        }),
    );
    send_event(
        stdout,
        json!({
            "type": "tool_execution_end",
            "toolCallId": "raw-secret-tool-call-id",
            "toolName": "write",
            "isError": false
        }),
    );
    send_event(stdout, json!({ "type": "agent_end" }));
    send_event(stdout, json!({ "type": "agent_settled" }));
}

fn send_permission_request(stdout: &mut BufWriter<io::StdoutLock<'_>>) {
    let message = serde_json::to_string(&json!({
        "toolCallId": "raw-secret-permission-id",
        "toolName": "write"
    }))
    .expect("fixture permission notice serializes");
    send_event(
        stdout,
        json!({
            "type": "extension_ui_request",
            "id": "ui-request-1",
            "method": "confirm",
            "message": message
        }),
    );
}

fn send_event(stdout: &mut BufWriter<io::StdoutLock<'_>>, event: Value) {
    write_json(stdout, &event, b"\n");
}

fn write_json(stdout: &mut BufWriter<io::StdoutLock<'_>>, value: &Value, suffix: &[u8]) {
    let mut encoded = serde_json::to_vec(value).expect("fixture JSON serializes");
    encoded.extend_from_slice(suffix);
    write_raw(stdout, &encoded);
}

fn write_raw(stdout: &mut BufWriter<io::StdoutLock<'_>>, bytes: &[u8]) {
    stdout
        .write_all(bytes)
        .expect("fixture stdout is available");
    stdout.flush().expect("fixture stdout flushes");
}
