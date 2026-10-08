use crate::application::chatgpt::ChatGptAccounts;
use crate::application::error::AppError;
use crate::application::jobs::JobRegistry;
use crate::application::model::{
    EnvironmentStatus, RefinementResult, SetupResult, TranscriptImportResult, TranscriptionResult,
};
use crate::application::model::{RecordingResult, RecordingStatus};
use crate::application::ports::{
    ArtifactPort, EnginePort, RecordingPort, RefinementJob, RefinementPort, SettingsPort,
    TranscriptImportPort, TranscriptionPort,
};
use crate::domain::artifact::{ArtifactKind, minutes_path};
use crate::domain::chatgpt::{
    AssistantAuthMode, AssistantTransport, ChatGptModel, ChatGptPreferences, ChatGptSettings,
    SignOutResult,
};
use crate::domain::engine::EnginePreset;
use crate::domain::job::{
    SetupRequest, TranscriptImportRequest, TranscriptionRequest, validate_speaker_hint,
};
use crate::domain::roster::{AssistantSettings, GlossaryEntry, Participant};
use crate::domain::worker::AsrContext;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// The credential and endpoint settings one refinement runs with.
struct RefinementCredential {
    api_key: String,
    model: Option<String>,
    base_url: Option<String>,
    reasoning_effort: Option<String>,
    transport: AssistantTransport,
}

pub struct Application {
    engine: Arc<dyn EnginePort>,
    transcription: Arc<dyn TranscriptionPort>,
    imports: Arc<dyn TranscriptImportPort>,
    artifacts: Arc<dyn ArtifactPort>,
    recording: Arc<dyn RecordingPort>,
    settings: Arc<dyn SettingsPort>,
    refinement: Arc<dyn RefinementPort>,
    chatgpt: ChatGptAccounts,
    jobs: JobRegistry,
    active_recording: tokio::sync::Mutex<Option<Uuid>>,
}

