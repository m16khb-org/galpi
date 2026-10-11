use super::{WriterCommand, downmix_to_mono, spawn};
use crate::adapters::outbound::recording::cleanup::cancel_and_remove;
use crate::adapters::outbound::recording::failure;
use crate::application::error::AppError;
use crate::application::model::RecordingFailure;
use crate::application::ports::RecordingEvents;
use hound::WavReader;
use std::sync::Arc;
use uuid::Uuid;

struct NoopEvents;

impl RecordingEvents for NoopEvents {
    fn emit_failure(&self, _failure: RecordingFailure) -> Result<(), AppError> {
        Ok(())
    }
}

#[test]
fn writes_exact_pcm_samples_and_header() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("galpi-writer-{}.wav", Uuid::now_v7()));
    let failure = failure::new(Uuid::now_v7(), Arc::new(NoopEvents));
    let writer = spawn(&path, 48_000, 2, false, failure)?;
    writer.sender().send(WriterCommand::Samples {
        samples: vec![-32_768, 0, 32_767, 1],
        dropped_before: 0,
    })?;
    let summary = writer.finish(0)?;
    let mut reader = WavReader::open(&path)?;
    let spec = reader.spec();
    let samples = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
    std::fs::remove_file(path)?;

    assert_eq!(summary.samples, 4);
    assert_eq!(spec.channels, 2);
    assert_eq!(spec.sample_rate, 48_000);
    assert_eq!(samples, [-32_768, 0, 32_767, 1]);
    Ok(())
}

#[test]
fn fills_dropped_samples_with_silence_and_reports_them() -> Result<(), Box<dyn std::error::Error>> {
    // Given
    let path = std::env::temp_dir().join(format!("galpi-writer-{}.wav", Uuid::now_v7()));
    let failure = failure::new(Uuid::now_v7(), Arc::new(NoopEvents));
    let writer = spawn(&path, 48_000, 1, false, failure)?;

    // When
    writer.sender().send(WriterCommand::Samples {
        samples: vec![1, 2],
        dropped_before: 2,
    })?;
    let summary = writer.finish(2)?;
    let mut reader = WavReader::open(&path)?;
    let samples = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
    std::fs::remove_file(path)?;

    // Then
    assert_eq!(summary.samples, 6);
    assert_eq!(summary.dropped_samples, 4);
    assert_eq!(samples, [0, 0, 1, 2, 0, 0]);
    Ok(())
}

#[test]
fn cancelling_a_recording_leaves_no_partial_file_or_empty_folder()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a recording in progress inside its own meeting folder
    let folder = std::env::temp_dir().join(format!("galpi-cancel-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&folder)?;
    let partial = folder.join("x.wav.part");
    let failure = failure::new(Uuid::now_v7(), Arc::new(NoopEvents));
    let writer = spawn(&partial, 48_000, 1, false, failure)?;
    writer.sender().send(WriterCommand::Samples {
        samples: vec![1, 2, 3, 4],
        dropped_before: 0,
    })?;
    assert!(partial.exists());

    // When: the recording is cancelled (the writer must release its handle
    // first, or Windows refuses to delete the file)
    cancel_and_remove(writer, &partial, &folder)?;

    // Then: neither the partial file nor the empty folder is left
    assert!(!partial.exists());
    assert!(!folder.exists());
    Ok(())
}

#[test]
fn a_mono_track_averages_each_frame_and_counts_drops_in_frames()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a 7.1 output device (8 interleaved channels) saved as one channel
    let path = std::env::temp_dir().join(format!("galpi-writer-{}.wav", Uuid::now_v7()));
    let failure = failure::new(Uuid::now_v7(), Arc::new(NoopEvents));
    let writer = spawn(&path, 48_000, 8, true, failure)?;

    // When: two frames arrive after two dropped frames (16 samples), then one
    // more frame (8 samples) is dropped at the end
    writer.sender().send(WriterCommand::Samples {
        samples: vec![8, 8, 8, 8, 8, 8, 8, 8, 0, 0, 0, 0, 0, 0, 0, 16],
        dropped_before: 16,
    })?;
    let summary = writer.finish(8)?;
    let mut reader = WavReader::open(&path)?;
    let spec = reader.spec();
    let samples = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
    std::fs::remove_file(path)?;

    // Then: one channel, and drops are reported as frames rather than samples
    assert_eq!(spec.channels, 1);
    assert_eq!(samples, [0, 0, 8, 2, 0]);
    assert_eq!(summary.samples, 5);
    assert_eq!(summary.dropped_samples, 3);
    Ok(())
}

#[test]
fn downmix_averages_frames_in_place_and_drops_a_partial_trailing_frame() {
    // Given: three stereo frames plus one stray sample
    let mut chunk = vec![100_i16, 300, -100, -300, 32_767, 32_767, 5];

    // When
    downmix_to_mono(&mut chunk, 2);

    // Then: the sum is taken in 32 bits, so full-scale input cannot overflow
    assert_eq!(chunk, [200, -200, 32_767]);
}
