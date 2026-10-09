use super::chatgpt::ChatGptAccounts;
use super::error::AppError;
use super::gateway::AppAccessGate;
use super::model::{CompletedTranscription, EnvironmentStatus};
use super::ports::{
    ArtifactPort, ChatGptAuthPort, ChatGptStore, ClockPort, EnginePort, GatewayAuthPort,
    GatewaySessionStore, RecordingPort, RefinementJob, RefinementPort, SettingsPort,
    TranscriptImportPort, TranscriptionPort,
};
use super::use_cases::Application;
use crate::domain::artifact::{ArtifactKind, Artifacts};
use crate::domain::chatgpt::AssistantTransport;
use crate::domain::engine::{ComputeDevice, EnginePreset, EngineSelection};
use crate::domain::gateway::AppAccess;
use crate::domain::job::{
    SetupRequest, SpeakerHint, TranscriptImportRequest, TranscriptionRequest,
};
use crate::domain::roster::{AssistantSettings, GlossaryEntry, Participant};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

enum TranscriptionBehavior {
    Success,
    Failure,
    Blocking(Mutex<Option<mpsc::UnboundedSender<Uuid>>>),
}

struct SeenRefinement {
    transcript: PathBuf,
    api_key: String,
    model: Option<String>,
    base_url: Option<String>,
    reasoning_effort: Option<String>,
    background: Option<String>,
    participants: Vec<String>,
    transport: AssistantTransport,
    glossary: Vec<String>,
}

fn participant(id: &str, name: &str) -> Participant {
    Participant {
        id: id.to_owned(),
        name: name.to_owned(),
        team: None,
        role: None,
        description: None,
        aliases: Vec::new(),
    }
}

fn glossary_entry(id: &str, term: &str) -> GlossaryEntry {
    GlossaryEntry {
        id: id.to_owned(),
        term: term.to_owned(),
        description: None,
    }
}

struct FakePort {
    prepare_calls: AtomicUsize,
    opened_paths: Mutex<Vec<PathBuf>>,
    prepared_token: Mutex<Option<String>>,
    settings_token: Mutex<Option<String>>,
    assistant_key: Mutex<Option<String>>,
    assistant: Mutex<AssistantSettings>,
    refinements: Mutex<Vec<SeenRefinement>>,
    asr_contexts: Mutex<Vec<String>>,
    engine_preset: Mutex<EnginePreset>,
    compute_device: Mutex<ComputeDevice>,
    seen_selections: Mutex<Vec<EngineSelection>>,
    seen_engines: Mutex<Vec<EnginePreset>>,
    seen_prepare_engines: Mutex<Vec<EnginePreset>>,
    ready_presets: Mutex<Vec<EnginePreset>>,
    behavior: TranscriptionBehavior,
    chatgpt: chatgpt::ChatGptFake,
    gateway: gateway::GatewayFake,
}

impl FakePort {
    fn new(behavior: TranscriptionBehavior) -> Self {
        Self {
            prepare_calls: AtomicUsize::new(0),
            opened_paths: Mutex::new(Vec::new()),
            prepared_token: Mutex::new(None),
            settings_token: Mutex::new(None),
            assistant_key: Mutex::new(None),
            assistant: Mutex::new(AssistantSettings::default()),
            refinements: Mutex::new(Vec::new()),
            asr_contexts: Mutex::new(Vec::new()),
            engine_preset: Mutex::new(EnginePreset::default()),
            compute_device: Mutex::new(ComputeDevice::default()),
            seen_selections: Mutex::new(Vec::new()),
            seen_engines: Mutex::new(Vec::new()),
            seen_prepare_engines: Mutex::new(Vec::new()),
            ready_presets: Mutex::new(vec![EnginePreset::Qwen3, EnginePreset::WhisperX]),
            chatgpt: chatgpt::ChatGptFake::new(),
            gateway: gateway::GatewayFake::default(),
            behavior,
        }
    }

