use super::failure::{SharedFailure, has_failure, record_failure, set_failure};
use super::writer::{FRAMES_PER_CHUNK, WriterCommand};
use crate::application::error::AppError;
use cpal::traits::DeviceTrait;
use cpal::{
    Device, ErrorKind, FromSample, I24, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    U24,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};

/// The writer-facing endpoints one capture stream owns.
pub struct WriterChannels {
    pub sender: SyncSender<WriterCommand>,
    /// Buffers the writer has finished with, waiting to be filled again.
    pub recycled: Receiver<Vec<i16>>,
}

/// How a capture stream names itself when something goes wrong, so a second
/// stream (the computer's own output) reports under its own codes.
#[derive(Clone, Copy)]
pub struct CaptureErrors {
    pub disconnected: &'static str,
    pub error: &'static str,
    /// What the user is told is being captured, for unsupported formats.
    pub label: &'static str,
}

pub const MICROPHONE_ERRORS: CaptureErrors = CaptureErrors {
    disconnected: "MICROPHONE_DISCONNECTED",
    error: "MICROPHONE_ERROR",
    label: "마이크",
};

pub const SYSTEM_AUDIO_ERRORS: CaptureErrors = CaptureErrors {
    disconnected: "SYSTEM_AUDIO_DISCONNECTED",
    error: "SYSTEM_AUDIO_ERROR",
    label: "컴퓨터 소리",
};

/// What the capture callback shares with the rest of the recording.
pub struct CaptureState {
    pub failure: SharedFailure,
    pub dropped_samples: Arc<AtomicU64>,
    /// Interleaved channels the device delivers.
    pub channels: u16,
    pub errors: CaptureErrors,
}

pub fn build(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    channels_out: WriterChannels,
    state: CaptureState,
) -> Result<Stream, AppError> {
    let capture = Capture {
        device,
        config,
        sender: channels_out.sender,
        recycled: channels_out.recycled,
        state,
    };
    match format {
        SampleFormat::I8 => capture.typed::<i8>(convert::<i8>),
        SampleFormat::I16 => capture.typed::<i16>(|sample| sample),
        SampleFormat::I24 => capture.typed::<I24>(convert::<I24>),
        SampleFormat::I32 => capture.typed::<i32>(convert::<i32>),
        SampleFormat::I64 => capture.typed::<i64>(convert::<i64>),
        SampleFormat::U8 => capture.typed::<u8>(convert::<u8>),
        SampleFormat::U16 => capture.typed::<u16>(convert::<u16>),
        SampleFormat::U24 => capture.typed::<U24>(convert::<U24>),
        SampleFormat::U32 => capture.typed::<u32>(convert::<u32>),
        SampleFormat::U64 => capture.typed::<u64>(convert::<u64>),
        SampleFormat::F32 => capture.typed::<f32>(f32_to_i16),
        SampleFormat::F64 => capture.typed::<f64>(f64_to_i16),
        unsupported => Err(AppError::new(
            "UNSUPPORTED_AUDIO_CONFIG",
            format!(
                "지원하지 않는 {} 샘플 형식입니다: {unsupported}",
                capture.state.errors.label
            ),
        )),
    }
}

struct Capture<'a> {
    device: &'a Device,
    config: &'a StreamConfig,
    sender: SyncSender<WriterCommand>,
    /// Buffers the writer has finished with, waiting to be filled again.
    recycled: Receiver<Vec<i16>>,
    state: CaptureState,
}

impl Capture<'_> {
    fn typed<T>(self, convert: fn(T) -> i16) -> Result<Stream, AppError>
    where
        T: SizedSample + Copy + Send + 'static,
    {
        let callback_failure = self.state.failure.clone();
        let errors = self.state.errors;
        self.device
            .build_input_stream(
                *self.config,
                move |input: &[T], _| {
                    enqueue(
                        input,
                        &self.sender,
                        &self.recycled,
                        &self.state.failure,
                        &self.state.dropped_samples,
                        self.state.channels,
                        convert,
                    );
                },
                move |error| {
                    if is_recoverable_stream_error(error.kind()) {
                        return;
                    }
                    set_failure(&callback_failure, errors.disconnected, error.to_string());
                },
                None,
            )
            .map_err(|error| AppError::new(errors.error, error.to_string()))
    }
}

pub(super) fn is_recoverable_stream_error(kind: ErrorKind) -> bool {
    matches!(kind, ErrorKind::Xrun)
}

