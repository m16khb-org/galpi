use crate::domain::artifact::Artifacts;
use crate::domain::engine::{ComputeDevice, EnginePreset};
use serde::Serialize;
use uuid::Uuid;

// Readiness travels as flat booleans because it mirrors the wire contract
// the frontend parses one-to-one; nesting would churn the IPC schema.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStatus {
    pub engine_preset: EnginePreset,
    pub engine_ready: bool,
    pub models_ready: bool,
    pub ffmpeg_ready: bool,
    pub qwen3_ready: bool,
    pub whisperx_ready: bool,
    pub data_directory: String,
    pub default_output_directory: String,
    pub engine_version: String,
    pub compute_device: ComputeDevice,
    /// Presets this platform offers; the first one is its default.
    pub available_presets: Vec<EnginePreset>,
    /// Devices the user may choose between; empty means there is no choice.
    pub available_devices: Vec<ComputeDevice>,
    pub cuda_driver_detected: bool,
}

impl EnvironmentStatus {
    pub fn is_ready(&self) -> bool {
        self.engine_ready && self.models_ready && self.ffmpeg_ready
    }
}

#[derive(Debug)]
pub struct CompletedTranscription {
    pub artifacts: Artifacts,
    pub segments: usize,
    pub filtered: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefinementResult {
    pub job_id: Uuid,
    pub minutes: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub job_id: Uuid,
    pub status: EnvironmentStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionResult {
    pub job_id: Uuid,
    pub srt: String,
    pub txt: String,
    pub checkpoint: Option<String>,
    pub output_directory: String,
    pub segments: usize,
    pub filtered: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptImportResult {
    pub job_id: Uuid,
    pub txt: String,
    pub output_directory: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub recording_id: Uuid,
    pub path: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    pub recording_id: Uuid,
    pub path: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub dropped_frames: u64,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingFailure {
    pub recording_id: Uuid,
    pub code: String,
    pub message: String,
}
