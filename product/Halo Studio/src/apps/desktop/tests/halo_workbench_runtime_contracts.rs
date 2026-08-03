use std::collections::BTreeSet;

use bitfun_agent_runtime::halo_workbench::{
    HaloWorkbenchAdapterSnapshot, HaloWorkbenchEvent, HaloWorkbenchEventKind, HaloWorkbenchPhase,
    HaloWorkbenchSnapshot,
};
use bitfun_desktop_lib::api::remote_workspace_policy::{
    remote_workspace_policy, RemoteWorkspacePolicy,
};

const SNAPSHOT_COMMAND: &str = "halo_workbench_runtime_snapshot";
const SUBMIT_COMMAND: &str = "halo_workbench_runtime_submit_intent";
const EVENT_NAME: &str = "halo-workbench://event";

const PI_COMMANDS: [(&str, &str); 8] = [
    ("halo_pi_credential_write", "halo_pi_credential_write"),
    ("halo_pi_credential_delete", "halo_pi_credential_delete"),
    (
        "halo_pi_configuration_snapshot",
        "halo_pi_configuration_snapshot",
    ),
    (
        "halo_pi_configuration_create",
        "halo_pi_configuration_create",
    ),
    (
        "halo_pi_configuration_update",
        "halo_pi_configuration_update",
    ),
    (
        "halo_pi_configuration_delete",
        "halo_pi_configuration_delete",
    ),
    (
        "halo_pi_configuration_rollback",
        "halo_pi_configuration_rollback",
    ),
    (
        "halo_pi_configuration_readiness",
        "halo_pi_configuration_readiness",
    ),
];

#[test]
fn tauri_exposes_two_commands_and_one_ordered_event_stream() {
    let app = include_str!("../src/lib.rs");
    let api_mod = include_str!("../src/api/mod.rs");
    let api = include_str!("../src/api/workbench_runtime_api.rs");

    assert_eq!(app.matches(SNAPSHOT_COMMAND).count(), 1);
    assert_eq!(app.matches(SUBMIT_COMMAND).count(), 1);
    let module_offset = api_mod
        .find("pub mod workbench_runtime_api;")
        .expect("Workbench Runtime API module");
    assert!(api_mod[module_offset.saturating_sub(80)..module_offset]
        .contains("#[cfg(feature = \"halo-local-coding\")]"));
    for command in [SNAPSHOT_COMMAND, SUBMIT_COMMAND] {
        let offset = app
            .find(command)
            .unwrap_or_else(|| panic!("missing command entry: {command}"));
        let prefix = &app[offset.saturating_sub(120)..offset];
        assert!(
            prefix.contains("#[cfg(feature = \"halo-local-coding\")]"),
            "{command} must only be registered for Halo local coding"
        );
    }
    assert!(api.contains(&format!(
        "pub const HALO_WORKBENCH_EVENT: &str = \"{EVENT_NAME}\""
    )));
    assert!(!api.contains("eventsource"));
    assert!(!api.contains("pi --mode rpc"));
    assert!(!api.contains("extension_ui_request"));
    assert!(!api.contains("Authorization"));
    assert!(!api.contains("acp stdio"));
}

#[test]
fn new_commands_have_explicit_remote_unsupported_policy() {
    for command in [SNAPSHOT_COMMAND, SUBMIT_COMMAND] {
        assert_eq!(
            remote_workspace_policy(command),
            Some(RemoteWorkspacePolicy::RemoteUnsupported),
            "{command} must explicitly reject remote workspaces"
        );
    }
}

#[test]
fn remote_workspace_policy_is_enforced_by_both_handlers() {
    let api = include_str!("../src/api/workbench_runtime_api.rs");

    assert!(api.contains("WorkspaceKind::Remote"));
    assert!(api.contains("remote_workspace_unsupported"));
    assert!(api.contains("switch_to_local_workspace"));
    assert!(api.contains("ensure_active_workspace_is_local(&state).await?"));
    assert!(api.contains("ensure_intent_workspace_is_local(&state, &request).await?"));
}

#[test]
fn workspace_cleanup_is_a_noop_for_remote_workspaces() {
    let runtime = include_str!("../src/runtime/mod.rs");
    assert!(runtime.contains("WorkspaceKind::Remote"));
    assert!(runtime.contains("get_current_workspace()"));
    assert!(runtime.contains("return Ok(())"));
}

