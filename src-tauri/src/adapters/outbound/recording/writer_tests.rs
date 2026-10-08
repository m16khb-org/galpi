use super::{WriterCommand, spawn};
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
    let writer = spawn(&path, 48_000, 2, failure)?;
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
    let writer = spawn(&path, 48_000, 1, failure)?;

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
    let writer = spawn(&partial, 48_000, 1, failure)?;
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
