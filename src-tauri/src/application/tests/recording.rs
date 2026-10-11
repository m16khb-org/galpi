use super::{FakePort, TranscriptionBehavior};
use crate::application::error::AppError;
use crate::application::model::{RecordingResult, RecordingStatus};
use crate::application::ports::RecordingPort;
use crate::domain::recording::RecordingSource;
use async_trait::async_trait;
use std::path::Path;
use uuid::Uuid;

#[async_trait]
impl RecordingPort for FakePort {
    async fn start(
        &self,
        recording_id: Uuid,
        output_root: &Path,
        source: RecordingSource,
    ) -> Result<RecordingStatus, AppError> {
        self.started_recording_sources
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "started sources lock poisoned"))?
            .push(source);
        Ok(RecordingStatus {
            recording_id,
            path: output_root
                .join("recording.wav")
                .to_string_lossy()
                .into_owned(),
            sample_rate: 48_000,
            channels: 1,
        })
    }

    async fn stop(&self, recording_id: Uuid) -> Result<RecordingResult, AppError> {
        Ok(RecordingResult {
            recording_id,
            path: "/tmp/recording.wav".to_owned(),
            sample_rate: 48_000,
            channels: 1,
            frames: 48_000,
            dropped_frames: 0,
            duration_seconds: 1.0,
        })
    }

    async fn cancel(&self, _recording_id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
}

#[tokio::test]
async fn recording_lifecycle_rejects_reentry_and_wrong_session() -> Result<(), AppError> {
    let app = std::sync::Arc::new(FakePort::new(TranscriptionBehavior::Success)).application();
    let started = app
        .start_recording("/tmp".to_owned(), RecordingSource::Microphone)
        .await?;

    let busy = app
        .start_recording("/tmp".to_owned(), RecordingSource::Microphone)
        .await;
    let Err(busy) = busy else {
        return Err(AppError::new(
            "TEST_ERROR",
            "second recording unexpectedly started",
        ));
    };
    assert_eq!(busy.code, "RECORDING_BUSY");

    let mismatch = app.stop_recording(Uuid::now_v7()).await;
    let Err(mismatch) = mismatch else {
        return Err(AppError::new(
            "TEST_ERROR",
            "wrong recording id unexpectedly stopped",
        ));
    };
    assert_eq!(mismatch.code, "RECORDING_ID_MISMATCH");

    let stopped = app.stop_recording(started.recording_id).await?;
    assert_eq!(stopped.recording_id, started.recording_id);
    let restarted = app
        .start_recording("/tmp".to_owned(), RecordingSource::Microphone)
        .await?;
    app.cancel_recording(restarted.recording_id).await
}

#[tokio::test]
async fn the_chosen_recording_source_reaches_the_recorder_and_is_remembered() -> Result<(), AppError>
{
    // Given
    let port = std::sync::Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.application();
    assert_eq!(
        app.load_recording_source().await?,
        RecordingSource::Microphone
    );

    // When: the user picks the combined source and starts recording
    app.save_recording_source(RecordingSource::MicrophoneAndSystem)
        .await?;
    let source = app.load_recording_source().await?;
    let started = app.start_recording("/tmp".to_owned(), source).await?;

    // Then
    assert_eq!(source, RecordingSource::MicrophoneAndSystem);
    assert_eq!(
        port.started_recording_sources
            .lock()
            .map_err(|_| AppError::new("TEST_ERROR", "started sources lock poisoned"))?
            .as_slice(),
        [RecordingSource::MicrophoneAndSystem]
    );
    app.cancel_recording(started.recording_id).await
}