    /// An application someone has already signed in to.
    fn application(self: &Arc<Self>) -> Application {
        self.application_with(AppAccess::SignedIn {
            email: None,
            offline: false,
        })
    }

    /// An application as it starts: signed out until `load_app_access`.
    fn signed_out_application(self: &Arc<Self>) -> Application {
        self.application_with(AppAccess::SignedOut)
    }

    fn application_with(self: &Arc<Self>, access: AppAccess) -> Application {
        let engine: Arc<dyn EnginePort> = self.clone();
        let transcription: Arc<dyn TranscriptionPort> = self.clone();
        let imports: Arc<dyn TranscriptImportPort> = self.clone();
        let artifacts: Arc<dyn ArtifactPort> = self.clone();
        let recording: Arc<dyn RecordingPort> = self.clone();
        let settings: Arc<dyn SettingsPort> = self.clone();
        let refinement: Arc<dyn RefinementPort> = self.clone();
        let chatgpt_auth: Arc<dyn ChatGptAuthPort> = self.clone();
        let chatgpt_store: Arc<dyn ChatGptStore> = self.clone();
        let clock: Arc<dyn ClockPort> = self.clone();
        let gateway_auth: Arc<dyn GatewayAuthPort> = self.clone();
        let gateway_store: Arc<dyn GatewaySessionStore> = self.clone();
        let gate = AppAccessGate::new(gateway_auth, gateway_store);
        if let Ok(mut opened) = gate.access.lock() {
            *opened = access;
        }
        Application::new(
            engine,
            transcription,
            imports,
            artifacts,
            recording,
            settings,
            refinement,
            ChatGptAccounts::new(chatgpt_auth, chatgpt_store, clock),
            gate,
        )
    }
}

#[async_trait]
impl RefinementPort for FakePort {
    async fn refine(
        &self,
        _job_id: Uuid,
        _cancel: &mut oneshot::Receiver<()>,
        job: RefinementJob<'_>,
    ) -> Result<PathBuf, AppError> {
        self.refinements
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "refinement lock poisoned"))?
            .push(SeenRefinement {
                transcript: job.transcript.to_owned(),
                api_key: job.api_key.to_owned(),
                transport: job.transport,
                model: job.model.map(str::to_owned),
                base_url: job.base_url.map(str::to_owned),
                reasoning_effort: job.reasoning_effort.map(str::to_owned),
                background: job.background.map(str::to_owned),
                participants: job
                    .participants
                    .iter()
                    .map(|participant| participant.name.clone())
                    .collect(),
                glossary: job
                    .glossary
                    .iter()
                    .map(|entry| entry.term.clone())
                    .collect(),
            });
        self.chatgpt.refine_outcome()?;
        Ok(job.output.to_owned())
    }
}

#[async_trait]
impl EnginePort for FakePort {
    async fn diagnose(&self, selection: EngineSelection) -> Result<EnvironmentStatus, AppError> {
        let preset = selection.preset;
        self.seen_selections
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "selections lock poisoned"))?
            .push(selection);
        let ready = self
            .ready_presets
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "ready presets lock poisoned"))?
            .contains(&preset);
        Ok(EnvironmentStatus {
            engine_preset: preset,
            engine_ready: ready,
            models_ready: ready,
            ffmpeg_ready: ready,
            qwen3_ready: ready && preset == EnginePreset::Qwen3,
            whisperx_ready: ready && preset == EnginePreset::WhisperX,
            data_directory: "/tmp/galpi-test".to_owned(),
            default_output_directory: "/tmp/output".to_owned(),
            engine_version: "test".to_owned(),
            compute_device: selection.device,
            available_presets: vec![EnginePreset::Qwen3, EnginePreset::WhisperX],
            available_devices: Vec::new(),
            cuda_driver_detected: false,
        })
    }

    async fn prepare(
        &self,
        job_id: Uuid,
        _cancel: &mut oneshot::Receiver<()>,
        request: &SetupRequest,
        selection: EngineSelection,
    ) -> Result<EnvironmentStatus, AppError> {
        let _ = job_id;
        let preset = selection.preset;
        *self
            .prepared_token
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "prepared token lock poisoned"))? =
            request.hugging_face_token.clone();
        *self
            .seen_prepare_engines
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "prepare engines lock poisoned"))? =
            vec![preset];
        self.diagnose(selection).await
    }
}

