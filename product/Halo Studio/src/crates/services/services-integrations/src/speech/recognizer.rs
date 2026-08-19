use super::types::{SpeechRecognizerKind, SpeechTranscribeRequest, SpeechTranscriptionResult};
use super::HaloResult;
use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(super) struct SpeechRecognizerWarmupRequest {
    pub model_dir: PathBuf,
    pub recognizer: SpeechRecognizerKind,
    pub language: String,
}

#[async_trait]
pub(super) trait SpeechRecognizer: Send + Sync {
    async fn warmup(&self, request: SpeechRecognizerWarmupRequest) -> HaloResult<()>;

    async fn unload(&self) -> HaloResult<()>;

    async fn transcribe(
        &self,
        request: SpeechTranscribeRequest,
    ) -> HaloResult<SpeechTranscriptionResult>;
}
