use super::mix_tracks;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::path::{Path, PathBuf};
use uuid::Uuid;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, std::io::Error> {
        let directory = std::env::temp_dir().join(format!("galpi-mix-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&directory)?;
        Ok(Self(directory))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _removed = std::fs::remove_dir_all(&self.0);
    }
}

fn write_track(
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

struct Mixed {
    sample_rate: u32,
    channels: u16,
    samples: Vec<i16>,
    frames: u64,
}

fn mix(
    scratch: &Scratch,
    microphone: (u32, u16, &[i16]),
    system: (u32, &[i16]),
) -> Result<Mixed, Box<dyn std::error::Error>> {
    let microphone_path = scratch.path("mic.wav");
    let system_path = scratch.path("system.wav");
    let output = scratch.path("mixed.wav");
    write_track(&microphone_path, microphone.0, microphone.1, microphone.2)?;
    write_track(&system_path, system.0, 1, system.1)?;
    let frames = mix_tracks(&microphone_path, &system_path, &output)?;
    let mut reader = WavReader::open(&output)?;
    let spec = reader.spec();
    Ok(Mixed {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        samples: reader.samples::<i16>().collect::<Result<_, _>>()?,
        frames,
    })
}

#[test]
fn sums_two_tracks_at_the_same_rate_into_one_mono_file() -> TestResult {
    // Given
    let scratch = Scratch::new()?;

    // When
    let mixed = mix(&scratch, (48_000, 1, &[1, 2, 3]), (48_000, &[10, 20, 30]))?;

    // Then
    assert_eq!(mixed.samples, [11, 22, 33]);
    assert_eq!(mixed.frames, 3);
    assert_eq!((mixed.sample_rate, mixed.channels), (48_000, 1));
    Ok(())
}

#[test]
fn averages_a_stereo_microphone_to_mono_before_adding_the_system_track() -> TestResult {
    // Given: two stereo microphone frames, (100, 300) and (200, 400)
    let scratch = Scratch::new()?;

    // When
    let mixed = mix(
        &scratch,
        (48_000, 2, &[100, 300, 200, 400]),
        (48_000, &[5, 5]),
    )?;

    // Then
    assert_eq!(mixed.samples, [205, 305]);
    Ok(())
}

#[test]
fn pads_the_shorter_track_with_silence_in_either_direction() -> TestResult {
    // Given / When: the microphone outlasts the system track, then the reverse
    let longer_microphone = mix(
        &Scratch::new()?,
        (48_000, 1, &[1, 2, 3, 4]),
        (48_000, &[10]),
    )?;
    let longer_system = mix(&Scratch::new()?, (48_000, 1, &[1]), (48_000, &[10, 20, 30]))?;

    // Then
    assert_eq!(longer_microphone.samples, [11, 2, 3, 4]);
    assert_eq!(longer_system.samples, [11, 20, 30]);
    Ok(())
}

#[test]
fn an_empty_system_track_leaves_the_microphone_untouched() -> TestResult {
    // Given / When
    let mixed = mix(&Scratch::new()?, (48_000, 1, &[5, 6]), (48_000, &[]))?;

    // Then
    assert_eq!(mixed.samples, [5, 6]);
    Ok(())
}

#[test]
fn clamps_overlapping_loud_samples_instead_of_wrapping() -> TestResult {
    // Given / When
    let mixed = mix(
        &Scratch::new()?,
        (48_000, 1, &[30_000, -30_000]),
        (48_000, &[30_000, -30_000]),
    )?;

    // Then
    assert_eq!(mixed.samples, [i16::MAX, i16::MIN]);
    Ok(())
}

#[test]
fn upsamples_a_slower_system_track_linearly_to_the_microphone_rate() -> TestResult {
    // Given: a 24 kHz system track against a 48 kHz microphone of silence
    let scratch = Scratch::new()?;

    // When
    let mixed = mix(&scratch, (48_000, 1, &[0; 6]), (24_000, &[0, 100, 200]))?;

    // Then: each source gap is halved, and the last source frame is held
    assert_eq!(mixed.samples, [0, 50, 100, 150, 200, 200]);
    Ok(())
}

#[test]
fn downsampling_a_48k_system_track_to_a_44_1k_microphone_keeps_its_duration() -> TestResult {
    // Given: one second of each, at different rates
    let scratch = Scratch::new()?;
    let microphone = vec![0_i16; 44_100];
    let system = vec![1_000_i16; 48_000];

    // When
    let mixed = mix(&scratch, (44_100, 1, &microphone), (48_000, &system))?;

    // Then: still one second at the microphone's rate, with the level intact
    assert_eq!(mixed.frames, 44_100);
    assert_eq!(mixed.sample_rate, 44_100);
    assert!(mixed.samples.iter().all(|sample| *sample == 1_000));
    Ok(())
}

#[test]
fn a_track_that_is_not_16_bit_pcm_is_refused_with_a_stable_code() -> TestResult {
    // Given: a 32-bit float "system" track
    let scratch = Scratch::new()?;
    let microphone = scratch.path("mic.wav");
    let system = scratch.path("system.wav");
    write_track(&microphone, 48_000, 1, &[1])?;
    let mut float = WavWriter::create(
        &system,
        WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        },
    )?;
    float.write_sample(0.5_f32)?;
    float.finalize()?;

    // When
    let result = mix_tracks(&microphone, &system, &scratch.path("mixed.wav"));

    // Then
    assert_eq!(
        result.err().map(|error| error.code),
        Some("WAV_MIX_FAILED".to_owned())
    );
    Ok(())
}