impl Application {
    // One port per outbound capability, so the list grows with the app.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine: Arc<dyn EnginePort>,
        transcription: Arc<dyn TranscriptionPort>,
        imports: Arc<dyn TranscriptImportPort>,
        artifacts: Arc<dyn ArtifactPort>,
        recording: Arc<dyn RecordingPort>,
        settings: Arc<dyn SettingsPort>,
        refinement: Arc<dyn RefinementPort>,
        chatgpt: ChatGptAccounts,
    ) -> Self {
        Self {
            engine,
            transcription,
            imports,
            artifacts,
            recording,
            settings,
            refinement,
            chatgpt,
            jobs: JobRegistry::default(),
            active_recording: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn diagnose(&self) -> Result<EnvironmentStatus, AppError> {
        let preset = self.settings.load_engine_preset().await?;
        self.engine.diagnose(preset).await
    }

    pub async fn prepare(&self, request: SetupRequest) -> Result<SetupResult, AppError> {
        let request = if request.hugging_face_token.is_some() {
            request
        } else {
            SetupRequest {
                job_id: request.job_id,
                hugging_face_token: self.settings.load_hugging_face_token().await?,
            }
        };
        let preset = self.settings.load_engine_preset().await?;
        let (job, mut cancel) = self.jobs.claim_with_id(request.job_id)?;
        let job_id = job.id();
        let result = self
            .engine
            .prepare(job_id, &mut cancel, &request, preset)
            .await;
        result.map(|status| SetupResult { job_id, status })
    }

    pub async fn save_engine_preset(&self, preset: EnginePreset) -> Result<(), AppError> {
        self.settings.save_engine_preset(preset).await
    }

    pub async fn hugging_face_token_stored(&self) -> Result<bool, AppError> {
        self.settings.hugging_face_token_stored().await
    }

    pub async fn save_hugging_face_token(&self, token: String) -> Result<(), AppError> {
        let token = token.trim();
        self.settings
            .save_hugging_face_token((!token.is_empty()).then(|| token.to_owned()))
            .await
    }

    pub async fn load_assistant_settings(&self) -> Result<AssistantSettings, AppError> {
        self.settings.load_assistant().await
    }

    /// Store the assistant key, or clear it when the text is blank.
    pub async fn save_assistant_api_key(&self, key: String) -> Result<(), AppError> {
        let key = key.trim();
        self.settings
            .save_assistant_api_key((!key.is_empty()).then(|| key.to_owned()))
            .await
    }

    pub async fn save_assistant_settings(
        &self,
        settings: AssistantSettings,
    ) -> Result<(), AppError> {
        self.settings.save_assistant(settings.trimmed()).await
    }

    /// Turn a completed transcript into meeting minutes with the configured assistant.
    ///
    /// `attendees` holds the roster ids selected for this meeting; an empty selection
    /// sends no participant context at all.
    pub async fn refine_transcript(
        &self,
        job_id: Uuid,
        target: Uuid,
        attendees: &[String],
    ) -> Result<RefinementResult, AppError> {
        let artifacts = self.jobs.artifacts(target)?;
        let assistant = self.settings.load_assistant().await?.trimmed();
        let participants: Vec<Participant> = assistant
            .participants
            .iter()
            .filter(|participant| attendees.contains(&participant.id))
            .cloned()
            .collect();
        let glossary: Vec<GlossaryEntry> = assistant.glossary.clone();
        // Resolved before the job slot is claimed, so a missing credential or
        // a failed token refresh never leaves the slot held.
        let credential = self.refinement_credential(&assistant).await?;
        let output = minutes_path(&artifacts.txt);
        let (job, mut cancel) = self.jobs.claim_with_id(job_id)?;
        let job_id = job.id();
        let result = self
            .refinement
            .refine(
                job_id,
                &mut cancel,
                RefinementJob {
                    transcript: &artifacts.txt,
                    output: &output,
                    source_audio: artifacts.source_audio.as_deref(),
                    background: assistant.background.as_deref(),
                    participants: &participants,
                    glossary: &glossary,
                    model: credential.model.as_deref(),
                    base_url: credential.base_url.as_deref(),
                    reasoning_effort: credential.reasoning_effort.as_deref(),
                    api_key: &credential.api_key,
                    transport: credential.transport,
                },
            )
            .await;
        let minutes = result?;
        self.jobs.register_minutes(target, minutes.clone())?;
        Ok(RefinementResult {
            job_id,
            minutes: minutes.to_string_lossy().into_owned(),
        })
    }

    /// Pick the credential and endpoint settings for the saved auth mode.
    async fn refinement_credential(
        &self,
        assistant: &AssistantSettings,
    ) -> Result<RefinementCredential, AppError> {
        match self.chatgpt.settings().await?.auth_mode {
            AssistantAuthMode::ApiKey => {
                // Read here rather than with the rest of the settings: this is the one
                // moment the key is actually needed, so it is the only moment worth
                // asking the keychain — and the user — about it.
                let api_key = self
                    .settings
                    .load_assistant_api_key()
                    .await?
                    .ok_or_else(|| {
                        AppError::new(
                            "ASSISTANT_KEY_MISSING",
                            "설정에서 z.ai 코딩 플랜 토큰을 먼저 저장해 주세요.",
                        )
                    })?;
                Ok(RefinementCredential {
                    api_key,
                    model: assistant.model.clone(),
                    base_url: assistant.base_url.clone(),
                    reasoning_effort: assistant.reasoning_effort.clone(),
                    transport: AssistantTransport::ChatCompletions,
                })
            }
            AssistantAuthMode::ChatGpt => {
                let access = self.chatgpt.access_for_refinement().await?;
                Ok(RefinementCredential {
                    api_key: access.token,
                    model: Some(access.model),
                    base_url: None,
                    reasoning_effort: None,
                    transport: AssistantTransport::Responses,
                })
            }
        }
    }

    pub async fn load_chatgpt_settings(&self) -> Result<ChatGptSettings, AppError> {
        self.chatgpt.settings().await
    }

    pub async fn save_chatgpt_preferences(
        &self,
        preferences: ChatGptPreferences,
    ) -> Result<(), AppError> {
        self.chatgpt.save_preferences(preferences).await
    }

    /// Resolves when the browser sign-in finishes, is cancelled, or times out.
    pub async fn sign_in_with_chatgpt(&self) -> Result<ChatGptSettings, AppError> {
        self.chatgpt.sign_in().await
    }

    pub fn cancel_chatgpt_sign_in(&self) -> Result<(), AppError> {
        self.chatgpt.cancel_sign_in()
    }

    pub async fn list_chatgpt_models(&self) -> Result<Vec<ChatGptModel>, AppError> {
        self.chatgpt.models().await
    }

    pub async fn sign_out_of_chatgpt(&self) -> Result<SignOutResult, AppError> {
        self.chatgpt.sign_out().await
    }

    pub async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, AppError> {
        validate_speaker_hint(&request.speaker_hint)
            .map_err(|error| AppError::new("INVALID_SPEAKER_HINT", error.to_string()))?;
        let (job, mut cancel) = self.jobs.claim_with_id(request.job_id)?;
        self.run_transcription(job.id(), &mut cancel, &request)
            .await
    }

    /// Register an existing transcript file as a finished meeting so it can be
    /// refined without recording or transcribing anything.
    pub async fn import_transcript(
        &self,
        request: TranscriptImportRequest,
    ) -> Result<TranscriptImportResult, AppError> {
        let (job, _unused_cancel) = self.jobs.claim_with_id(request.job_id)?;
        let job_id = job.id();
        let result = self
            .imports
            .import_transcript(
                Path::new(&request.input_path),
                Path::new(&request.output_root),
            )
            .await;
        let artifacts = result?;
        self.jobs.register(job_id, artifacts.clone())?;
        Ok(TranscriptImportResult {
            job_id,
            txt: artifacts.txt.to_string_lossy().into_owned(),
            output_directory: artifacts.output_directory.to_string_lossy().into_owned(),
        })
    }

    async fn run_transcription(
        &self,
        job_id: Uuid,
        cancel: &mut tokio::sync::oneshot::Receiver<()>,
        request: &TranscriptionRequest,
    ) -> Result<TranscriptionResult, AppError> {
        let engine = self.settings.load_engine_preset().await?;
        if !self.engine.diagnose(engine).await?.is_ready() {
            return Err(AppError::new(
                "SETUP_REQUIRED",
                "먼저 엔진과 모델 준비를 완료해 주세요.",
            ));
        }
        let (input, output) = self
            .transcription
            .prepare_job(
                Path::new(&request.input_path),
                Path::new(&request.output_root),
            )
            .await?;
        let result = self
            .transcription
            .transcribe(
                job_id,
                cancel,
                &input,
                &output,
                &request.speaker_hint,
                engine,
                self.asr_context().await?.as_deref(),
            )
            .await;
        let completed = result?;
        self.jobs.register(job_id, completed.artifacts.clone())?;
        Ok(TranscriptionResult {
            job_id,
            srt: artifact_path(completed.artifacts.srt.as_ref()),
            txt: completed.artifacts.txt.to_string_lossy().into_owned(),
            checkpoint: optional_artifact_path(completed.artifacts.checkpoint.as_ref()),
            output_directory: completed
                .artifacts
                .output_directory
                .to_string_lossy()
                .into_owned(),
            segments: completed.segments,
            filtered: completed.filtered,
        })
    }

    pub fn cancel(&self, id: Uuid) -> Result<(), AppError> {
        self.jobs.cancel(id)
    }

    /// Bias ASR recognition with the saved glossary and roster: terms first,
    /// then participant names and spoken aliases, packed by the worker.
    async fn asr_context(&self) -> Result<Option<String>, AppError> {
        let assistant = self.settings.load_assistant().await?.trimmed();
        let context = AsrContext::new(
            assistant.glossary.iter().map(|e| e.term.clone()).collect(),
            assistant
                .participants
                .iter()
                .map(|p| p.name.clone())
                .collect(),
            assistant
                .participants
                .iter()
                .flat_map(|p| p.aliases.clone())
                .collect(),
        );
        Ok(context.map(AsrContext::into_wire_json))
    }

    pub fn open_artifact(&self, id: Uuid, kind: ArtifactKind) -> Result<(), AppError> {
        let artifacts = self.jobs.artifacts(id)?;
        let path = artifacts.path_for(kind).ok_or_else(|| {
            AppError::new("ARTIFACT_NOT_FOUND", "아직 만들어진 회의록이 없습니다.")
        })?;
        self.artifacts.open_file(path, &artifacts.output_directory)
    }

    pub fn reveal_output(&self, id: Uuid) -> Result<(), AppError> {
        let artifacts = self.jobs.artifacts(id)?;
        self.artifacts.open_directory(&artifacts.output_directory)
    }

    pub async fn start_recording(&self, output_root: String) -> Result<RecordingStatus, AppError> {
        let mut active = self.active_recording.lock().await;
        if active.is_some() {
            return Err(AppError::new(
                "RECORDING_BUSY",
                "이미 마이크 녹음이 진행 중입니다.",
            ));
        }
        let recording_id = Uuid::now_v7();
        let result = self
            .recording
            .start(recording_id, Path::new(&output_root))
            .await;
        if result.is_ok() {
            *active = Some(recording_id);
        }
        result
    }

    pub async fn stop_recording(&self, recording_id: Uuid) -> Result<RecordingResult, AppError> {
        let mut active = self.active_recording.lock().await;
        verify_recording_id(*active, recording_id)?;
        let result = self.recording.stop(recording_id).await;
        *active = None;
        result
    }

    pub async fn cancel_recording(&self, recording_id: Uuid) -> Result<(), AppError> {
        let mut active = self.active_recording.lock().await;
        verify_recording_id(*active, recording_id)?;
        let result = self.recording.cancel(recording_id).await;
        *active = None;
        result
    }
}

fn verify_recording_id(active: Option<Uuid>, requested: Uuid) -> Result<(), AppError> {
    match active {
        Some(id) if id == requested => Ok(()),
        Some(_) => Err(AppError::new(
            "RECORDING_ID_MISMATCH",
            "다른 녹음 세션은 정지하거나 취소할 수 없습니다.",
        )),
        None => Err(AppError::new(
            "RECORDING_NOT_ACTIVE",
            "진행 중인 마이크 녹음이 없습니다.",
        )),
    }
}

/// Completed transcriptions always carry an srt, so its slot stays a plain
/// string; the checkpoint is optional and keeps its absence in the type.
fn artifact_path(path: Option<&std::path::PathBuf>) -> String {
    path.map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn optional_artifact_path(path: Option<&std::path::PathBuf>) -> Option<String> {
    path.map(|path| path.to_string_lossy().into_owned())
}