#[test]
fn halo_build_excludes_legacy_execution_authorities() {
    let app = include_str!("../src/lib.rs");
    let gate = "#[cfg(not(feature = \"halo-local-coding\"))]";

    let handler = app
        .split_once(".invoke_handler(tauri::generate_handler![")
        .expect("Tauri invoke handler")
        .1;
    let mut previous_nonempty = "";
    for line in handler.lines().take_while(|line| !line.contains("]))")) {
        let trimmed = line.trim();
        if trimmed.contains("api::agentic_api::") {
            assert_eq!(
                previous_nonempty, gate,
                "legacy Agent Runtime command must be disabled for halo-local-coding: {trimmed}"
            );
        }
        if !trimmed.is_empty() {
            previous_nonempty = trimmed;
        }
    }

    for entry in [
        "api::agentic_api::create_session,",
        "api::agentic_api::start_dialog_turn,",
        "api::btw_api::btw_ask_stream,",
        "api::editor_ai_api::editor_ai_stream,",
        "api::miniapp_api::miniapp_ai_complete,",
        "api::miniapp_api::miniapp_ai_chat,",
        "api::miniapp_api::miniapp_ai_cancel,",
        "api::miniapp_api::miniapp_ai_list_models,",
        "initialize_acp_clients,",
        "create_acp_flow_session,",
        "start_acp_dialog_turn,",
        "api::miniapp_agent_api::miniapp_agent_run,",
        "test_ai_connection,",
        "test_ai_config_connection,",
        "list_ai_models_by_config,",
        "initialize_ai,",
        "refresh_model_client,",
        "get_model_configs,",
        "generate_commit_message,",
        "quick_commit_message,",
        "preview_commit_message,",
        "analyze_work_state,",
        "quick_analyze_work_state,",
        "generate_greeting_only,",
        "get_work_state_summary,",
        "api::insights_api::generate_insights,",
        "api::insights_api::cancel_insights_generation,",
    ] {
        let offset = app
            .find(entry)
            .unwrap_or_else(|| panic!("missing command entry: {entry}"));
        let prefix = &app[offset.saturating_sub(100)..offset];
        assert!(
            prefix.contains(gate),
            "{entry} must be compile-time disabled for halo-local-coding"
        );
    }

    for entry in [
        "init_mcp_servers(app_handle.clone());",
        "init_acp_clients(app_handle.clone());",
    ] {
        let offset = app
            .find(entry)
            .unwrap_or_else(|| panic!("missing native initialization entry: {entry}"));
        let prefix = &app[offset.saturating_sub(400)..offset];
        assert!(
            prefix.contains(gate),
            "{entry} must be skipped for halo-local-coding"
        );
    }
    assert!(app.contains("Skipped legacy MCP and ACP initialization"));

    for entry in [
        "init_agentic_system().await",
        "init_function_agents(state.4.clone()).await",
        "start_event_loop_with_transport(event_queue, event_router, transport);",
    ] {
        let offset = app
            .find(entry)
            .unwrap_or_else(|| panic!("missing legacy startup entry: {entry}"));
        let prefix = &app[offset.saturating_sub(1200)..offset];
        assert!(
            prefix.contains(gate),
            "{entry} must be skipped for halo-local-coding"
        );
    }

    let app_state = include_str!("../src/api/app_state.rs");
    for entry in [
        "MCPService::new(config_service.clone())",
        "bitfun_acp::AcpClientService::new(config_service.clone(), path_manager.clone())",
    ] {
        let offset = app_state
            .find(entry)
            .unwrap_or_else(|| panic!("missing legacy service initialization entry: {entry}"));
        let prefix = &app_state[offset.saturating_sub(240)..offset];
        assert!(
            prefix.contains(gate),
            "{entry} must be skipped for halo-local-coding"
        );
    }

    for entry in [
        "initialize_mcp_servers,",
        "api::mcp_api::initialize_mcp_servers_non_destructive,",
        "get_mcp_servers,",
        "api::mcp_api::start_mcp_remote_oauth,",
        "execute_tool,",
        "list_cron_jobs,",
        "list_persisted_sessions,",
    ] {
        let offset = app
            .find(entry)
            .unwrap_or_else(|| panic!("missing MCP command entry: {entry}"));
        let prefix = &app[offset.saturating_sub(120)..offset];
        assert!(
            prefix.contains(gate),
            "{entry} must be compile-time disabled for halo-local-coding"
        );
    }
}