#[async_trait]
impl SettingsPort for FakePort {
    async fn hugging_face_token_stored(&self) -> Result<bool, AppError> {
        Ok(self.load_hugging_face_token().await?.is_some())
    }

    async fn load_assistant_api_key(&self) -> Result<Option<String>, AppError> {
        self.assistant_key
            .lock()
            .map(|key| key.clone())
            .map_err(|_| AppError::new("TEST_ERROR", "assistant key lock poisoned"))
    }

    async fn save_assistant_api_key(&self, key: Option<String>) -> Result<(), AppError> {
        *self
            .assistant_key
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "assistant key lock poisoned"))? = key;
        Ok(())
    }

    async fn load_hugging_face_token(&self) -> Result<Option<String>, AppError> {
        self.settings_token
            .lock()
            .map(|token| token.clone())
            .map_err(|_| AppError::new("TEST_ERROR", "settings token lock poisoned"))
    }

    async fn save_hugging_face_token(&self, token: Option<String>) -> Result<(), AppError> {
        *self
            .settings_token
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "settings token lock poisoned"))? = token;
        Ok(())
    }

    async fn load_assistant(&self) -> Result<AssistantSettings, AppError> {
        self.assistant
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| AppError::new("TEST_ERROR", "assistant settings lock poisoned"))
    }

    async fn save_assistant(&self, settings: AssistantSettings) -> Result<(), AppError> {
        *self
            .assistant
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "assistant settings lock poisoned"))? =
            settings;
        Ok(())
    }

    async fn load_engine_preset(&self) -> Result<EnginePreset, AppError> {
        self.engine_preset
            .lock()
            .map(|preset| *preset)
            .map_err(|_| AppError::new("TEST_ERROR", "engine preset lock poisoned"))
    }

    async fn save_engine_preset(&self, preset: EnginePreset) -> Result<(), AppError> {
        *self
            .engine_preset
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "engine preset lock poisoned"))? = preset;
        Ok(())
    }

    async fn load_compute_device(&self) -> Result<ComputeDevice, AppError> {
        self.compute_device
            .lock()
            .map(|device| *device)
            .map_err(|_| AppError::new("TEST_ERROR", "compute device lock poisoned"))
    }

    async fn save_compute_device(&self, device: ComputeDevice) -> Result<(), AppError> {
        *self
            .compute_device
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "compute device lock poisoned"))? = device;
        Ok(())
    }
}

#[async_trait]
impl TranscriptionPort for FakePort {
    async fn prepare_job(
        &self,
        input: &Path,
        output_root: &Path,
    ) -> Result<(PathBuf, PathBuf), AppError> {
        self.prepare_calls.fetch_add(1, Ordering::SeqCst);
        Ok((input.to_owned(), output_root.join("job")))
    }

