use crate::application::chatgpt::ChatGptAccounts;
use crate::application::drive::DriveAccounts;
use crate::application::error::AppError;
use crate::application::jobs::JobRegistry;
use crate::application::model::{
    EnvironmentStatus, RefinementResult, SetupResult, TranscriptImportResult, TranscriptionResult,
};
use crate::application::model::{RecordingResult, RecordingStatus};
use crate::application::ports::{
    ArtifactPort, DrivePickerKind, EnginePort, RecordingPort, RefinementJob, RefinementPort,
    SettingsPort, TranscriptImportPort, TranscriptionPort,
};
use crate::domain::artifact::{ArtifactKind, Artifacts, minutes_path};
use crate::domain::chatgpt::{
    AssistantAuthMode, AssistantTransport, ChatGptModel, ChatGptPreferences, ChatGptSettings,
    SignOutResult,
};
use crate::domain::drive::{
    DriveClientStatus, DriveFileId, DriveImport, DriveMeeting, DriveRecordingUpload, DriveStatus,
    DriveSyncResult, MeetingKey,
};
use crate::domain::engine::{ComputeDevice, EnginePreset, EngineSelection};
use crate::domain::job::{
    SetupRequest, TranscriptImportRequest, TranscriptionRequest, validate_speaker_hint,
};
use crate::domain::recording::RecordingSource;
use crate::domain::roster::{AssistantSettings, GlossaryEntry, Participant};
use crate::domain::worker::AsrContext;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
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
    drive: DriveAccounts,
    jobs: JobRegistry,
    active_recording: tokio::sync::Mutex<Option<Uuid>>,
    /// Where each finished meeting goes on Drive, by its transcription or
    /// transcript-import job id. Refinement registers its minutes under that
    /// same id, so a sync after refinement finds them here too.
    drive_meetings: Mutex<HashMap<Uuid, DriveMeeting>>,
    /// Every recording downloaded from Drive, so a transcription that claims
    /// a Drive source is checked against what was actually imported.
    drive_imports: Mutex<HashMap<PathBuf, DriveFileId>>,
    /// The canonical file of every recording this run finished, by recording
    /// id, so an upload names a recording instead of a path.
    finished_recordings: Mutex<HashMap<Uuid, PathBuf>>,
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
        drive: DriveAccounts,
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
            drive,
            jobs: JobRegistry::default(),
            active_recording: tokio::sync::Mutex::new(None),
            drive_meetings: Mutex::new(HashMap::new()),
            drive_imports: Mutex::new(HashMap::new()),
            finished_recordings: Mutex::new(HashMap::new()),
        }
    }

    pub async fn diagnose(&self) -> Result<EnvironmentStatus, AppError> {
        let selection = self.engine_selection().await?;
        self.engine.diagnose(selection).await
    }

    async fn engine_selection(&self) -> Result<EngineSelection, AppError> {
        Ok(EngineSelection {
            preset: self.settings.load_engine_preset().await?,
            device: self.settings.load_compute_device().await?,
        })
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
        let selection = self.engine_selection().await?;
        let (job, mut cancel) = self.jobs.claim_with_id(request.job_id)?;
        let job_id = job.id();
        let result = self
            .engine
            .prepare(job_id, &mut cancel, &request, selection)
            .await;
        result.map(|status| SetupResult { job_id, status })
    }

    pub async fn save_engine_preset(&self, preset: EnginePreset) -> Result<(), AppError> {
        self.settings.save_engine_preset(preset).await
    }

    pub async fn save_compute_device(&self, device: ComputeDevice) -> Result<(), AppError> {
        self.settings.save_compute_device(device).await
    }

    pub async fn load_recording_source(&self) -> Result<RecordingSource, AppError> {
        self.settings.load_recording_source().await
    }

    pub async fn save_recording_source(&self, source: RecordingSource) -> Result<(), AppError> {
        self.settings.save_recording_source(source).await
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
                            "설정의 AI 증강에서 API 키를 먼저 저장해 주세요.",
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
                    reasoning_effort: access.reasoning_effort,
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

    pub async fn load_drive_status(&self) -> Result<DriveStatus, AppError> {
        self.drive.status().await
    }

    pub async fn dismiss_drive_onboarding(&self) -> Result<DriveStatus, AppError> {
        self.drive.dismiss_onboarding().await
    }

    /// Choose whether finished recordings upload to Drive on their own.
    pub async fn set_drive_save_recordings(&self, enabled: bool) -> Result<DriveStatus, AppError> {
        self.drive.set_save_recordings(enabled).await
    }

    /// Which Google client Drive uses, if one is saved.
    pub async fn load_drive_client(&self) -> Result<DriveClientStatus, AppError> {
        self.drive.client_status().await
    }

    /// Save the person's Google client. Refused while Drive is connected.
    pub async fn save_drive_client(
        &self,
        client_id: &str,
        client_secret: &str,
    ) -> Result<DriveClientStatus, AppError> {
        self.drive.save_client(client_id, client_secret).await
    }

    /// Remove the saved Google client. Refused while Drive is connected.
    pub async fn clear_drive_client(&self) -> Result<DriveClientStatus, AppError> {
        self.drive.clear_client().await
    }

    /// Upload a recording this run finished, named by its recording id. The
    /// window never supplies a path: only a file the recorder reported can go.
    pub async fn upload_recording_to_drive(
        &self,
        recording_id: Uuid,
    ) -> Result<DriveRecordingUpload, AppError> {
        let path = self
            .finished_recordings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&recording_id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    "RECORDING_NOT_FOUND",
                    "Drive에 올릴 녹음을 찾지 못했습니다.",
                )
            })?;
        // The same key a later transcription of this file registers, so its
        // results land in the folder that holds the recording.
        let meeting = DriveMeeting::new(MeetingKey::local(&path), &path);
        self.drive.upload_recording(&meeting, &path).await
    }

    /// Resolves when the browser consent finishes, is cancelled, or times out.
    pub async fn connect_drive(&self) -> Result<DriveStatus, AppError> {
        self.drive.connect().await
    }

    pub fn cancel_drive_operation(&self) -> Result<(), AppError> {
        self.drive.cancel()
    }

    pub async fn disconnect_drive(&self) -> Result<DriveStatus, AppError> {
        self.drive.disconnect().await
    }

    /// Pick one recording in the desktop Picker and download it as job `job_id`,
    /// so the job's cancel stops both the consent wait and the download.
    pub async fn import_from_drive(&self, job_id: Uuid) -> Result<DriveImport, AppError> {
        let (job, mut cancel) = self.jobs.claim_with_id(job_id)?;
        let job_id = job.id();
        let imported = self
            .drive
            .import(DrivePickerKind::Recording, job_id, &mut cancel)
            .await?;
        self.drive_imports
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(imported.path.clone(), imported.id.clone());
        Ok(DriveImport {
            job_id,
            input_path: imported.path.to_string_lossy().into_owned(),
            drive_file_id: imported.id,
            name: imported.name,
        })
    }

    /// Upload or update a finished meeting's results on Drive. `job_id` is the
    /// transcription or transcript-import job; refinement adds its minutes there.
    pub async fn sync_meeting_to_drive(
        &self,
        job_id: Uuid,
        overwrite: bool,
    ) -> Result<DriveSyncResult, AppError> {
        let artifacts = self.jobs.artifacts(job_id)?;
        let meeting = self
            .drive_meetings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&job_id)
            .cloned()
            .ok_or_else(|| AppError::new("ARTIFACT_NOT_FOUND", "완료된 결과를 찾지 못했습니다."))?;
        self.drive.sync(&artifacts, &meeting, overwrite).await
    }

    fn remember_meeting(&self, job_id: Uuid, key: MeetingKey, source: &Path) {
        self.drive_meetings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(job_id, DriveMeeting::new(key, source));
    }

    /// The Drive id a transcription claims, accepted only for the very file
    /// that id was imported to.
    fn imported_drive_source(&self, input_path: &str, id: &str) -> Result<DriveFileId, AppError> {
        let mismatch = || {
            AppError::new(
                "DRIVE_SOURCE_MISMATCH",
                "Drive에서 가져온 녹음 정보가 맞지 않습니다. 다시 가져와 주세요.",
            )
        };
        let id = DriveFileId::parse(id).map_err(|_| mismatch())?;
        let imports = self
            .drive_imports
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if imports.get(Path::new(input_path)) == Some(&id) {
            Ok(id)
        } else {
            Err(mismatch())
        }
    }

    pub async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, AppError> {
        validate_speaker_hint(&request.speaker_hint)
            .map_err(|error| AppError::new("INVALID_SPEAKER_HINT", error.to_string()))?;
        let drive_source = request
            .drive_source_id
            .as_deref()
            .map(|id| self.imported_drive_source(&request.input_path, id))
            .transpose()?;
        let (job, mut cancel) = self.jobs.claim_with_id(request.job_id)?;
        let job_id = job.id();
        let result = self
            .run_transcription(job_id, &mut cancel, &request)
            .await?;
        if let Some(source) = self.jobs.artifacts(job_id)?.source_audio {
            let key = drive_source
                .as_ref()
                .map_or_else(|| MeetingKey::local(&source), MeetingKey::drive);
            self.remember_meeting(job_id, key, &source);
        }
        Ok(result)
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
        self.register_transcript(job_id, result?, None)
    }

    /// Pick one transcript in the desktop Picker, download it as job `job_id`,
    /// and register it exactly like [`Self::import_transcript`], so the job's
    /// cancel stops both the consent wait and the download.
    ///
    /// A file Galpi uploaded keeps its meeting, so refined minutes sync into
    /// the Drive folder it came from; any other file is its own meeting.
    pub async fn import_transcript_from_drive(
        &self,
        job_id: Uuid,
        output_root: &str,
    ) -> Result<TranscriptImportResult, AppError> {
        let (job, mut cancel) = self.jobs.claim_with_id(job_id)?;
        let job_id = job.id();
        let picked = self
            .drive
            .import(DrivePickerKind::Transcript, job_id, &mut cancel)
            .await?;
        let artifacts = self
            .imports
            .import_transcript(&picked.path, Path::new(output_root))
            .await?;
        let key = picked
            .meeting
            .unwrap_or_else(|| MeetingKey::drive(&picked.id));
        self.register_transcript(job_id, artifacts, Some(key))
    }

    /// Remember an imported transcript as a finished meeting. Without a
    /// Drive `key` the meeting is identified by the transcript's source path.
    fn register_transcript(
        &self,
        job_id: Uuid,
        artifacts: Artifacts,
        key: Option<MeetingKey>,
    ) -> Result<TranscriptImportResult, AppError> {
        let result = TranscriptImportResult {
            job_id,
            txt: artifacts.txt.to_string_lossy().into_owned(),
            output_directory: artifacts.output_directory.to_string_lossy().into_owned(),
        };
        let source = artifacts.source_transcript.clone();
        self.jobs.register(job_id, artifacts)?;
        if let Some(source) = source {
            let key = key.unwrap_or_else(|| MeetingKey::local(&source));
            self.remember_meeting(job_id, key, &source);
        }
        Ok(result)
    }

    async fn run_transcription(
        &self,
        job_id: Uuid,
        cancel: &mut tokio::sync::oneshot::Receiver<()>,
        request: &TranscriptionRequest,
    ) -> Result<TranscriptionResult, AppError> {
        let selection = self.engine_selection().await?;
        let engine = selection.preset;
        if !self.engine.diagnose(selection).await?.is_ready() {
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

    pub async fn start_recording(
        &self,
        output_root: String,
        source: RecordingSource,
    ) -> Result<RecordingStatus, AppError> {
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
            .start(recording_id, Path::new(&output_root), source)
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
        if let Ok(finished) = &result {
            self.finished_recordings
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(recording_id, PathBuf::from(&finished.path));
        }
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
