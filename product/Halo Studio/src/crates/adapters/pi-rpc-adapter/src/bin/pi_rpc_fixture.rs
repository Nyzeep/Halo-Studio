use std::env;
use std::io::{self, BufRead, BufWriter, Write};
use std::time::Duration;

use serde_json::{json, Value};

fn main() {
    let arguments = env::args().collect::<Vec<_>>();
    let mode = env::var("HALO_PI_RPC_FIXTURE_MODE").unwrap_or_else(|_| "happy".to_string());
    let readiness_probe = arguments.iter().any(|argument| argument == "--no-session")
        && !arguments.iter().any(|argument| argument == "--extension");
    if arguments.iter().any(|argument| argument == "--version") {
        if matches!(
            mode.as_str(),
            "version_probe_requires_isolation" | "version_probe_failure"
        ) {
            let Some(config_dir) = env::var_os("PI_CODING_AGENT_DIR") else {
                std::process::exit(2);
            };
            if !std::path::Path::new(&config_dir).is_dir()
                || env::var_os("HALO_PI_CREDENTIAL").is_some()
            {
                std::process::exit(2);
            }
        }
        if mode == "version_probe_failure" {
            std::process::exit(1);
        }
        if mode == "version_probe_unknown" {
            println!("9.99.0");
            return;
        }
        if mode == "version_probe_malformed" {
            println!("pi-development-build");
            return;
        }
        if mode == "version_probe_0830" {
            println!("0.83.0");
            return;
        }
        println!("0.81.1");
        return;
    }

    if mode == "credential_projection"
        && !readiness_launch_is_valid(&arguments)
        && !controlled_launch_is_valid(&arguments)
    {
        return;
    }
    let stdin = io::stdin();
    let mut stdout = BufWriter::new(io::stdout().lock());
    let mut out_of_order_requests = Vec::new();
    let mut awaiting_since = false;
    let mut abort_requests = 0usize;

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
            "missing_get_entries_capability" if command == "get_entries" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    false,
                    Value::Null,
                );
                continue;
            }
            "missing_abort_capability" if command == "abort" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    false,
                    Value::Null,
                );
                continue;
            }
            "model_mismatch" if command == "get_available_models" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({
                        "models": [{ "provider": "anthropic", "id": "other-model" }]
                    }),
                );
                continue;
            }
            "thinking_mismatch" if command == "get_available_thinking_levels" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({ "levels": ["off"] }),
                );
                continue;
            }
            "require_since" | "bad_since"
                if awaiting_since
                    && (command != "get_entries"
                        || request.get("since").and_then(Value::as_str) != Some("entry-1")) =>
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
                    let data = if mode == "bad_since" {
                        json!({ "entries": [{ "id": "entry-1" }], "leafId": "entry-1" })
                    } else {
                        json!({ "entries": [], "leafId": "entry-1" })
                    };
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        data,
                    );
                } else {
                    awaiting_since = mode == "require_since";
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        json!({
                            "entries": [{ "id": "entry-1" }],
                            "leafId": "entry-1"
                        }),
                    );
                }
            }
            "get_available_models" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({
                        "models": [{ "provider": "openai", "id": "gpt-5" }]
                    }),
                );
            }
            "set_model" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    Value::Null,
                );
            }
            "get_available_thinking_levels" => {
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    json!({ "levels": ["off", "minimal", "low", "medium"] }),
                );
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
                "graceful_abort"
                | "hang_abort"
                | "hang_abort_response"
                | "agent_end_without_settled" => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_event(&mut stdout, json!({ "type": "agent_start" }));
                    if mode == "agent_end_without_settled" {
                        send_event(&mut stdout, json!({ "type": "agent_end" }));
                    }
                }
                "malformed_message_update" => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_event(&mut stdout, json!({ "type": "agent_start" }));
                    send_event(
                        &mut stdout,
                        json!({ "type": "message_update", "text": "invalid" }),
                    );
                }
                "unsupported_message_update" => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_event(&mut stdout, json!({ "type": "agent_start" }));
                    send_event(
                        &mut stdout,
                        json!({
                            "type": "message_update",
                            "message": {},
                            "assistantMessageEvent": { "type": "future_delta" }
                        }),
                    );
                }
                "sensitive_message_projection" | "sensitive_tool_projection" => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_event(&mut stdout, json!({ "type": "agent_start" }));
                    send_event(
                        &mut stdout,
                        json!({
                            "type": "message_update",
                            "message": {},
                            "assistantMessageEvent": {
                                "type": "text_delta",
                                "contentIndex": 0,
                                "delta": "Bearer raw-bearer-token Authorization: Basic basic-auth-canary Cookie: session=cookie-canary password=message-password-canary token=raw-token sk-live-canary {\"sessionId\":\"message-session-id-canary\",\"entryId\":\"message-entry-id-canary\",\"toolCallId\":\"message-tool-call-id-canary\"}\nsafe response"
                            }
                        }),
                    );
                    if mode == "sensitive_tool_projection" {
                        for event_type in [
                            "tool_execution_start",
                            "tool_execution_update",
                            "tool_execution_end",
                        ] {
                            let mut event = json!({
                                "type": event_type,
                                "toolCallId": "raw-sensitive-tool-call-id",
                                "toolName": "Authorization: Basic tool-basic-canary Cookie: tool-cookie-canary password=tool-password-canary sessionId=tool-session-id-canary entryId: tool-entry-id-canary toolCallId=tool-tool-call-id-canary"
                            });
                            if event_type == "tool_execution_end" {
                                event["isError"] = Value::Bool(false);
                            }
                            send_event(&mut stdout, event);
                        }
                    }
                    send_event(&mut stdout, json!({ "type": "agent_settled" }));
                }
                "malformed_tool_execution_end" => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_event(&mut stdout, json!({ "type": "agent_start" }));
                    send_event(
                        &mut stdout,
                        json!({
                            "type": "tool_execution_end",
                            "toolCallId": "raw-secret-tool-call-id",
                            "toolName": "write"
                        }),
                    );
                }
                "malformed_extension_ui_request" => {
                    respond(
                        &mut stdout,
                        request.get("id").and_then(Value::as_str),
                        command,
                        true,
                        Value::Null,
                    );
                    send_event(&mut stdout, json!({ "type": "agent_start" }));
                    send_event(
                        &mut stdout,
                        json!({
                            "type": "extension_ui_request",
                            "id": "ui-request-malformed",
                            "method": "confirm"
                        }),
                    );
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
                abort_requests += 1;
                if mode == "hang_abort_response" {
                    if abort_requests > 1 {
                        std::thread::sleep(Duration::from_secs(5));
                        return;
                    }
                }
                respond(
                    &mut stdout,
                    request.get("id").and_then(Value::as_str),
                    command,
                    true,
                    Value::Null,
                );
                if mode == "readiness_probe_eof" && readiness_probe {
                    return;
                }
                if mode != "hang_abort" && mode != "agent_end_without_settled" {
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

fn controlled_launch_is_valid(arguments: &[String]) -> bool {
    if arguments.iter().any(|argument| argument == "--api-key")
        || arguments
            .iter()
            .any(|argument| argument.contains("api.example.test"))
    {
        return false;
    }
    let model_is_projected = arguments
        .windows(2)
        .any(|pair| pair[0] == "--model" && pair[1] == "gpt-5:medium");
    let provider_is_projected = arguments
        .windows(2)
        .any(|pair| pair[0] == "--provider" && pair[1] == "openai");
    let config_dir = env::var_os("PI_CODING_AGENT_DIR").map(std::path::PathBuf::from);
    let Some(config_dir) = config_dir else {
        return false;
    };
    let Ok(models) = std::fs::read_to_string(config_dir.join("models.json")) else {
        return false;
    };
    let credential_is_injected =
        env::var("HALO_PI_CREDENTIAL").as_deref() == Ok("synthetic-credential-canary");
    let managed_session = arguments.iter().any(|argument| argument == "--no-session")
        && !arguments.iter().any(|argument| argument == "--session-dir");
    model_is_projected
        && provider_is_projected
        && credential_is_injected
        && managed_session
        && models.contains("$HALO_PI_CREDENTIAL")
        && models.contains("\"baseUrl\":\"https://api.example.test/v1\"")
        && !models.contains("synthetic-credential-canary")
        && !config_dir.join("auth.json").exists()
        && !config_dir.join("settings.json").exists()
}

fn readiness_launch_is_valid(arguments: &[String]) -> bool {
    let managed_session = arguments.iter().any(|argument| argument == "--no-session")
        && !arguments.iter().any(|argument| argument == "--session-dir");
    let extensions_are_disabled = arguments
        .iter()
        .any(|argument| argument == "--no-extensions")
        && !arguments.iter().any(|argument| argument == "--extension");
    let no_task_selection = !arguments.iter().any(|argument| argument == "--provider")
        && !arguments.iter().any(|argument| argument == "--model");
    let Some(config_dir) = env::var_os("PI_CODING_AGENT_DIR") else {
        return false;
    };
    managed_session
        && extensions_are_disabled
        && no_task_selection
        && env::var_os("HALO_PI_CREDENTIAL").is_none()
        && std::path::Path::new(&config_dir).is_dir()
        && !std::path::Path::new(&config_dir)
            .join("models.json")
            .exists()
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
            "message": {},
            "assistantMessageEvent": {
                "type": "text_delta",
                "contentIndex": 0,
                "delta": "unicode\u{2028}separator\u{2029}payload",
                "partial": {}
            }
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