    async fn transcribe(
        &self,
        job_id: Uuid,
        cancel: &mut oneshot::Receiver<()>,
        _input: &Path,
        output: &Path,
        _hint: &SpeakerHint,
        engine: EnginePreset,
        asr_context: Option<&str>,
    ) -> Result<CompletedTranscription, AppError> {
        self.seen_engines
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "engines lock poisoned"))?
            .push(engine);
        if let Some(context) = asr_context {
            self.asr_contexts
                .lock()
                .map_err(|_| AppError::new("TEST_ERROR", "asr context lock poisoned"))?
                .push(context.to_owned());
        }
        match &self.behavior {
            TranscriptionBehavior::Success => Ok(completion(output)),
            TranscriptionBehavior::Failure => {
                Err(AppError::new("TRANSCRIBE_FAILED", "expected test failure"))
            }
            TranscriptionBehavior::Blocking(started) => {
                let sender = started
                    .lock()
                    .map_err(|_| AppError::new("TEST_ERROR", "sender lock poisoned"))?
                    .take()
                    .ok_or_else(|| AppError::new("TEST_ERROR", "sender missing"))?;
                sender
                    .send(job_id)
                    .map_err(|_| AppError::new("TEST_ERROR", "receiver closed"))?;
                let _cancelled = cancel.await;
                Err(AppError::new("CANCELLED", "cancelled by test"))
            }
        }
    }
}

impl ArtifactPort for FakePort {
    fn open_file(&self, path: &Path, _trusted_root: &Path) -> Result<(), AppError> {
        self.opened_paths
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "opener lock poisoned"))?
            .push(path.to_owned());
        Ok(())
    }

    fn open_directory(&self, path: &Path) -> Result<(), AppError> {
        self.opened_paths
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "opener lock poisoned"))?
            .push(path.to_owned());
        Ok(())
    }
}

#[async_trait]
impl TranscriptImportPort for FakePort {
    async fn import_transcript(
        &self,
        input: &Path,
        _output_root: &Path,
    ) -> Result<Artifacts, AppError> {
        Ok(Artifacts {
            srt: None,
            txt: input.to_owned(),
            checkpoint: None,
            minutes: None,
            output_directory: input.parent().unwrap_or(Path::new("/tmp")).to_owned(),
            source_audio: None,
        })
    }
}

#[tokio::test]
async fn saved_hugging_face_token_is_trimmed_and_can_be_cleared() -> Result<(), AppError> {
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();

    app.save_hugging_face_token("  hf_saved  ".to_owned())
        .await?;
    assert!(app.hugging_face_token_stored().await?);
    assert_eq!(
        port.settings_token
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "settings token lock poisoned"))?
            .clone(),
        Some("hf_saved".to_owned())
    );

    app.save_hugging_face_token("   ".to_owned()).await?;
    assert!(!app.hugging_face_token_stored().await?);
    Ok(())
}

#[tokio::test]
async fn prepare_uses_saved_hugging_face_token() -> Result<(), AppError> {
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    app.save_hugging_face_token("hf_saved".to_owned()).await?;

    app.prepare(SetupRequest {
        job_id: Uuid::now_v7(),
        hugging_face_token: None,
    })
    .await?;

    let token = port
        .prepared_token
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "prepared token lock poisoned"))?;
    assert_eq!(token.as_deref(), Some("hf_saved"));
    Ok(())
}

#[tokio::test]
async fn invalid_hint_is_rejected_before_workspace_access() -> Result<(), AppError> {
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();

    let result = app
        .transcribe(request(SpeakerHint::Exact { count: 0 }))
        .await;

    let Err(error) = result else {
        return Err(AppError::new("TEST_ERROR", "invalid hint was accepted"));
    };
    assert_eq!(error.code, "INVALID_SPEAKER_HINT");
    assert_eq!(port.prepare_calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn transcription_carries_glossary_and_roster_for_asr_biasing() -> Result<(), AppError> {
    // Given: a saved glossary and one participant with a spoken alias
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    app.save_assistant_settings(AssistantSettings {
        api_key_stored: false,
        model: None,
        base_url: None,
        reasoning_effort: None,
        background: None,
        participants: vec![Participant {
            id: "hb".to_owned(),
            name: "하빈".to_owned(),
            team: None,
            role: None,
            description: None,
            aliases: vec!["프로님".to_owned()],
        }],
        glossary: vec![GlossaryEntry {
            id: "t1".to_owned(),
            term: "갈피".to_owned(),
            description: None,
        }],
    })
    .await?;

    // When
    app.transcribe(request(SpeakerHint::Auto)).await?;

    // Then: terms, names, and aliases all reach the worker for biasing
    let seen = port
        .asr_contexts
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "asr context lock poisoned"))?;
    let context = seen
        .first()
        .ok_or_else(|| AppError::new("TEST_ERROR", "asr context was not requested"))?;
    assert!(context.contains("갈피"));
    assert!(context.contains("하빈"));
    assert!(context.contains("프로님"));
    Ok(())
}

