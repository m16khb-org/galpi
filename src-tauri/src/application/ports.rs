use crate::application::error::AppError;
use crate::application::model::{
    CompletedTranscription, EnvironmentStatus, RecordingFailure, RecordingResult, RecordingStatus,
};
use crate::domain::artifact::Artifacts;
use crate::domain::engine::{ComputeDevice, EnginePreset, EngineSelection};
use crate::domain::job::{SetupRequest, SpeakerHint};
use crate::domain::roster::{AssistantSettings, GlossaryEntry, Participant};
use crate::domain::worker::WorkerEvent;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::sync::oneshot;
use uuid::Uuid;

#[async_trait]
pub trait EnginePort: Send + Sync {
    async fn diagnose(&self, selection: EngineSelection) -> Result<EnvironmentStatus, AppError>;

    async fn prepare(
        &self,
        job_id: Uuid,
        cancel: &mut oneshot::Receiver<()>,
        request: &SetupRequest,
        selection: EngineSelection,
    ) -> Result<EnvironmentStatus, AppError>;
}

// The transcribe signature mirrors the worker call; allow carries through
// the async_trait expansion only on the trait itself.
#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait TranscriptionPort: Send + Sync {
    async fn prepare_job(
        &self,
        input: &Path,
        output_root: &Path,
    ) -> Result<(PathBuf, PathBuf), AppError>;

    async fn transcribe(
        &self,
        job_id: Uuid,
        cancel: &mut oneshot::Receiver<()>,
        input: &Path,
        output: &Path,
        hint: &SpeakerHint,
        engine: EnginePreset,
        asr_context: Option<&str>,
    ) -> Result<CompletedTranscription, AppError>;
}

pub trait ArtifactPort: Send + Sync {
    fn open_file(&self, path: &Path, trusted_root: &Path) -> Result<(), AppError>;
    fn open_directory(&self, path: &Path) -> Result<(), AppError>;
}

/// Copies an existing transcript into a meeting folder without transcribing.
#[async_trait]
pub trait TranscriptImportPort: Send + Sync {
    async fn import_transcript(
        &self,
        input: &Path,
        output_root: &Path,
    ) -> Result<Artifacts, AppError>;
}

pub trait JobEvents: Send + Sync {
    fn emit(&self, job_id: Uuid, event: WorkerEvent) -> Result<(), AppError>;
}

#[async_trait]
pub trait RecordingPort: Send + Sync {
    async fn start(
        &self,
        recording_id: Uuid,
        output_root: &Path,
    ) -> Result<RecordingStatus, AppError>;
    async fn stop(&self, recording_id: Uuid) -> Result<RecordingResult, AppError>;
    async fn cancel(&self, recording_id: Uuid) -> Result<(), AppError>;
}

pub trait RecordingEvents: Send + Sync {
    fn emit_failure(&self, failure: RecordingFailure) -> Result<(), AppError>;
}

#[async_trait]
pub trait SettingsPort: Send + Sync {
    /// Whether a Hugging Face token is on file, without reading it.
    async fn hugging_face_token_stored(&self) -> Result<bool, AppError>;
    async fn load_hugging_face_token(&self) -> Result<Option<String>, AppError>;
    async fn save_hugging_face_token(&self, token: Option<String>) -> Result<(), AppError>;
    /// Everything except the API key, which stays in the keychain until a
    /// refinement actually needs it.
    async fn load_assistant(&self) -> Result<AssistantSettings, AppError>;
    async fn load_assistant_api_key(&self) -> Result<Option<String>, AppError>;
    /// Store or clear the assistant key, which `save_assistant` never touches.
    async fn save_assistant_api_key(&self, key: Option<String>) -> Result<(), AppError>;
    async fn save_assistant(&self, settings: AssistantSettings) -> Result<(), AppError>;
    async fn load_engine_preset(&self) -> Result<EnginePreset, AppError>;
    async fn save_engine_preset(&self, preset: EnginePreset) -> Result<(), AppError>;
    async fn load_compute_device(&self) -> Result<ComputeDevice, AppError>;
    async fn save_compute_device(&self, device: ComputeDevice) -> Result<(), AppError>;
}

/// One transcript refinement request handed to the assistant worker.
pub struct RefinementJob<'a> {
    pub transcript: &'a Path,
    pub output: &'a Path,
    /// The recorded audio, when this meeting came from a recording rather than
    /// an imported transcript. Its timestamp dates the minutes.
    pub source_audio: Option<&'a Path>,
    pub background: Option<&'a str>,
    pub participants: &'a [Participant],
    pub glossary: &'a [GlossaryEntry],
    pub model: Option<&'a str>,
    pub base_url: Option<&'a str>,
    pub reasoning_effort: Option<&'a str>,
    pub api_key: &'a str,
}

#[async_trait]
pub trait RefinementPort: Send + Sync {
    async fn refine(
        &self,
        job_id: Uuid,
        cancel: &mut oneshot::Receiver<()>,
        job: RefinementJob<'_>,
    ) -> Result<PathBuf, AppError>;
}
