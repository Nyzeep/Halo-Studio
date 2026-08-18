use crate::util::errors::{HaloError, HaloResult};
use halo_runtime_ports::{
    RemoteExecPort, RemoteSendStdinRequest, TerminalPort, TerminalSendStdinRequest,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ExecCommandInputRequest {
    pub session_id: i32,
    pub chars: String,
    pub append_enter: bool,
    pub remote: bool,
}

pub async fn send_exec_command_input(
    request: ExecCommandInputRequest,
    terminal_port: Option<&Arc<dyn TerminalPort>>,
    remote_exec_port: Option<&Arc<dyn RemoteExecPort>>,
) -> HaloResult<()> {
    if request.remote {
        let remote_exec_port = remote_exec_port.ok_or_else(|| {
            HaloError::tool(
                "remote exec runtime service is required for ExecCommand input".to_string(),
            )
        })?;
        remote_exec_port
            .send_stdin(RemoteSendStdinRequest {
                session_id: request.session_id,
                chars: request.chars,
                append_enter: request.append_enter,
            })
            .await
            .map_err(|error| {
                HaloError::tool(format!("ExecCommand input failed: {}", error.message))
            })?;
        return Ok(());
    }

    let terminal_port = terminal_port.ok_or_else(|| {
        HaloError::tool("terminal runtime service is required for ExecCommand input".to_string())
    })?;
    terminal_port
        .send_stdin(TerminalSendStdinRequest {
            session_id: request.session_id,
            chars: request.chars,
            append_enter: request.append_enter,
        })
        .await
        .map_err(|error| HaloError::tool(format!("ExecCommand input failed: {}", error.message)))
}