#[tokio::test]
async fn transcription_sends_no_asr_context_without_saved_context() -> Result<(), AppError> {
    // Given: no glossary and no roster
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();

    // When
    app.transcribe(request(SpeakerHint::Auto)).await?;

    // Then
    let seen = port
        .asr_contexts
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "asr context lock poisoned"))?;
    assert!(seen.is_empty());
    Ok(())
}

#[tokio::test]
async fn failed_transcription_releases_active_job() -> Result<(), AppError> {
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Failure));
    let app = port.application();

    for _attempt in 0..2 {
        let result = app.transcribe(request(SpeakerHint::Auto)).await;
        let Err(error) = result else {
            return Err(AppError::new(
                "TEST_ERROR",
                "transcription unexpectedly passed",
            ));
        };
        assert_eq!(error.code, "TRANSCRIBE_FAILED");
    }
    Ok(())
}

#[tokio::test]
async fn completed_artifact_is_opened_from_registry() -> Result<(), AppError> {
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    let result = app.transcribe(request(SpeakerHint::Auto)).await?;

    app.open_artifact(result.job_id, ArtifactKind::SpeakerText)?;

    let opened = port
        .opened_paths
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "opener lock poisoned"))?;
    assert_eq!(
        opened.as_slice(),
        [PathBuf::from("/tmp/output/job/meeting_화자별.txt")]
    );
    Ok(())
}

#[tokio::test]
async fn cancellation_reaches_running_port_without_timing_waits()
-> Result<(), Box<dyn std::error::Error>> {
    let (started_sender, mut started_receiver) = mpsc::unbounded_channel();
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Blocking(Mutex::new(
        Some(started_sender),
    ))));
    let app = Arc::new(port.application());
    let running = {
        let app = app.clone();
        tokio::spawn(async move { app.transcribe(request(SpeakerHint::Auto)).await })
    };
    let job_id = started_receiver
        .recv()
        .await
        .ok_or("worker did not announce start")?;

    app.cancel(job_id)?;
    let result = running.await?;

    let Err(error) = result else {
        return Err("cancelled transcription unexpectedly passed".into());
    };
    assert_eq!(error.code, "CANCELLED");
    Ok(())
}

