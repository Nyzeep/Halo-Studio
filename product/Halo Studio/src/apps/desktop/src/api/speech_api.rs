//! Desktop adapter for local speech input.

use crate::api::AppState;
use halo_core_types::speech::{
    SpeechAppendAudioChunkRequest, SpeechAppendAudioChunkResponse, SpeechCancelInputSessionRequest,
    SpeechCancelModelDownloadRequest, SpeechDeleteModelRequest, SpeechDownloadModelRequest,
    SpeechFinishInputSessionRequest, SpeechInputSession, SpeechListModelsResponse,
    SpeechModelStatus, SpeechStartInputSessionRequest, SpeechTranscriptionResult,
    SpeechVerifyModelRequest,
};
#[cfg(feature = "speech")]
use halo_core_types::speech::SpeechModelProgressEvent;
#[cfg(feature = "speech")]
use halo_events::{SPEECH_MODEL_PROGRESS_EVENT, SPEECH_MODEL_STATUS_CHANGED_EVENT};
#[cfg(feature = "speech")]
use tauri::Emitter;
use tauri::{AppHandle, State};

#[cfg(not(feature = "speech"))]
const SPEECH_DISABLED: &str = "Speech input is disabled in Halo Studio local coding scope";

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_list_models(
    state: State<'_, AppState>,
) -> Result<SpeechListModelsResponse, String> {
    state
        .speech_service
        .list_models()
        .await
        .map_err(|error| format!("Failed to list speech models: {error}"))
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_list_models(
    _state: State<'_, AppState>,
) -> Result<SpeechListModelsResponse, String> {
    Ok(SpeechListModelsResponse { models: Vec::new() })
}

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_download_model(
    state: State<'_, AppState>,
    app: AppHandle,
    request: SpeechDownloadModelRequest,
) -> Result<SpeechModelStatus, String> {
    let progress_app = app.clone();
    let status = state
        .speech_service
        .download_model(request, move |event: SpeechModelProgressEvent| {
            if let Err(error) = progress_app.emit(SPEECH_MODEL_PROGRESS_EVENT, &event) {
                log::warn!("Failed to emit speech model progress event: {error}");
            }
        })
        .await
        .map_err(|error| format!("Failed to download speech model: {error}"))?;
    emit_status(&app, &status);
    Ok(status)
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_download_model(
    _state: State<'_, AppState>,
    _app: AppHandle,
    _request: SpeechDownloadModelRequest,
) -> Result<SpeechModelStatus, String> {
    Err(SPEECH_DISABLED.to_string())
}

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_cancel_model_download(
    state: State<'_, AppState>,
    app: AppHandle,
    request: SpeechCancelModelDownloadRequest,
) -> Result<SpeechModelStatus, String> {
    let status = state
        .speech_service
        .cancel_model_download(request)
        .await
        .map_err(|error| format!("Failed to cancel speech model download: {error}"))?;
    emit_status(&app, &status);
    Ok(status)
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_cancel_model_download(
    _state: State<'_, AppState>,
    _app: AppHandle,
    _request: SpeechCancelModelDownloadRequest,
) -> Result<SpeechModelStatus, String> {
    Err(SPEECH_DISABLED.to_string())
}

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_delete_model(
    state: State<'_, AppState>,
    app: AppHandle,
    request: SpeechDeleteModelRequest,
) -> Result<SpeechModelStatus, String> {
    let status = state
        .speech_service
        .delete_model(request)
        .await
        .map_err(|error| format!("Failed to delete speech model: {error}"))?;
    emit_status(&app, &status);
    Ok(status)
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_delete_model(
    _state: State<'_, AppState>,
    _app: AppHandle,
    _request: SpeechDeleteModelRequest,
) -> Result<SpeechModelStatus, String> {
    Err(SPEECH_DISABLED.to_string())
}

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_verify_model(
    state: State<'_, AppState>,
    app: AppHandle,
    request: SpeechVerifyModelRequest,
) -> Result<SpeechModelStatus, String> {
    let status = state
        .speech_service
        .verify_model(request)
        .await
        .map_err(|error| format!("Failed to verify speech model: {error}"))?;
    emit_status(&app, &status);
    Ok(status)
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_verify_model(
    _state: State<'_, AppState>,
    _app: AppHandle,
    _request: SpeechVerifyModelRequest,
) -> Result<SpeechModelStatus, String> {
    Err(SPEECH_DISABLED.to_string())
}

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_start_input_session(
    state: State<'_, AppState>,
    request: SpeechStartInputSessionRequest,
) -> Result<SpeechInputSession, String> {
    state
        .speech_service
        .start_input_session(request)
        .await
        .map_err(|error| format!("Failed to start speech input session: {error}"))
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_start_input_session(
    _state: State<'_, AppState>,
    _request: SpeechStartInputSessionRequest,
) -> Result<SpeechInputSession, String> {
    Err(SPEECH_DISABLED.to_string())
}

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_append_audio_chunk(
    state: State<'_, AppState>,
    request: SpeechAppendAudioChunkRequest,
) -> Result<SpeechAppendAudioChunkResponse, String> {
    state
        .speech_service
        .append_audio_chunk(request)
        .await
        .map_err(|error| format!("Failed to append speech audio chunk: {error}"))
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_append_audio_chunk(
    _state: State<'_, AppState>,
    _request: SpeechAppendAudioChunkRequest,
) -> Result<SpeechAppendAudioChunkResponse, String> {
    Err(SPEECH_DISABLED.to_string())
}

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_finish_input_session(
    state: State<'_, AppState>,
    request: SpeechFinishInputSessionRequest,
) -> Result<SpeechTranscriptionResult, String> {
    state
        .speech_service
        .finish_input_session(request)
        .await
        .map_err(|error| format!("Failed to transcribe speech input: {error}"))
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_finish_input_session(
    _state: State<'_, AppState>,
    _request: SpeechFinishInputSessionRequest,
) -> Result<SpeechTranscriptionResult, String> {
    Err(SPEECH_DISABLED.to_string())
}

#[tauri::command]
#[cfg(feature = "speech")]
pub async fn speech_cancel_input_session(
    state: State<'_, AppState>,
    request: SpeechCancelInputSessionRequest,
) -> Result<(), String> {
    state
        .speech_service
        .cancel_input_session(request)
        .await
        .map_err(|error| format!("Failed to cancel speech input session: {error}"))
}

#[tauri::command]
#[cfg(not(feature = "speech"))]
pub async fn speech_cancel_input_session(
    _state: State<'_, AppState>,
    _request: SpeechCancelInputSessionRequest,
) -> Result<(), String> {
    Err(SPEECH_DISABLED.to_string())
}

#[cfg(feature = "speech")]
fn emit_status(app: &AppHandle, status: &SpeechModelStatus) {
    if let Err(error) = app.emit(SPEECH_MODEL_STATUS_CHANGED_EVENT, status) {
        log::warn!("Failed to emit speech model status event: {error}");
    }
}
