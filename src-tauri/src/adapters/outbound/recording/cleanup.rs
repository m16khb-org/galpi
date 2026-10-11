use super::writer::WriterHandle;
use crate::adapters::outbound::platform::Os;
use crate::application::error::AppError;
use cpal::ErrorKind;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

impl Drop for super::NativeRecorder {
    fn drop(&mut self) {
        if Arc::strong_count(&self.active) != 1 {
            return;
        }
        let recording = self.active.lock().ok().and_then(|mut active| active.take());
        if let Some(recording) = recording
            && let Err(error) = recording.cancel()
        {
            eprintln!("recording shutdown cleanup failed: {error}");
        }
    }
}

pub fn state_lock<T>(state: &Mutex<T>) -> Result<MutexGuard<'_, T>, AppError> {
    state
        .lock()
        .map_err(|_| AppError::new("STATE_ERROR", "녹음 상태 잠금이 손상되었습니다."))
}

pub fn microphone_error(error: &cpal::Error) -> AppError {
    let code = match error.kind() {
        ErrorKind::PermissionDenied => "MICROPHONE_PERMISSION_DENIED",
        ErrorKind::DeviceNotAvailable => "MICROPHONE_UNAVAILABLE",
        ErrorKind::DeviceBusy => "MICROPHONE_BUSY",
        ErrorKind::UnsupportedConfig => "UNSUPPORTED_AUDIO_CONFIG",
        _ => "MICROPHONE_ERROR",
    };
    AppError::new(code, error.to_string())
}

/// The computer's own output could not be opened. The microphone-only source
/// does not need it, so the message ends by saying that still works.
pub fn system_audio_unavailable(os: Os, detail: &str) -> AppError {
    let remedy = match os {
        Os::MacOs => {
            "macOS 14.6 이상이 필요하며, 시스템 설정의 개인정보 보호 및 보안에서 \
             갈피의 시스템 오디오 녹음을 허용해야 합니다."
        }
        Os::Windows => "기본 출력 장치가 연결되어 있는지 확인해 주세요.",
    };
    AppError::new(
        "SYSTEM_AUDIO_UNAVAILABLE",
        format!(
            "컴퓨터에서 재생되는 소리를 녹음하지 못했습니다: {detail} {remedy} \
             마이크만 녹음은 계속 쓸 수 있습니다."
        ),
    )
}

pub fn remove_if_exists(path: &Path) -> Result<(), AppError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::io("녹음 파일을 지우지 못했습니다", &error)),
    }
}

/// Stop one track's writer and delete its partial file, whichever of the two fails.
pub fn cancel_track(writer: WriterHandle, path: &Path) -> Result<(), AppError> {
    let writer_result = writer.cancel();
    let remove_result = remove_if_exists(path);
    writer_result.and(remove_result)
}

pub fn cancel_and_remove(writer: WriterHandle, path: &Path, folder: &Path) -> Result<(), AppError> {
    let result = cancel_track(writer, path);
    let _ = std::fs::remove_dir(folder);
    result
}

/// Remove every partial file of one recording, then its meeting folder if that
/// left it empty. Every path is tried even when an earlier one fails.
pub fn remove_partials(paths: &[&Path], folder: &Path) -> Result<(), AppError> {
    let result = paths
        .iter()
        .map(|path| remove_if_exists(path))
        .fold(Ok(()), Result::and);
    let _ = std::fs::remove_dir(folder);
    result
}

pub fn with_cleanup(error: AppError, cleanup: Result<(), AppError>) -> AppError {
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => AppError::new(
            "RECORDING_CLEANUP_FAILED",
            format!("{error}; cleanup: {cleanup_error}"),
        ),
    }
}
