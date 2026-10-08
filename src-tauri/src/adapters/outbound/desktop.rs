use super::paths::{AppPaths, canonical_blocking, prepare_job_directory};
use super::{import, refinement, setup, transcription};
use crate::application::error::AppError;
use crate::application::model::{CompletedTranscription, EnvironmentStatus};
use crate::application::ports::{
    ArtifactPort, EnginePort, JobEvents, RefinementJob, RefinementPort, TranscriptImportPort,
    TranscriptionPort,
};
use crate::domain::artifact::Artifacts;
use crate::domain::engine::{EnginePreset, EngineSelection};
use crate::domain::job::{SetupRequest, SpeakerHint};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Clone)]
pub struct DesktopAdapter {
    app: AppHandle,
    events: std::sync::Arc<dyn JobEvents>,
}

impl DesktopAdapter {
    pub fn new(app: AppHandle, events: std::sync::Arc<dyn JobEvents>) -> Self {
        Self { app, events }
    }
}

#[async_trait]
impl EnginePort for DesktopAdapter {
    async fn diagnose(&self, selection: EngineSelection) -> Result<EnvironmentStatus, AppError> {
        setup::diagnose(&self.app, selection)
    }

    async fn prepare(
        &self,
        job_id: Uuid,
        cancel: &mut oneshot::Receiver<()>,
        request: &SetupRequest,
        selection: EngineSelection,
    ) -> Result<EnvironmentStatus, AppError> {
        setup::prepare(
            &self.app,
            self.events.as_ref(),
            job_id,
            cancel,
            request,
            selection,
        )
        .await
    }
}

#[async_trait]
impl TranscriptionPort for DesktopAdapter {
    async fn prepare_job(
        &self,
        input: &Path,
        output_root: &Path,
    ) -> Result<(PathBuf, PathBuf), AppError> {
        prepare_job_directory(input, output_root).await
    }

    async fn transcribe(
        &self,
        job_id: Uuid,
        cancel: &mut oneshot::Receiver<()>,
        input: &Path,
        output: &Path,
        hint: &SpeakerHint,
        engine: EnginePreset,
        asr_context: Option<&str>,
    ) -> Result<CompletedTranscription, AppError> {
        let paths = AppPaths::resolve(&self.app)?;
        transcription::run(
            transcription::Runtime {
                app: &self.app,
                events: self.events.as_ref(),
                paths: &paths,
            },
            job_id,
            cancel,
            input,
            output,
            hint,
            engine,
            asr_context,
        )
        .await
    }
}

#[async_trait]
impl RefinementPort for DesktopAdapter {
    async fn refine(
        &self,
        job_id: Uuid,
        cancel: &mut oneshot::Receiver<()>,
        job: RefinementJob<'_>,
    ) -> Result<PathBuf, AppError> {
        let paths = AppPaths::resolve(&self.app)?;
        refinement::run(
            refinement::Runtime {
                app: &self.app,
                events: self.events.as_ref(),
                paths: &paths,
            },
            job_id,
            cancel,
            job,
        )
        .await
    }
}

#[async_trait]
impl TranscriptImportPort for DesktopAdapter {
    async fn import_transcript(
        &self,
        input: &Path,
        output_root: &Path,
    ) -> Result<Artifacts, AppError> {
        import::import_transcript(input, output_root).await
    }
}

impl ArtifactPort for DesktopAdapter {
    fn open_file(&self, path: &Path, trusted_root: &Path) -> Result<(), AppError> {
        let root = canonical_blocking(trusted_root)
            .map_err(|error| AppError::io("출력 폴더를 다시 확인하지 못했습니다", &error))?;
        let path = canonical_blocking(path)
            .map_err(|error| AppError::io("결과 파일을 다시 확인하지 못했습니다", &error))?;
        if !path.starts_with(root)
            || !std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_file())
        {
            return Err(AppError::new(
                "ARTIFACT_PATH_ERROR",
                "결과 파일이 신뢰된 출력 폴더를 벗어났습니다.",
            ));
        }
        self.app
            .opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|error| AppError::new("OPEN_ERROR", error.to_string()))
    }

    fn open_directory(&self, path: &Path) -> Result<(), AppError> {
        let path = canonical_blocking(path)
            .map_err(|error| AppError::io("출력 폴더를 다시 확인하지 못했습니다", &error))?;
        self.app
            .opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|error| AppError::new("OPEN_ERROR", error.to_string()))
    }
}