#[tokio::test]
async fn refinement_sends_saved_background_and_publishes_minutes() -> Result<(), AppError> {
    // Given
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    app.save_assistant_api_key("  zai_key  ".to_owned()).await?;
    app.save_assistant_settings(AssistantSettings {
        api_key_stored: true,
        model: Some("glm-5-turbo".to_owned()),
        base_url: Some("https://openrouter.ai/api/v1".to_owned()),
        reasoning_effort: Some("max".to_owned()),
        background: Some("제품: 갈피\n팀리더: 하빈".to_owned()),
        participants: vec![
            participant("hb", "하빈"),
            participant("jw", "지우"),
            participant("ms", "민수"),
        ],
        glossary: vec![
            glossary_entry("t1", "갈피"),
            glossary_entry("t2", "화자분리"),
        ],
    })
    .await?;
    let transcription = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    let refined = app
        .refine_transcript(
            Uuid::now_v7(),
            transcription.job_id,
            &["ms".to_owned(), "hb".to_owned()],
        )
        .await?;

    // Then
    assert_eq!(
        Path::new(&refined.minutes),
        Path::new("/tmp/output/job/meeting_회의록.md")
    );
    let seen = port
        .refinements
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "refinement lock poisoned"))?;
    let job = seen
        .first()
        .ok_or_else(|| AppError::new("TEST_ERROR", "refinement was not requested"))?;
    assert_eq!(job.api_key, "zai_key");
    assert_eq!(job.model.as_deref(), Some("glm-5-turbo"));
    assert_eq!(
        job.base_url.as_deref(),
        Some("https://openrouter.ai/api/v1")
    );
    assert_eq!(job.reasoning_effort.as_deref(), Some("max"));
    assert_eq!(job.background.as_deref(), Some("제품: 갈피\n팀리더: 하빈"));
    // Only the selected attendees travel, in roster order rather than selection order.
    assert_eq!(job.participants, ["하빈", "민수"]);
    // The glossary is global context, so every saved term travels on each refinement.
    assert_eq!(job.glossary, ["갈피", "화자분리"]);

    app.open_artifact(transcription.job_id, ArtifactKind::Minutes)?;
    let opened = port
        .opened_paths
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "opener lock poisoned"))?;
    assert_eq!(
        opened.as_slice(),
        [PathBuf::from("/tmp/output/job/meeting_회의록.md")]
    );
    Ok(())
}

#[tokio::test]
async fn refinement_is_rejected_before_a_token_is_saved() -> Result<(), AppError> {
    // Given
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    let transcription = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    let result = app
        .refine_transcript(Uuid::now_v7(), transcription.job_id, &[])
        .await;

    // Then
    let Err(error) = result else {
        return Err(AppError::new(
            "TEST_ERROR",
            "refinement ran without a token",
        ));
    };
    assert_eq!(error.code, "ASSISTANT_KEY_MISSING");
    Ok(())
}

fn request(speaker_hint: SpeakerHint) -> TranscriptionRequest {
    TranscriptionRequest {
        job_id: Uuid::now_v7(),
        input_path: "/tmp/input.m4a".to_owned(),
        output_root: "/tmp/output".to_owned(),
        speaker_hint,
    }
}

fn completion(output: &Path) -> CompletedTranscription {
    CompletedTranscription {
        artifacts: Artifacts {
            srt: Some(output.join("meeting.srt")),
            txt: output.join("meeting_화자별.txt"),
            checkpoint: Some(output.join("meeting.aligned.v2.json")),
            minutes: None,
            output_directory: output.to_owned(),
            source_audio: Some(output.join("meeting.wav")),
        },
        segments: 8,
        filtered: 1,
    }
}

#[tokio::test]
async fn imported_transcript_is_refinable_without_transcription() -> Result<(), AppError> {
    // Given: an assistant key and an existing transcript file, no transcription run
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    app.save_assistant_api_key("zai_key".to_owned()).await?;
    app.save_assistant_settings(AssistantSettings {
        api_key_stored: true,
        ..AssistantSettings::default()
    })
    .await?;

    // When
    let imported = app
        .import_transcript(TranscriptImportRequest {
            job_id: Uuid::now_v7(),
            input_path: "/tmp/galpi/팀미팅/팀미팅.txt".to_owned(),
            output_root: "/tmp/galpi".to_owned(),
        })
        .await?;
    app.refine_transcript(Uuid::now_v7(), imported.job_id, &[])
        .await?;

    // Then: refinement received the imported transcript itself
    let seen = port
        .refinements
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "refinement lock poisoned"))?;
    let job = seen
        .first()
        .ok_or_else(|| AppError::new("TEST_ERROR", "refinement was not requested"))?;
    assert_eq!(job.transcript, Path::new("/tmp/galpi/팀미팅/팀미팅.txt"));
    Ok(())
}

