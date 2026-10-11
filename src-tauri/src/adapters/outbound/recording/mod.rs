mod capture;
mod cleanup;
mod failure;
mod mix;
mod power;
mod system;
mod writer;

use crate::adapters::outbound::platform::Os;
use crate::application::error::AppError;
use crate::application::model::{RecordingResult, RecordingStatus};
use crate::application::ports::{RecordingEvents, RecordingPort};
use crate::domain::recording::RecordingSource;
use async_trait::async_trait;
use cpal::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::adapters::outbound::paths::{canonical_blocking, recording_folder_name};
use cleanup::{
    cancel_and_remove, cancel_track, microphone_error, remove_if_exists, remove_partials,
    state_lock, with_cleanup,
};
use failure::{SharedFailure, take_failure};

/// Where one recording keeps its audio on disk.
///
/// A microphone-only recording writes `<name>.wav.part` and renames it. With
/// system audio the two tracks are separate files until the recording stops,
/// then merge into `<name>.wav.part` and are renamed from there.
#[derive(Clone)]
struct Files {
    microphone_part: PathBuf,
    system_part: Option<PathBuf>,
    merged_part: PathBuf,
    final_path: PathBuf,
    folder: PathBuf,
}

impl Files {
    fn plan(folder: &Path, name: &str, source: RecordingSource) -> Self {
        let merged_part = folder.join(format!("{name}.wav.part"));
        let (microphone_part, system_part) = if source.includes_system_audio() {
            (
                folder.join(format!("{name}.mic.wav.part")),
                Some(folder.join(format!("{name}.system.wav.part"))),
            )
        } else {
            (merged_part.clone(), None)
        };
        Self {
            microphone_part,
            system_part,
            merged_part,
            final_path: folder.join(format!("{name}.wav")),
            folder: folder.to_owned(),
        }
    }

    /// Every unfinished file this recording may have left behind.
    fn parts(&self) -> Vec<&Path> {
        let mut parts = vec![self.microphone_part.as_path()];
        if let Some(system) = &self.system_part {
            parts.push(system);
        }
        if self.merged_part != self.microphone_part {
            parts.push(&self.merged_part);
        }
        parts
    }

    fn remove_parts(&self) -> Result<(), AppError> {
        remove_partials(&self.parts(), &self.folder)
    }
}

struct ActiveRecording {
    id: Uuid,
    stream: Stream,
    sleep_blocker: Option<power::SleepBlocker>,
    writer: writer::WriterHandle,
    failure: SharedFailure,
    files: Files,
    sample_rate: u32,
    channels: u16,
    dropped_samples: Arc<AtomicU64>,
    system: Option<system::SystemTrack>,
}

impl ActiveRecording {
    /// Close every stream and delete every partial file.
    fn cancel(self) -> Result<(), AppError> {
        let Self {
            stream,
            writer,
            files,
            system,
            ..
        } = self;
        drop(stream);
        let microphone = cancel_track(writer, &files.microphone_part);
        let system = system.map_or(Ok(()), system::SystemTrack::cancel);
        let leftovers = files.remove_parts();
        microphone.and(system).and(leftovers)
    }
}

/// What the capture side hands over once every stream is closed and every
/// writer has finished its file.
struct Stopped {
    sample_rate: u32,
    channels: u16,
    microphone_samples: u64,
    microphone_dropped_samples: u64,
    /// The finished system track and how many of its frames were dropped.
    system: Option<(PathBuf, u64)>,
}

