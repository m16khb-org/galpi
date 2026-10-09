mod capture;
mod cleanup;
mod failure;
mod power;
mod writer;

use crate::application::error::AppError;
use crate::application::model::{RecordingResult, RecordingStatus};
use crate::application::ports::{RecordingEvents, RecordingPort};
use async_trait::async_trait;
use cpal::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::adapters::outbound::paths::{canonical_blocking, recording_folder_name};
use cleanup::{
    cancel_and_remove, microphone_error, remove_if_exists, remove_partial, state_lock, with_cleanup,
};
use failure::{SharedFailure, take_failure};
struct ActiveRecording {
    id: Uuid,
    stream: Stream,
    _sleep_blocker: Option<power::SleepBlocker>,
    writer: writer::WriterHandle,
    failure: SharedFailure,
    partial_path: PathBuf,
    final_path: PathBuf,
    folder: PathBuf,
    sample_rate: u32,
    channels: u16,
    dropped_samples: Arc<AtomicU64>,
}

pub struct NativeRecorder {
    active: Arc<Mutex<Option<ActiveRecording>>>,
    events: Arc<dyn RecordingEvents>,
}

impl NativeRecorder {
    pub fn new(events: Arc<dyn RecordingEvents>) -> Self {
        Self {
            active: Arc::new(Mutex::new(None)),
            events,
        }
    }
}

#[async_trait]
impl RecordingPort for NativeRecorder {
    async fn start(
        &self,
        recording_id: Uuid,
        output_root: &Path,
    ) -> Result<RecordingStatus, AppError> {
        let active = self.active.clone();
        let events = self.events.clone();
        let output_root = output_root.to_owned();
        tokio::task::spawn_blocking(move || start_sync(&active, recording_id, &output_root, events))
            .await
            .map_err(|error| AppError::new("RECORDING_ERROR", error.to_string()))?
    }

    async fn stop(&self, recording_id: Uuid) -> Result<RecordingResult, AppError> {
        let active = self.active.clone();
        tokio::task::spawn_blocking(move || stop_sync(&active, recording_id))
            .await
            .map_err(|error| AppError::new("RECORDING_ERROR", error.to_string()))?
    }

    async fn cancel(&self, recording_id: Uuid) -> Result<(), AppError> {
        let active = self.active.clone();
        tokio::task::spawn_blocking(move || cancel_sync(&active, recording_id))
            .await
            .map_err(|error| AppError::new("RECORDING_ERROR", error.to_string()))?
    }
}

fn start_sync(
    state: &Mutex<Option<ActiveRecording>>,
    recording_id: Uuid,
    output_root: &Path,
    events: Arc<dyn RecordingEvents>,
) -> Result<RecordingStatus, AppError> {
    let mut active = state_lock(state)?;
    if active.is_some() {
        return Err(AppError::new(
            "RECORDING_BUSY",
            "이미 마이크 녹음이 진행 중입니다.",
        ));
    }
    std::fs::create_dir_all(output_root)
        .map_err(|error| AppError::io("녹음 폴더를 만들지 못했습니다", &error))?;
    let root = canonical_blocking(output_root)
        .map_err(|error| AppError::io("녹음 폴더를 확인하지 못했습니다", &error))?;
    // A recording immediately owns a meeting folder named after itself, so the
    // finished `.wav` and every later artifact share one predictable name.
    let folder_name = recording_folder_name();
    let folder = root.join(&folder_name);
    std::fs::create_dir_all(&folder)
        .map_err(|error| AppError::io("녹음 폴더를 만들지 못했습니다", &error))?;
    let final_path = folder.join(format!("{folder_name}.wav"));
    let partial_path = folder.join(format!("{folder_name}.wav.part"));

    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| {
        AppError::new(
            "MICROPHONE_UNAVAILABLE",
            "사용 가능한 마이크를 찾지 못했습니다.",
        )
    })?;
    let supported = device
        .default_input_config()
        .map_err(|error| microphone_error(&error))?;
    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let failure = failure::new(recording_id, events);
    let dropped_samples = Arc::new(AtomicU64::new(0));
    let mut writer = writer::spawn(&partial_path, sample_rate, channels, failure.clone())?;
    let recycled = writer.take_recycled();
    let stream = match capture::build(
        &device,
        &supported.config(),
        supported.sample_format(),
        capture::WriterChannels {
            sender: writer.sender(),
            recycled,
        },
        failure.clone(),
        dropped_samples.clone(),
        channels,
    ) {
        Ok(stream) => stream,
        Err(error) => {
            return Err(with_cleanup(
                error,
                cancel_and_remove(writer, &partial_path, &folder),
            ));
        }
    };
    if let Err(error) = stream.play() {
        drop(stream);
        return Err(with_cleanup(
            microphone_error(&error),
            cancel_and_remove(writer, &partial_path, &folder),
        ));
    }

    let sleep_blocker = power::SleepBlocker::acquire("Galpi meeting recording in progress");
    if sleep_blocker.is_none() {
        eprintln!("recording sleep assertion unavailable; recording continues without it");
    }

    *active = Some(ActiveRecording {
        id: recording_id,
        stream,
        _sleep_blocker: sleep_blocker,
        writer,
        failure,
        partial_path: partial_path.clone(),
        final_path,
        folder,
        sample_rate,
        channels,
        dropped_samples,
    });
    Ok(RecordingStatus {
        recording_id,
        path: partial_path.to_string_lossy().into_owned(),
        sample_rate,
        channels,
    })
}

