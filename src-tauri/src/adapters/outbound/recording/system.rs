//! The computer's own output as a second capture track.
//!
//! cpal records a render (output) device by opening an *input* stream on it:
//! WASAPI loopback on Windows, a Core Audio process tap on macOS 14.6+. The
//! samples then take the same bounded-queue path as the microphone.

use super::capture::{self, CaptureState, SYSTEM_AUDIO_ERRORS, WriterChannels};
use super::cleanup::{cancel_track, system_audio_unavailable, with_cleanup};
use super::failure::SharedFailure;
use super::writer::{self, WriterHandle};
use crate::adapters::outbound::platform::Os;
use crate::application::error::AppError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, SupportedStreamConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The running system-audio capture of one recording.
pub struct SystemTrack {
    pub stream: Stream,
    /// Plays silence on the same device so the loopback keeps delivering.
    pub keepalive: Option<Stream>,
    pub writer: WriterHandle,
    pub partial_path: PathBuf,
    /// Interleaved samples the callback could not queue since the last chunk;
    /// the writer turns them into frames.
    pub dropped_samples: Arc<AtomicU64>,
}

impl SystemTrack {
    /// Stop capturing and wait for the file to be complete on disk.
    pub fn finish(self) -> Result<(PathBuf, u64), AppError> {
        let Self {
            stream,
            keepalive,
            writer,
            partial_path,
            dropped_samples,
        } = self;
        drop(stream);
        drop(keepalive);
        let trailing = dropped_samples.swap(0, Ordering::Relaxed);
        let summary = writer.finish(trailing)?;
        Ok((partial_path, summary.dropped_samples))
    }

    /// Stop capturing and delete the partial file.
    pub fn cancel(self) -> Result<(), AppError> {
        drop(self.stream);
        drop(self.keepalive);
        cancel_track(self.writer, &self.partial_path)
    }
}

/// Open the default output device as a capture track writing to `partial_path`.
///
/// Every failure is reported as `SYSTEM_AUDIO_UNAVAILABLE`, and the track's own
/// writer and file are already gone when it returns.
pub fn start(
    host: &Host,
    os: Os,
    partial_path: &Path,
    failure: &SharedFailure,
) -> Result<SystemTrack, AppError> {
    let device = host
        .default_output_device()
        .ok_or_else(|| system_audio_unavailable(os, "소리를 받을 출력 장치를 찾지 못했습니다."))?;
    // A Core Audio device that also has inputs is recorded through those inputs
    // instead of the process tap, which would capture a microphone, not the
    // computer's sound. WASAPI reports render endpoints as inputs too, so the
    // check only makes sense on macOS.
    if os == Os::MacOs && device.supports_input() {
        return Err(system_audio_unavailable(
            os,
            "기본 출력 장치에 입력 기능이 함께 있어 컴퓨터 소리만 따로 받을 수 없습니다. \
             입력이 없는 스피커나 내장 출력으로 바꿔 주세요.",
        ));
    }
    let supported = device
        .default_output_config()
        .map_err(|error| system_audio_unavailable(os, &error.to_string()))?;
    let mut writer = writer::spawn(
        partial_path,
        supported.sample_rate(),
        supported.channels(),
        true,
        failure.clone(),
    )?;
    let recycled = writer.take_recycled();
    let dropped_samples = Arc::new(AtomicU64::new(0));
    let opened = open_streams(
        &device,
        &supported,
        os,
        WriterChannels {
            sender: writer.sender(),
            recycled,
        },
        CaptureState {
            failure: failure.clone(),
            dropped_samples: dropped_samples.clone(),
            channels: supported.channels(),
            errors: SYSTEM_AUDIO_ERRORS,
        },
    );
    match opened {
        Ok((stream, keepalive)) => Ok(SystemTrack {
            stream,
            keepalive,
            writer,
            partial_path: partial_path.to_owned(),
            dropped_samples,
        }),
        Err(error) => Err(with_cleanup(error, cancel_track(writer, partial_path))),
    }
}

fn open_streams(
    device: &Device,
    supported: &SupportedStreamConfig,
    os: Os,
    channels_out: WriterChannels,
    state: CaptureState,
) -> Result<(Stream, Option<Stream>), AppError> {
    // WASAPI loopback only delivers packets while something is playing, so a
    // silent stretch would be missing from the file and shift everything after
    // it earlier than the microphone track. A silent render stream keeps the
    // endpoint producing audio; it starts first so the first packet is not lost.
    let keepalive = if os == Os::Windows {
        Some(silence(device, supported, os)?)
    } else {
        None
    };
    let stream = capture::build(
        device,
        &supported.config(),
        supported.sample_format(),
        channels_out,
        state,
    )
    .map_err(|error| system_audio_unavailable(os, &error.message))?;
    stream
        .play()
        .map_err(|error| system_audio_unavailable(os, &error.to_string()))?;
    Ok((stream, keepalive))
}

/// A render stream that outputs nothing: cpal hands the callback a buffer
/// already filled with silence, which this leaves untouched.
fn silence(device: &Device, supported: &SupportedStreamConfig, os: Os) -> Result<Stream, AppError> {
    let stream = device
        .build_output_stream_raw(
            supported.config(),
            supported.sample_format(),
            |_data, _| {},
            |_error| {},
            None,
        )
        .map_err(|error| system_audio_unavailable(os, &error.to_string()))?;
    stream
        .play()
        .map_err(|error| system_audio_unavailable(os, &error.to_string()))?;
    Ok(stream)
}