fn enqueue<T: Copy>(
    input: &[T],
    sender: &SyncSender<WriterCommand>,
    recycled: &Receiver<Vec<i16>>,
    failure: &SharedFailure,
    dropped_samples: &AtomicU64,
    channels: u16,
    convert: fn(T) -> i16,
) {
    if has_failure(failure) {
        return;
    }
    let chunk_samples = FRAMES_PER_CHUNK * usize::from(channels);
    for input_chunk in input.chunks(chunk_samples) {
        let mut chunk = take_buffer(recycled, chunk_samples);
        chunk.extend(input_chunk.iter().copied().map(convert));
        let dropped_before = dropped_samples.swap(0, Ordering::Relaxed);
        match sender.try_send(WriterCommand::Samples {
            samples: chunk,
            dropped_before,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(WriterCommand::Samples { mut samples, .. })) => {
                // The writer is behind. Drop this chunk, remember how much of
                // the timeline it covered, and keep the buffer for reuse.
                samples.clear();
                let _returned = recycled.try_iter().count();
                dropped_samples.fetch_add(
                    dropped_before.saturating_add(input_chunk.len() as u64),
                    Ordering::Relaxed,
                );
            }
            Err(TrySendError::Full(_)) => {
                dropped_samples.fetch_add(
                    dropped_before.saturating_add(input_chunk.len() as u64),
                    Ordering::Relaxed,
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                // The writer thread already reported whatever killed it; the
                // audio thread only needs the state to be visible to `stop`.
                record_failure(
                    failure,
                    "WAV_WRITER_FAILED",
                    "녹음 파일 writer가 예기치 않게 종료되었습니다.",
                );
                return;
            }
        }
    }
}

/// Take a buffer the writer has finished with, allocating only when the pool
/// is empty. Keeping allocation out of the steady state is what makes the
/// callback safe to run on the realtime audio thread.
fn take_buffer(recycled: &Receiver<Vec<i16>>, capacity: usize) -> Vec<i16> {
    match recycled.try_recv() {
        Ok(mut buffer) => {
            buffer.clear();
            buffer
        }
        Err(_) => Vec::with_capacity(capacity),
    }
}

fn convert<T>(sample: T) -> i16
where
    i16: FromSample<T>,
{
    i16::from_sample(sample)
}

pub(super) fn f32_to_i16(sample: f32) -> i16 {
    float_to_i16(f64::from(sample))
}

pub(super) fn f64_to_i16(sample: f64) -> i16 {
    float_to_i16(sample)
}

fn float_to_i16(sample: f64) -> i16 {
    if !sample.is_finite() {
        0
    } else if sample <= -1.0 {
        i16::MIN
    } else if sample >= 1.0 {
        i16::MAX
    } else {
        i16::from_sample(sample)
    }
}

fn _assert_sample_types(_: I24, _: U24) {}

#[cfg(test)]
mod tests {
    use super::enqueue;
    use crate::adapters::outbound::recording::failure;
    use crate::adapters::outbound::recording::writer::WriterCommand;
    use crate::application::error::AppError;
    use crate::application::model::RecordingFailure;
    use crate::application::ports::RecordingEvents;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::{channel, sync_channel};
    use uuid::Uuid;

    struct NoopEvents;

    impl RecordingEvents for NoopEvents {
        fn emit_failure(&self, _failure: RecordingFailure) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[test]
    fn counts_samples_instead_of_failing_when_writer_queue_is_full()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let (sender, _receiver) = sync_channel(1);
        sender.send(WriterCommand::Samples {
            samples: vec![1],
            dropped_before: 0,
        })?;
        let failure = failure::new(Uuid::now_v7(), Arc::new(NoopEvents));
        let dropped_samples = AtomicU64::new(0);
        let (_recycle, recycled) = channel();

        // When
        enqueue(
            &[2_i16, 3_i16],
            &sender,
            &recycled,
            &failure,
            &dropped_samples,
            1,
            |sample| sample,
        );

        // Then
        assert_eq!(dropped_samples.load(Ordering::Relaxed), 2);
        assert!(failure::take_failure(&failure)?.is_none());
        Ok(())
    }

    #[test]
    fn refills_a_recycled_buffer_instead_of_allocating() -> Result<(), Box<dyn std::error::Error>> {
        // Given: the writer returned a buffer it had finished with
        let (sender, receiver) = sync_channel(4);
        let (recycle, recycled) = channel();
        let mut spent = Vec::with_capacity(64);
        spent.push(9_i16);
        let address = spent.as_ptr();
        spent.clear();
        recycle.send(spent)?;
        let failure = failure::new(Uuid::now_v7(), Arc::new(NoopEvents));
        let dropped_samples = AtomicU64::new(0);

        // When
        enqueue(
            &[4_i16, 5_i16],
            &sender,
            &recycled,
            &failure,
            &dropped_samples,
            1,
            |sample| sample,
        );

        // Then: the queued chunk reuses that same allocation
        match receiver.recv()? {
            WriterCommand::Samples { samples, .. } => {
                assert_eq!(samples, [4, 5]);
                assert_eq!(samples.as_ptr(), address);
            }
            _ => unreachable!("enqueue only sends samples"),
        }
        Ok(())
    }
}
