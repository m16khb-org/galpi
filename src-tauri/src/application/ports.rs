use crate::application::error::AppError;
use crate::application::model::{
    CompletedTranscription, EnvironmentStatus, RecordingFailure, RecordingResult, RecordingStatus,
};
use crate::domain::artifact::Artifacts;
use crate::domain::chatgpt::{
    AgentHostId, AssistantTransport, ChatGptModel, ChatGptPreferences, ChatGptRegistration,
    ChatGptSettings, ChatGptSignInEvent, ChatGptTokens, RefreshFailure, RevocationOutcome,
    SignInGrant, SignInRequest,
};
use crate::domain::engine::EnginePreset;
use crate::domain::job::{SetupRequest, SpeakerHint};
use crate::domain::roster::{AssistantSettings, GlossaryEntry, Participant};
use crate::domain::worker::WorkerEvent;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::sync::oneshot;
use uuid::Uuid;

#[async_trait]
pub trait EnginePort: Send + Sync {
    async fn diagnose(&self, preset: EnginePreset) -> Result<EnvironmentStatus, AppError>;

    async fn prepare(
        &self,
        job_id: Uuid,
        cancel: &mut oneshot::Receiver<()>,
        request: &SetupRequest,
        preset: EnginePreset,
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
    /// The bearer credential: the saved API key, or a ChatGPT access token
    /// when `transport` is `Responses`.
    pub api_key: &'a str,
    pub transport: AssistantTransport,
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

/// The Sign in with ChatGPT authorization server and the account's model list.
#[async_trait]
pub trait ChatGptAuthPort: Send + Sync {
    /// Run the browser sign-in until the callback is exchanged, the user
    /// cancels, or it times out.
    async fn sign_in(
        &self,
        request: &SignInRequest,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<SignInGrant, AppError>;
    /// Exchange the refresh token; the result carries the rotated refresh token.
    async fn refresh(
        &self,
        registration: &ChatGptRegistration,
        refresh_token: &str,
    ) -> Result<ChatGptTokens, RefreshFailure>;
    /// Best-effort server-side revocation; never fails the caller.
    async fn revoke(
        &self,
        registration: &ChatGptRegistration,
        refresh_token: &str,
    ) -> RevocationOutcome;
    /// Listed models in server order.
    async fn list_models(&self, access_token: &str) -> Result<Vec<ChatGptModel>, AppError>;
}

/// Persistent ChatGPT account state. Only `load_tokens`, `save_session`,
/// `replace_tokens`, and the clears touch the secret store.
#[async_trait]
pub trait ChatGptStore: Send + Sync {
    async fn load_settings(&self) -> Result<ChatGptSettings, AppError>;
    async fn save_preferences(&self, preferences: ChatGptPreferences) -> Result<(), AppError>;
    async fn load_or_create_host_id(&self) -> Result<AgentHostId, AppError>;
    async fn load_registration(&self) -> Result<Option<ChatGptRegistration>, AppError>;
    async fn load_tokens(&self) -> Result<Option<ChatGptTokens>, AppError>;
    /// Store the registration, and the tokens when the grant allows plan use.
    async fn save_session(
        &self,
        registration: ChatGptRegistration,
        tokens: Option<ChatGptTokens>,
    ) -> Result<(), AppError>;
    async fn replace_tokens(&self, tokens: ChatGptTokens) -> Result<(), AppError>;
    /// Drop the tokens and keep the registration.
    async fn clear_tokens(&self) -> Result<(), AppError>;
    /// Drop registration, tokens, and model, return to API-key mode, keep the host id.
    async fn clear_session(&self) -> Result<(), AppError>;
}

pub trait ChatGptEvents: Send + Sync {
    fn emit(&self, event: ChatGptSignInEvent) -> Result<(), AppError>;
}

pub trait ClockPort: Send + Sync {
    fn now_unix(&self) -> u64;
}
