//! Folding the microphone and system-audio tracks into the one WAV a meeting
//! keeps.
//!
//! This runs after the recording has stopped, on a blocking thread, never in an
//! audio callback: the two devices have unrelated clocks and rates, and mixing
//! them live would need exactly the synchronisation the callback must not do.

use super::writer::RIFF_DATA_LIMIT;
use crate::application::error::AppError;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::Path;

const CHUNK_FRAMES: usize = 4_096;

fn mix_error(detail: impl Display) -> AppError {
    AppError::new(
        "WAV_MIX_FAILED",
        format!("녹음 트랙을 하나로 합치지 못했습니다: {detail}"),
    )
}

/// One finished track, read one frame at a time.
struct Track {
    reader: WavReader<BufReader<File>>,
    channels: u16,
    sample_rate: u32,
}

impl Track {
    fn open(path: &Path) -> Result<Self, AppError> {
        let reader = WavReader::open(path).map_err(mix_error)?;
        let spec = reader.spec();
        if spec.sample_format != SampleFormat::Int
            || spec.bits_per_sample != 16
            || spec.channels == 0
            || spec.sample_rate == 0
        {
            return Err(mix_error("지원하지 않는 WAV 형식입니다."));
        }
        Ok(Self {
            reader,
            channels: spec.channels,
            sample_rate: spec.sample_rate,
        })
    }

    /// The next frame averaged to one sample, or `None` at the end. A partial
    /// frame at the very end, which a finished track never has, is ignored.
    fn next_frame(&mut self) -> Result<Option<i16>, AppError> {
        let mut sum = 0_i32;
        for _ in 0..self.channels {
            let Some(sample) = self.reader.samples::<i16>().next() else {
                return Ok(None);
            };
            sum += i32::from(sample.map_err(mix_error)?);
        }
        let mean = sum / i32::from(self.channels);
        i16::try_from(mean).map(Some).map_err(mix_error)
    }
}

/// A track resampled to another rate by linear interpolation, streaming.
///
/// Output frame `n` sits at source position `n * rate_in / rate_out`; the two
/// source frames around it are all that is kept. Speech that goes on to be
/// resampled to 16 kHz again does not need a better filter than this.
struct Resampled {
    track: Track,
    rate_in: i64,
    rate_out: i64,
    current: Option<i16>,
    following: Option<i16>,
    index: i64,
    produced: i64,
}

impl Resampled {
    fn new(mut track: Track, rate_out: u32) -> Result<Self, AppError> {
        let current = track.next_frame()?;
        let following = track.next_frame()?;
        Ok(Self {
            rate_in: i64::from(track.sample_rate),
            rate_out: i64::from(rate_out),
            track,
            current,
            following,
            index: 0,
            produced: 0,
        })
    }

    fn next_frame(&mut self) -> Result<Option<i16>, AppError> {
        let position = self.produced * self.rate_in;
        let wanted = position / self.rate_out;
        let fraction = position % self.rate_out;
        while self.index < wanted && self.current.is_some() {
            self.current = self.following;
            self.following = self.track.next_frame()?;
            self.index += 1;
        }
        let Some(current) = self.current else {
            return Ok(None);
        };
        self.produced += 1;
        let from = i64::from(current);
        // The last source frame has nothing after it to lean towards: hold it.
        let to = self.following.map_or(from, i64::from);
        let value = from + (to - from) * fraction / self.rate_out;
        i16::try_from(value).map(Some).map_err(mix_error)
    }
}

/// Mix `system` into `microphone` and write the mono result to `output`.
///
/// The output keeps the microphone's sample rate and is as long as the longer
/// track; the shorter one is padded with silence. The two tracks start a few
/// milliseconds apart because their streams open one after the other, and that
/// offset is not corrected. Returns the number of frames written.
pub fn mix_tracks(microphone: &Path, system: &Path, output: &Path) -> Result<u64, AppError> {
    let mut microphone = Track::open(microphone)?;
    let sample_rate = microphone.sample_rate;
    let mut system = Resampled::new(Track::open(system)?, sample_rate)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| AppError::io("합친 녹음 파일을 만들지 못했습니다", &error))?;
    let mut writer = WavWriter::new(
        BufWriter::new(file),
        WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        },
    )
    .map_err(mix_error)?;

    let mut frames = 0_u64;
    let mut chunk: Vec<i16> = Vec::with_capacity(CHUNK_FRAMES);
    loop {
        chunk.clear();
        while chunk.len() < CHUNK_FRAMES {
            let from_microphone = microphone.next_frame()?;
            let from_system = system.next_frame()?;
            if from_microphone.is_none() && from_system.is_none() {
                break;
            }
            // Speech from the two sources rarely overlaps, so the sum is not
            // halved (which would cost every voice 6 dB); it is clamped instead.
            let sum = i32::from(from_microphone.unwrap_or(0)) + i32::from(from_system.unwrap_or(0));
            let mixed = sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
            chunk.push(i16::try_from(mixed).map_err(mix_error)?);
        }
        if chunk.is_empty() {
            break;
        }
        frames += chunk.len() as u64;
        if frames * 2 > RIFF_DATA_LIMIT {
            return Err(AppError::new(
                "WAV_TOO_LARGE",
                "WAV 파일이 4GB 한도에 도달했습니다.",
            ));
        }
        let count = u32::try_from(chunk.len()).map_err(mix_error)?;
        let mut bulk = writer.get_i16_writer(count);
        for sample in &chunk {
            bulk.write_sample(*sample);
        }
        bulk.flush().map_err(mix_error)?;
    }
    writer.finalize().map_err(mix_error)?;
    Ok(frames)
}

#[cfg(test)]
#[path = "mix_tests.rs"]
mod tests;