#[test]
fn workspace_switch_and_exit_delegate_cleanup_before_host_teardown() {
    let commands = include_str!("../src/api/commands.rs");
    for (function_name, host_transition) in [
        (
            "pub async fn open_workspace",
            ".open_workspace(request.path.clone().into())",
        ),
        (
            "pub async fn close_workspace",
            ".close_workspace(&request.workspace_id)",
        ),
        (
            "pub async fn set_active_workspace",
            ".set_active_workspace(&request.workspace_id)",
        ),
    ] {
        let function = commands
            .split_once(function_name)
            .unwrap_or_else(|| panic!("missing workspace function: {function_name}"))
            .1;
        let runtime_close = function
            .find("close_halo_workbench_before_workspace_transition(&app).await?")
            .unwrap_or_else(|| panic!("{function_name} must fail closed on Runtime cleanup"));
        let host_transition = function
            .find(host_transition)
            .unwrap_or_else(|| panic!("missing host transition in {function_name}"));
        assert!(
            runtime_close < host_transition,
            "{function_name} must clean up Workbench Runtime before mutating host workspace state"
        );
    }

    let app = include_str!("../src/lib.rs");
    let exit = app
        .split_once("tauri::RunEvent::ExitRequested")
        .expect("Tauri exit hook")
        .1;
    let runtime_shutdown = exit
        .find("shutdown_halo_workbench_once")
        .expect("Workbench shutdown");
    let process_cleanup = exit
        .find("perform_process_exit_cleanup")
        .expect("process cleanup");
    assert!(runtime_shutdown < process_cleanup);
}

#[test]
fn snapshot_and_event_wire_shapes_are_camel_case_and_redacted() {
    let snapshot = HaloWorkbenchSnapshot {
        schema_version: 1,
        phase: HaloWorkbenchPhase::Disconnected,
        adapter: HaloWorkbenchAdapterSnapshot {
            identity: "pi-rpc-p0".to_string(),
            available: false,
            readiness: None,
        },
        workspace: None,
        sessions: Vec::new(),
        pending_operations: Vec::new(),
        last_sequence: 7,
        state_version: 3,
        error: None,
    };
    let snapshot_json = serde_json::to_value(snapshot).expect("snapshot serializes");
    let snapshot_keys = snapshot_json
        .as_object()
        .expect("snapshot is an object")
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        snapshot_keys,
        BTreeSet::from([
            "adapter".to_string(),
            "error".to_string(),
            "lastSequence".to_string(),
            "pendingOperations".to_string(),
            "phase".to_string(),
            "schemaVersion".to_string(),
            "sessions".to_string(),
            "stateVersion".to_string(),
            "workspace".to_string(),
        ])
    );

    let event = HaloWorkbenchEvent {
        sequence: 8,
        state_version: 4,
        correlation_id: Some("request-local".to_string()),
        kind: HaloWorkbenchEventKind::RuntimeStateChanged,
        summary: "Runtime is disconnected".to_string(),
        session_id: None,
        operation_id: None,
        occurred_at_ms: 42,
    };
    let event_json = serde_json::to_value(event).expect("event serializes");
    let serialized = event_json.to_string();
    for forbidden in [
        "http://127.0.0.1:4096",
        "Authorization",
        "Bearer secret",
        "externalSessionId",
        "externalMessageId",
        "nativePayload",
    ] {
        assert!(!serialized.contains(forbidden));
    }
    assert_eq!(event_json["sequence"], 8);
    assert_eq!(event_json["stateVersion"], 4);
    assert_eq!(event_json["correlationId"], "request-local");
    assert_eq!(event_json["occurredAtMs"], 42);
}

#[test]
fn desktop_runtime_context_stays_a_composition_host() {
    let runtime = include_str!("../src/runtime/mod.rs");

    assert!(runtime.contains("HaloWorkbenchRuntime"));
    assert!(!runtime.contains("struct HaloWorkbenchSession"));
    assert!(!runtime.contains("enum HaloWorkbenchPermission"));
    assert!(!runtime.contains("credential_value"));
    assert!(!runtime.contains("external_session_id"));
}

#[test]
fn pi_configuration_commands_are_registered_only_for_halo_local_coding() {
    let app = include_str!("../src/lib.rs");
    let api_mod = include_str!("../src/api/mod.rs");
    let api = include_str!("../src/api/pi_configuration_api.rs");
    let gate = "#[cfg(feature = \"halo-local-coding\")]";

    let module_offset = api_mod
        .find("pub mod pi_configuration_api;")
        .expect("Pi configuration API module");
    assert!(api_mod[module_offset.saturating_sub(100)..module_offset].contains(gate));

    for (command, function) in PI_COMMANDS {
        let registration = format!("api::pi_configuration_api::{function},");
        assert_eq!(
            app.matches(&registration).count(),
            1,
            "Pi command must have exactly one Tauri registration: {command}"
        );
        let offset = app
            .find(&registration)
            .unwrap_or_else(|| panic!("missing Pi command registration: {command}"));
        let prefix = &app[offset.saturating_sub(120)..offset];
        assert!(
            prefix.contains(gate),
            "{command} must only be registered for Halo local coding"
        );

        let command_marker = format!("pub async fn {function}");
        assert!(
            api.contains("#[tauri::command]"),
            "{command} must remain a Tauri command"
        );
        assert!(
            api.contains(&command_marker),
            "missing public command handler: {function}"
        );
    }
}

