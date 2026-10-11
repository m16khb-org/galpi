use super::capture::{f32_to_i16, f64_to_i16, is_recoverable_stream_error};
use super::cleanup::{remove_partials, system_audio_unavailable};
use super::{Files, Stopped, assemble};
use crate::adapters::outbound::platform::Os;
use crate::domain::recording::RecordingSource;
use cpal::ErrorKind;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::path::{Path, PathBuf};
use uuid::Uuid;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn meeting_folder() -> Result<PathBuf, std::io::Error> {
    let folder = std::env::temp_dir().join(format!("galpi-recording-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&folder)?;
    Ok(folder)
}

fn write_wav(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[i16],
) -> Result<(), hound::Error> {
    let mut writer = WavWriter::create(
        path,
        WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        },
    )?;
    for sample in samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()
}

#[test]
fn a_microphone_only_recording_has_one_partial_file() {
    // Given / When
    let files = Files::plan(Path::new("/m"), "meeting", RecordingSource::Microphone);

    // Then: the one file is renamed in place, exactly as before system audio existed
    assert_eq!(files.parts(), [files.merged_part.as_path()]);
    assert_eq!(files.microphone_part, files.merged_part);
    assert!(files.system_part.is_none());
    assert_eq!(files.final_path, Path::new("/m").join("meeting.wav"));
}

#[test]
fn a_system_audio_recording_keeps_each_track_in_its_own_partial_file() {
    // Given / When
    let files = Files::plan(
        Path::new("/m"),
        "meeting",
        RecordingSource::MicrophoneAndSystem,
    );

    // Then: the merged file is not one of the track files
    let parts = files.parts();
    let names: Vec<_> = parts
        .iter()
        .filter_map(|part| part.file_name()?.to_str())
        .collect();
    assert_eq!(
        names,
        [
            "meeting.mic.wav.part",
            "meeting.system.wav.part",
            "meeting.wav.part"
        ]
    );
}

#[test]
fn finishing_a_microphone_only_recording_renames_its_partial_file() -> TestResult {
    // Given: a stopped microphone-only recording with two stereo frames
    let folder = meeting_folder()?;
    let files = Files::plan(&folder, "meeting", RecordingSource::Microphone);
    write_wav(&files.microphone_part, 48_000, 2, &[1, 2, 3, 4])?;
    let stopped = Stopped {
        sample_rate: 48_000,
        channels: 2,
        microphone_samples: 4,
        microphone_dropped_samples: 2,
        system: None,
    };

    // When
    let audio = assemble(&files, &stopped)?;

    // Then: same shape as before, and only the final file remains
    assert_eq!(
        (audio.channels, audio.frames, audio.dropped_frames),
        (2, 2, 1)
    );
    assert!(files.final_path.exists());
    assert!(!files.microphone_part.exists());
    std::fs::remove_dir_all(folder)?;
    Ok(())
}

#[test]
fn finishing_a_system_audio_recording_leaves_one_mono_wav_and_no_partial_files() -> TestResult {
    // Given: a stereo 44.1 kHz microphone track and a mono 48 kHz system track
    let folder = meeting_folder()?;
    let files = Files::plan(&folder, "meeting", RecordingSource::MicrophoneAndSystem);
    write_wav(&files.microphone_part, 44_100, 2, &[100; 8])?;
    let system_part = files.system_part.clone().ok_or("system part")?;
    write_wav(&system_part, 48_000, 1, &[50; 4])?;
    let stopped = Stopped {
        sample_rate: 44_100,
        channels: 2,
        microphone_samples: 8,
        microphone_dropped_samples: 4,
        system: Some((system_part.clone(), 3)),
    };

    // When
    let audio = assemble(&files, &stopped)?;

    // Then: mono at the microphone's rate, drops summed as frames (4 / 2 + 3)
    let reader = WavReader::open(&files.final_path)?;
    assert_eq!(
        (reader.spec().channels, reader.spec().sample_rate),
        (1, 44_100)
    );
    assert_eq!(
        (audio.channels, audio.sample_rate, audio.dropped_frames),
        (1, 44_100, 5)
    );
    assert_eq!(audio.frames, u64::from(reader.duration()));
    assert!(!files.microphone_part.exists());
    assert!(!system_part.exists());
    assert!(!files.merged_part.exists());
    std::fs::remove_dir_all(folder)?;
    Ok(())
}

#[test]
fn a_merge_that_fails_leaves_no_partial_file_and_no_meeting_folder() -> TestResult {
    // Given: the system track never made it to disk
    let folder = meeting_folder()?;
    let files = Files::plan(&folder, "meeting", RecordingSource::MicrophoneAndSystem);
    write_wav(&files.microphone_part, 48_000, 1, &[1, 2])?;
    let missing = files.system_part.clone().ok_or("system part")?;
    let stopped = Stopped {
        sample_rate: 48_000,
        channels: 1,
        microphone_samples: 2,
        microphone_dropped_samples: 0,
        system: Some((missing, 0)),
    };

    // When: the merge fails and the recorder cleans up the way stop does
    let error = assemble(&files, &stopped).err();
    files.remove_parts()?;

    // Then
    assert_eq!(
        error.map(|error| error.code),
        Some("WAV_MIX_FAILED".to_owned())
    );
    assert!(!folder.exists(), "meeting folder was left behind");
    Ok(())
}

#[test]
fn removing_partials_deletes_every_file_even_when_one_is_already_gone() -> TestResult {
    // Given: two partial files and one that was never created
    let folder = meeting_folder()?;
    let first = folder.join("a.wav.part");
    let second = folder.join("b.wav.part");
    let never_written = folder.join("c.wav.part");
    std::fs::write(&first, b"x")?;
    std::fs::write(&second, b"x")?;

    // When
    remove_partials(&[&first, &never_written, &second], &folder)?;

    // Then: the folder goes with them
    assert!(!folder.exists());
    Ok(())
}

#[test]
fn unavailable_system_audio_names_its_code_and_only_its_own_platforms_remedy() {
    // Given / When
    let mac = system_audio_unavailable(Os::MacOs, "권한 없음");
    let windows = system_audio_unavailable(Os::Windows, "장치 없음");

    // Then: stable code, microphone-only reassurance, and no foreign-OS advice
    assert_eq!(mac.code, "SYSTEM_AUDIO_UNAVAILABLE");
    assert_eq!(windows.code, "SYSTEM_AUDIO_UNAVAILABLE");
    assert!(mac.message.contains("14.6"));
    assert!(!windows.message.contains("macOS"));
    assert!(mac.message.contains("마이크만 녹음은 계속 쓸 수 있습니다."));
    assert!(
        windows
            .message
            .contains("마이크만 녹음은 계속 쓸 수 있습니다.")
    );
}

#[test]
fn converts_and_clamps_float_samples_for_pcm_wav() {
    assert_eq!(f32_to_i16(-2.0), i16::MIN);
    assert_eq!(f32_to_i16(-1.0), i16::MIN);
    assert_eq!(f32_to_i16(0.0), 0);
    assert_eq!(f32_to_i16(1.0), i16::MAX);
    assert_eq!(f32_to_i16(2.0), i16::MAX);
    assert_eq!(f32_to_i16(f32::NAN), 0);
    assert_eq!(f64_to_i16(f64::INFINITY), 0);
}

#[test]
fn treats_core_audio_xrun_as_recoverable() {
    assert!(is_recoverable_stream_error(ErrorKind::Xrun));
    assert!(!is_recoverable_stream_error(ErrorKind::DeviceNotAvailable));
}

/// Skipped where `pmset` is unavailable or restricted (sandboxed CI images);
/// run it with `cargo test -- --ignored` on a real desktop session.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "reads the live system assertion table through pmset"]
fn sleep_blocker_holds_and_releases_a_system_sleep_assertion() -> Result<(), String> {
    // Given: a distinctive assertion name to find in the system assertion table
    let name = "galpi-test-recording-sleep-blocker";

    // When: the blocker is held
    let blocker = super::power::SleepBlocker::acquire(name)
        .ok_or("power assertion should be created on macOS")?;
    let held = pmset_assertions()?;

    // Then: the system reports the named assertion while held and drops it on release
    assert!(held.contains(name), "assertion missing while held:\n{held}");
    drop(blocker);
    let released = pmset_assertions()?;
    assert!(
        !released.contains(name),
        "assertion leaked after drop:\n{released}"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn pmset_assertions() -> Result<String, String> {
    let output = std::process::Command::new("pmset")
        .args(["-g", "assertions"])
        .output()
        .map_err(|error| format!("pmset should run on macOS: {error}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Needs administrator rights for `powercfg /requests`; run it explicitly with
/// `cargo test -- --ignored` from an elevated shell.
#[cfg(windows)]
#[test]
#[ignore = "reads the live power requests through powercfg, which needs administrator rights"]
fn sleep_blocker_reason_is_listed_by_powercfg_while_held() -> Result<(), String> {
    // Given
    let name = "galpi-test-recording-sleep-blocker";

    // When
    let blocker = super::power::SleepBlocker::acquire(name)
        .ok_or("a power request should be created on Windows")?;
    let output = std::process::Command::new("powercfg")
        .arg("/requests")
        .output()
        .map_err(|error| format!("powercfg should run on Windows: {error}"))?;
    let held = String::from_utf8_lossy(&output.stdout).into_owned();
    drop(blocker);

    // Then
    assert!(held.contains(name), "request missing while held:\n{held}");
    Ok(())
}

#[cfg(windows)]
#[test]
fn sleep_blocker_is_acquired_and_released() -> Result<(), String> {
    // Given / When
    let blocker = super::power::SleepBlocker::acquire("galpi-test-sleep-blocker")
        .ok_or("a power request should be created on Windows")?;

    // Then: dropping from another thread, as the recorder does, is fine
    std::thread::spawn(move || drop(blocker))
        .join()
        .map_err(|_| "dropping the blocker panicked".to_owned())
}
