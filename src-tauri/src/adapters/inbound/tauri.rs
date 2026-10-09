use crate::application::error::AppError;
use crate::application::model::{
    EnvironmentStatus, RecordingFailure, RecordingResult, RecordingStatus, RefinementResult,
    SetupResult, TranscriptImportResult, TranscriptionResult,
};
use crate::application::ports::{ChatGptEvents, JobEvents, RecordingEvents};
use crate::application::use_cases::Application;
use crate::domain::artifact::ArtifactKind;
use crate::domain::chatgpt::{
    ChatGptModel, ChatGptPreferences, ChatGptSettings, ChatGptSignInEvent, SignOutResult,
};
use crate::domain::engine::{ComputeDevice, EnginePreset};
use crate::domain::job::{SetupRequest, TranscriptImportRequest, TranscriptionRequest};
use crate::domain::roster::AssistantSettings;
use crate::domain::worker::WorkerEvent;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

#[tauri::command]
pub async fn diagnose_environment(
    application: State<'_, Application>,
) -> Result<EnvironmentStatus, AppError> {
    application.diagnose().await
}

#[tauri::command]
pub async fn prepare_environment(
    application: State<'_, Application>,
    request: SetupRequest,
) -> Result<SetupResult, AppError> {
    application.prepare(request).await
}

/// Whether a token is saved. The value never leaves the host: the sheet shows a
/// mask either way, and reading it would put a keychain prompt on every open.
#[tauri::command]
pub async fn hugging_face_token_stored(
    application: State<'_, Application>,
) -> Result<bool, AppError> {
    application.hugging_face_token_stored().await
}

#[tauri::command]
pub async fn save_hugging_face_token(
    application: State<'_, Application>,
    token: String,
) -> Result<(), AppError> {
    application.save_hugging_face_token(token).await
}

#[tauri::command]
pub async fn load_assistant_settings(
    application: State<'_, Application>,
) -> Result<AssistantSettings, AppError> {
    application.load_assistant_settings().await
}

/// Store the assistant key on its own, so the settings autosave never carries
/// it and an untouched key is never rewritten. A blank string clears it.
#[tauri::command]
pub async fn save_assistant_api_key(
    application: State<'_, Application>,
    key: String,
) -> Result<(), AppError> {
    application.save_assistant_api_key(key).await
}

#[tauri::command]
pub async fn save_assistant_settings(
    application: State<'_, Application>,
    settings: AssistantSettings,
) -> Result<(), AppError> {
    application.save_assistant_settings(settings).await
}

#[tauri::command]
pub async fn save_engine_preset(
    application: State<'_, Application>,
    preset: EnginePreset,
) -> Result<(), AppError> {
    application.save_engine_preset(preset).await
}

#[tauri::command]
pub async fn save_compute_device(
    application: State<'_, Application>,
    device: ComputeDevice,
) -> Result<(), AppError> {
    application.save_compute_device(device).await
}

#[tauri::command]
pub async fn refine_transcript(
    application: State<'_, Application>,
    job_id: Uuid,
    target: Uuid,
    attendees: Vec<String>,
) -> Result<RefinementResult, AppError> {
    application
        .refine_transcript(job_id, target, &attendees)
        .await
}

#[tauri::command]
pub async fn start_transcription(
    application: State<'_, Application>,
    request: TranscriptionRequest,
) -> Result<TranscriptionResult, AppError> {
    application.transcribe(request).await
}

#[tauri::command]
pub async fn import_transcript(
    application: State<'_, Application>,
    request: TranscriptImportRequest,
) -> Result<TranscriptImportResult, AppError> {
    application.import_transcript(request).await
}

#[tauri::command]
pub async fn cancel_job(application: State<'_, Application>, job_id: Uuid) -> Result<(), AppError> {
    application.cancel(job_id)
}

#[tauri::command]
pub async fn open_artifact(
    application: State<'_, Application>,
    job_id: Uuid,
    kind: ArtifactKind,
) -> Result<(), AppError> {
    application.open_artifact(job_id, kind)
}

#[tauri::command]
pub async fn reveal_output_directory(
    application: State<'_, Application>,
    job_id: Uuid,
) -> Result<(), AppError> {
    application.reveal_output(job_id)
}

#[tauri::command]
pub async fn start_recording(
    application: State<'_, Application>,
    output_root: String,
) -> Result<RecordingStatus, AppError> {
    application.start_recording(output_root).await
}

#[tauri::command]
pub async fn stop_recording(
    application: State<'_, Application>,
    recording_id: Uuid,
) -> Result<RecordingResult, AppError> {
    application.stop_recording(recording_id).await
}

#[tauri::command]
pub async fn cancel_recording(
    application: State<'_, Application>,
    recording_id: Uuid,
) -> Result<(), AppError> {
    application.cancel_recording(recording_id).await
}

/// The sheet's ChatGPT view: mode, model, and who is signed in. It holds no
/// token, client id, or host id, so none of them can reach the window.
#[tauri::command]
pub async fn load_chatgpt_settings(
    application: State<'_, Application>,
) -> Result<ChatGptSettings, AppError> {
    application.load_chatgpt_settings().await
}

/// Autosave target for the window-owned choices. Never touches a secret.
#[tauri::command]
pub async fn save_chatgpt_preferences(
    application: State<'_, Application>,
    preferences: ChatGptPreferences,
) -> Result<(), AppError> {
    application.save_chatgpt_preferences(preferences).await
}

/// Opens the system browser and resolves once the sign-in completes, fails,
/// is cancelled, or times out.
#[tauri::command]
pub async fn sign_in_with_chatgpt(
    application: State<'_, Application>,
) -> Result<ChatGptSettings, AppError> {
    application.sign_in_with_chatgpt().await
}

#[tauri::command]
pub async fn cancel_chatgpt_sign_in(application: State<'_, Application>) -> Result<(), AppError> {
    application.cancel_chatgpt_sign_in()
}

#[tauri::command]
pub async fn list_chatgpt_models(
    application: State<'_, Application>,
) -> Result<Vec<ChatGptModel>, AppError> {
    application.list_chatgpt_models().await
}

#[tauri::command]
pub async fn sign_out_of_chatgpt(
    application: State<'_, Application>,
) -> Result<SignOutResult, AppError> {
    application.sign_out_of_chatgpt().await
}

#[derive(Debug, Clone)]
pub struct TauriEvents {
    app: AppHandle,
}

impl TauriEvents {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobEvent {
    job_id: Uuid,
    #[serde(flatten)]
    event: WorkerEvent,
}

impl JobEvents for TauriEvents {
    fn emit(&self, job_id: Uuid, event: WorkerEvent) -> Result<(), AppError> {
        self.app
            .emit("job-event", JobEvent { job_id, event })
            .map_err(|error| AppError::new("EVENT_ERROR", error.to_string()))
    }
}

impl RecordingEvents for TauriEvents {
    fn emit_failure(&self, failure: RecordingFailure) -> Result<(), AppError> {
        self.app
            .emit("recording-event", failure)
            .map_err(|error| AppError::new("EVENT_ERROR", error.to_string()))
    }
}

impl ChatGptEvents for TauriEvents {
    fn emit(&self, event: ChatGptSignInEvent) -> Result<(), AppError> {
        self.app
            .emit("chatgpt-event", event)
            .map_err(|error| AppError::new("EVENT_ERROR", error.to_string()))
    }
}