#[test]
fn pi_configuration_commands_are_local_only_and_have_no_remote_route() {
    for (command, _) in PI_COMMANDS {
        assert_eq!(
            remote_workspace_policy(command),
            Some(RemoteWorkspacePolicy::LocalOnly),
            "{command} must never be routed to a remote workspace"
        );
    }

    let policy = include_str!("../src/api/remote_workspace_policy.rs");
    for (command, _) in PI_COMMANDS {
        let offset = policy
            .find(&format!("\"{command}\""))
            .unwrap_or_else(|| panic!("missing remote policy entry: {command}"));
        let entry = &policy[offset..policy.len().min(offset + 120)];
        assert!(
            entry.contains("RemoteWorkspacePolicy::LocalOnly"),
            "{command} policy must be explicit and local-only"
        );
        assert!(!entry.contains("RemoteRouted"));
        assert!(!entry.contains("RemoteUnsupported"));
    }
}

#[test]
fn pi_credential_response_and_errors_are_stable_and_redacted() {
    let api = include_str!("../src/api/pi_configuration_api.rs");

    let credential_request_prefix = api
        .split_once("pub struct PiCredentialWriteRequest")
        .expect("credential request DTO")
        .0;
    let credential_request = api
        .split_once("pub struct PiCredentialWriteRequest")
        .expect("credential request DTO")
        .1
        .split_once("pub struct PiCredentialWriteResponse")
        .expect("credential response DTO")
        .0;
    assert!(credential_request_prefix.contains("#[derive(Deserialize)]"));
    assert!(!credential_request_prefix.contains("derive(Debug"));
    assert!(credential_request.contains("field(\"secret\", &\"<redacted>\")"));

    let credential_response_prefix = api
        .split_once("pub struct PiCredentialWriteResponse")
        .expect("credential response DTO")
        .0;
    let credential_response = api
        .split_once("pub struct PiCredentialWriteResponse")
        .expect("credential response DTO")
        .1
        .split_once("pub struct PiRuntimeConfigurationRequest")
        .expect("configuration request DTO")
        .0;
    assert!(credential_response_prefix.contains("#[derive(Debug, Serialize)]"));
    assert!(credential_response.contains("pub credential_ref: String"));
    for forbidden in ["secret", "authorization", "base_url", "auth.json"] {
        assert!(
            !credential_response.contains(forbidden),
            "credential response must not expose {forbidden}"
        );
    }

    let readiness_response = api
        .split_once("pub struct PiProviderReadinessResponse")
        .expect("readiness response DTO")
        .1
        .split_once("pub struct PiConfigurationCommandError")
        .expect("command error DTO")
        .0;
    assert!(readiness_response.contains("pub available: bool"));
    for forbidden in ["provider_id", "model_id", "base_url", "credential_ref"] {
        assert!(
            !readiness_response.contains(forbidden),
            "readiness response must not expose {forbidden}"
        );
    }

    let errors = api
        .split_once("pub struct PiConfigurationCommandError")
        .expect("command error DTO")
        .1
        .split_once("fn map_error")
        .expect("stable error mapper")
        .0;
    assert!(errors.contains("pub code: &'static str"));
    assert!(errors.contains("pub summary: &'static str"));
    assert!(errors.contains("pub recovery_action: &'static str"));
    for forbidden in ["PortError", "source", "details", "message", "secret"] {
        assert!(
            !errors.contains(forbidden),
            "public command error must not expose {forbidden}"
        );
    }

    let mapper = api
        .split_once("fn map_error")
        .expect("stable error mapper")
        .1;
    for stable_code in [
        "pi_configuration_invalid",
        "pi_configuration_missing",
        "pi_configuration_denied",
        "pi_configuration_store_unavailable",
        "pi_configuration_unavailable",
    ] {
        assert!(
            mapper.contains(stable_code),
            "missing stable error code: {stable_code}"
        );
    }
    assert!(!mapper.contains("error.to_string"));
    assert!(!mapper.contains("format!("));
    assert!(!api.contains("Authorization"));
    assert!(!api.contains("auth.json"));
    assert!(!api.contains("std::env"));
    assert!(!api.contains("println!"));
}

#[test]
fn pi_configuration_snapshot_is_renderer_safe() {
    let contracts = include_str!("../../../crates/contracts/runtime-ports/src/halo_workbench.rs");
    let view = contracts
        .split_once("pub struct PiRuntimeConfigurationView")
        .expect("renderer-safe Pi configuration view")
        .1
        .split_once("impl fmt::Debug for PiRuntimeConfiguration")
        .expect("Pi configuration debug boundary")
        .0;

    assert!(view.contains("pub base_url_hint: Option<String>"));
    assert!(view.contains("pub credential_ref: String"));
    assert!(!view.contains("pub base_url: Option<String>"));
    assert!(!view.contains("pub secret:"));
}