#[tokio::test]
async fn imported_transcript_has_no_srt_or_checkpoint() -> Result<(), AppError> {
    // Given
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    let imported = app
        .import_transcript(TranscriptImportRequest {
            job_id: Uuid::now_v7(),
            input_path: "/tmp/galpi/팀미팅/팀미팅.txt".to_owned(),
            output_root: "/tmp/galpi".to_owned(),
        })
        .await?;

    // When / Then: only artifacts that exist can be opened
    let Err(error) = app.open_artifact(imported.job_id, ArtifactKind::Srt) else {
        return Err(AppError::new(
            "TEST_ERROR",
            "srt artifact unexpectedly opened",
        ));
    };
    assert_eq!(error.code, "ARTIFACT_NOT_FOUND");
    let Err(error) = app.open_artifact(imported.job_id, ArtifactKind::Checkpoint) else {
        return Err(AppError::new(
            "TEST_ERROR",
            "checkpoint artifact unexpectedly opened",
        ));
    };
    assert_eq!(error.code, "ARTIFACT_NOT_FOUND");
    app.open_artifact(imported.job_id, ArtifactKind::SpeakerText)?;
    Ok(())
}

#[tokio::test]
async fn transcription_defaults_to_qwen3_and_follows_the_saved_preset() -> Result<(), AppError> {
    // Given: a fresh store with no saved preset
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();

    // When: transcribing without touching the preset
    app.transcribe(request(SpeakerHint::Auto)).await?;

    // Then: the default candidate set was used
    {
        let seen = port
            .seen_engines
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "engines lock poisoned"))?;
        assert_eq!(*seen, [EnginePreset::Qwen3]);
    }

    // When: the user switches to the legacy set
    app.save_engine_preset(EnginePreset::WhisperX).await?;
    app.transcribe(request(SpeakerHint::Auto)).await?;

    // Then: the next transcription runs on WhisperX
    let seen = port
        .seen_engines
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "engines lock poisoned"))?;
    assert_eq!(*seen, [EnginePreset::Qwen3, EnginePreset::WhisperX]);
    Ok(())
}

#[tokio::test]
async fn prepare_prepares_the_selected_preset() -> Result<(), AppError> {
    // Given
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    app.save_engine_preset(EnginePreset::WhisperX).await?;

    // When
    app.prepare(SetupRequest {
        job_id: Uuid::now_v7(),
        hugging_face_token: None,
    })
    .await?;

    // Then
    let seen = port
        .seen_prepare_engines
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "prepare engines lock poisoned"))?;
    assert_eq!(*seen, [EnginePreset::WhisperX]);
    Ok(())
}

#[tokio::test]
async fn the_saved_compute_device_reaches_diagnose_prepare_and_the_readiness_gate()
-> Result<(), AppError> {
    // Given: a saved preset and a saved device
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    app.save_engine_preset(EnginePreset::WhisperX).await?;
    app.save_compute_device(ComputeDevice::Cuda).await?;
    let expected = EngineSelection {
        preset: EnginePreset::WhisperX,
        device: ComputeDevice::Cuda,
    };

    // When: diagnosing, preparing, and starting a transcription
    let status = app.diagnose().await?;
    app.prepare(SetupRequest {
        job_id: Uuid::now_v7(),
        hugging_face_token: None,
    })
    .await?;
    app.transcribe(request(SpeakerHint::Auto)).await?;

    // Then: every engine call saw the same selection
    assert_eq!(status.compute_device, ComputeDevice::Cuda);
    let seen = port
        .seen_selections
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "selections lock poisoned"))?;
    assert_eq!(seen.len(), 3);
    assert!(seen.iter().all(|selection| *selection == expected));
    Ok(())
}

#[tokio::test]
async fn save_compute_device_is_delegated_to_the_settings_port() -> Result<(), AppError> {
    // Given
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();

    // When
    app.save_compute_device(ComputeDevice::Cuda).await?;

    // Then
    assert_eq!(port.load_compute_device().await?, ComputeDevice::Cuda);
    Ok(())
}

mod chatgpt;
mod gateway;
mod recording;