/// The finished audio file's shape.
struct Audio {
    sample_rate: u32,
    channels: u16,
    frames: u64,
    /// Dropped sample frames summed over the tracks, each counted at its own rate.
    dropped_frames: u64,
    duration_seconds: f64,
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
        source: RecordingSource,
    ) -> Result<RecordingStatus, AppError> {
        let active = self.active.clone();
        let events = self.events.clone();
        let output_root = output_root.to_owned();
        tokio::task::spawn_blocking(move || {
            start_sync(&active, recording_id, &output_root, source, events)
        })
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

/// The default microphone, already recording into its partial file.
struct MicrophoneTrack {
    stream: Stream,
    writer: writer::WriterHandle,
    sample_rate: u32,
    channels: u16,
    dropped_samples: Arc<AtomicU64>,
}

/// Open the default microphone. On failure its writer and partial file are
/// already gone; the caller removes the meeting folder.
fn start_microphone(
    host: &cpal::Host,
    files: &Files,
    failure: &SharedFailure,
) -> Result<MicrophoneTrack, AppError> {
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
    let dropped_samples = Arc::new(AtomicU64::new(0));
    let mut writer = writer::spawn(
        &files.microphone_part,
        sample_rate,
        channels,
        false,
        failure.clone(),
    )?;
    let recycled = writer.take_recycled();
    let stream = match capture::build(
        &device,
        &supported.config(),
        supported.sample_format(),
        capture::WriterChannels {
            sender: writer.sender(),
            recycled,
        },
        capture::CaptureState {
            failure: failure.clone(),
            dropped_samples: dropped_samples.clone(),
            channels,
            errors: capture::MICROPHONE_ERRORS,
        },
    ) {
        Ok(stream) => stream,
        Err(error) => {
            return Err(with_cleanup(
                error,
                cancel_and_remove(writer, &files.microphone_part, &files.folder),
            ));
        }
    };
    if let Err(error) = stream.play() {
        drop(stream);
        return Err(with_cleanup(
            microphone_error(&error),
            cancel_and_remove(writer, &files.microphone_part, &files.folder),
        ));
    }
    Ok(MicrophoneTrack {
        stream,
        writer,
        sample_rate,
        channels,
        dropped_samples,
    })
}

fn start_sync(
    state: &Mutex<Option<ActiveRecording>>,
    recording_id: Uuid,
    output_root: &Path,
    source: RecordingSource,
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
    let files = Files::plan(&folder, &folder_name, source);

    let host = cpal::default_host();
    let failure = failure::new(recording_id, events);
    // Every early failure (no microphone, unreadable config, writer not
    // created) must leave no empty meeting folder behind; `remove_dir` only
    // succeeds on an empty one, so paths that already cleaned up are unaffected.
    let microphone = start_microphone(&host, &files, &failure).inspect_err(|_| {
        let _removed = std::fs::remove_dir(&folder);
    })?;
    let system = match files
        .system_part
        .as_deref()
        .map(|part| system::start(&host, Os::current(), part, &failure))
        .transpose()
    {
        Ok(track) => track,
        Err(error) => {
            // The track already removed its own writer and file.
            drop(microphone.stream);
            return Err(with_cleanup(
                error,
                cancel_and_remove(microphone.writer, &files.microphone_part, &folder),
            ));
        }
    };

    let sleep_blocker = power::SleepBlocker::acquire("Galpi meeting recording in progress");
    if sleep_blocker.is_none() {
        eprintln!("recording sleep assertion unavailable; recording continues without it");
    }

    let path = files.merged_part.to_string_lossy().into_owned();
    let (sample_rate, channels) = (microphone.sample_rate, microphone.channels);
    *active = Some(ActiveRecording {
        id: recording_id,
        stream: microphone.stream,
        sleep_blocker,
        writer: microphone.writer,
        failure,
        files,
        sample_rate,
        channels,
        dropped_samples: microphone.dropped_samples,
        system,
    });
    Ok(RecordingStatus {
        recording_id,
        path,
        sample_rate,
        channels,
    })
}

fn stop_sync(
    state: &Mutex<Option<ActiveRecording>>,
    recording_id: Uuid,
) -> Result<RecordingResult, AppError> {
    let mut recording = take_recording(state, recording_id)?;
    let files = recording.files.clone();
    // Held until the merge is done: the machine must not sleep mid-merge.
    let _sleep_blocker = recording.sleep_blocker.take();
    let finished = stop_capture(recording).and_then(|stopped| assemble(&files, &stopped));
    let audio = finished.map_err(|error| with_cleanup(error, files.remove_parts()))?;
    let path = canonical_blocking(&files.final_path)
        .map_err(|error| AppError::io("완료된 녹음 파일을 확인하지 못했습니다", &error))?;
    Ok(RecordingResult {
        recording_id,
        path: path.to_string_lossy().into_owned(),
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        frames: audio.frames,
        dropped_frames: audio.dropped_frames,
        duration_seconds: audio.duration_seconds,
    })
}

/// Close the streams and wait for every writer. Both writers are always
/// finished, even when the first fails, so no writer thread is left behind.
fn stop_capture(recording: ActiveRecording) -> Result<Stopped, AppError> {
    let ActiveRecording {
        stream,
        writer,
        failure,
        sample_rate,
        channels,
        dropped_samples,
        system,
        ..
    } = recording;
    drop(stream);
    let trailing_dropped = dropped_samples.swap(0, Ordering::Relaxed);
    let microphone = writer.finish(trailing_dropped);
    let system = system.map(system::SystemTrack::finish);
    let microphone = microphone?;
    let system = system.transpose()?;
    if let Some((code, message)) = take_failure(&failure)? {
        return Err(AppError::new(&code, message));
    }
    Ok(Stopped {
        sample_rate,
        channels,
        microphone_samples: microphone.samples,
        microphone_dropped_samples: microphone.dropped_samples,
        system,
    })
}

/// Turn the finished tracks into `<name>.wav`.
///
/// Removes nothing on failure; the caller deletes every partial file.
fn assemble(files: &Files, stopped: &Stopped) -> Result<Audio, AppError> {
    let microphone_dropped_frames =
        stopped.microphone_dropped_samples / u64::from(stopped.channels);
    let (frames, channels, dropped_frames) = match &stopped.system {
        None => (
            stopped.microphone_samples / u64::from(stopped.channels),
            stopped.channels,
            microphone_dropped_frames,
        ),
        Some((system_part, system_dropped_frames)) => {
            let frames = mix::mix_tracks(&files.microphone_part, system_part, &files.merged_part)?;
            remove_if_exists(&files.microphone_part)?;
            remove_if_exists(system_part)?;
            (frames, 1, microphone_dropped_frames + system_dropped_frames)
        }
    };
    let Ok(duration_frames) = u32::try_from(frames) else {
        return Err(AppError::new(
            "WAV_TOO_LARGE",
            "녹음 길이가 지원 가능한 PCM WAV 범위를 초과했습니다.",
        ));
    };
    if let Err(error) = std::fs::rename(&files.merged_part, &files.final_path) {
        return Err(AppError::io(
            "완료된 녹음 파일 이름을 확정하지 못했습니다",
            &error,
        ));
    }
    Ok(Audio {
        sample_rate: stopped.sample_rate,
        channels,
        frames,
        dropped_frames,
        duration_seconds: f64::from(duration_frames) / f64::from(stopped.sample_rate),
    })
}

fn cancel_sync(state: &Mutex<Option<ActiveRecording>>, recording_id: Uuid) -> Result<(), AppError> {
    take_recording(state, recording_id)?.cancel()
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