fn stop_sync(
    state: &Mutex<Option<ActiveRecording>>,
    recording_id: Uuid,
) -> Result<RecordingResult, AppError> {
    let recording = take_recording(state, recording_id)?;
    drop(recording.stream);
    let trailing_dropped = recording.dropped_samples.swap(0, Ordering::Relaxed);
    let summary = match recording.writer.finish(trailing_dropped) {
        Ok(summary) => summary,
        Err(error) => {
            return Err(with_cleanup(
                error,
                remove_partial(&recording.partial_path, &recording.folder),
            ));
        }
    };
    let callback_failure = match take_failure(&recording.failure) {
        Ok(failure) => failure,
        Err(error) => {
            return Err(with_cleanup(
                error,
                remove_partial(&recording.partial_path, &recording.folder),
            ));
        }
    };
    if let Some((code, message)) = callback_failure {
        return Err(with_cleanup(
            AppError::new(&code, message),
            remove_partial(&recording.partial_path, &recording.folder),
        ));
    }
    let frames = summary.samples / u64::from(recording.channels);
    let Ok(duration_frames) = u32::try_from(frames) else {
        return Err(with_cleanup(
            AppError::new(
                "WAV_TOO_LARGE",
                "녹음 길이가 지원 가능한 PCM WAV 범위를 초과했습니다.",
            ),
            remove_partial(&recording.partial_path, &recording.folder),
        ));
    };
    let duration_seconds = f64::from(duration_frames) / f64::from(recording.sample_rate);
    if let Err(error) = std::fs::rename(&recording.partial_path, &recording.final_path) {
        return Err(with_cleanup(
            AppError::io("완료된 녹음 파일 이름을 확정하지 못했습니다", &error),
            remove_if_exists(&recording.partial_path),
        ));
    }
    let path = canonical_blocking(&recording.final_path)
        .map_err(|error| AppError::io("완료된 녹음 파일을 확인하지 못했습니다", &error))?;
    Ok(RecordingResult {
        recording_id,
        path: path.to_string_lossy().into_owned(),
        sample_rate: recording.sample_rate,
        channels: recording.channels,
        frames,
        dropped_frames: summary.dropped_samples / u64::from(recording.channels),
        duration_seconds,
    })
}

fn cancel_sync(state: &Mutex<Option<ActiveRecording>>, recording_id: Uuid) -> Result<(), AppError> {
    let recording = take_recording(state, recording_id)?;
    drop(recording.stream);
    cancel_and_remove(recording.writer, &recording.partial_path, &recording.folder)
}

fn take_recording(
    state: &Mutex<Option<ActiveRecording>>,
    recording_id: Uuid,
) -> Result<ActiveRecording, AppError> {
    let mut active = state_lock(state)?;
    if active
        .as_ref()
        .is_some_and(|recording| recording.id != recording_id)
    {
        return Err(AppError::new(
            "RECORDING_ID_MISMATCH",
            "다른 녹음 세션은 정지하거나 취소할 수 없습니다.",
        ));
    }
    active
        .take()
        .ok_or_else(|| AppError::new("RECORDING_NOT_ACTIVE", "진행 중인 마이크 녹음이 없습니다."))
}

#[cfg(test)]
mod tests;
