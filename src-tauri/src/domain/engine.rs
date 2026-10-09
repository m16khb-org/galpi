use serde::{Deserialize, Serialize};

/// Selectable transcription engine preset. `Qwen3` is the default; the legacy
/// `WhisperX` stack stays selectable as the previous engine set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnginePreset {
    #[default]
    Qwen3,
    WhisperX,
}

impl EnginePreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Qwen3 => "qwen3",
            Self::WhisperX => "whisperx",
        }
    }
}

/// Which `PyTorch` build the `WhisperX` environment is installed with.
///
/// This is an install-time choice, not the runtime device: the worker still
/// picks `cuda`/`mps`/`cpu` itself from the installed torch. Platforms without
/// a choice (macOS) always use `Cpu`, meaning the standard lock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComputeDevice {
    #[default]
    Cpu,
    Cuda,
}

/// The engine a readiness check or installation is about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineSelection {
    pub preset: EnginePreset,
    pub device: ComputeDevice,
}

#[cfg(test)]
mod tests {
    use super::{ComputeDevice, EnginePreset, EngineSelection};

    #[test]
    fn defaults_to_qwen3_and_round_trips_wire_names() {
        // Given / When / Then: fresh installs start on the candidate set
        assert!(matches!(
            serde_json::to_value(EnginePreset::default()),
            Ok(value) if value == serde_json::json!("qwen3")
        ));
        let parsed: Result<EnginePreset, _> = serde_json::from_value(serde_json::json!("whisperx"));
        assert!(matches!(parsed, Ok(EnginePreset::WhisperX)));
    }

    #[test]
    fn rejects_unknown_preset_names() {
        let parsed: Result<EnginePreset, _> = serde_json::from_value(serde_json::json!("turbo"));
        assert!(parsed.is_err());
    }

    #[test]
    fn compute_devices_round_trip_wire_names_and_reject_unknown_ones() {
        // Given / When / Then
        assert!(matches!(
            serde_json::to_value(ComputeDevice::default()),
            Ok(value) if value == serde_json::json!("cpu")
        ));
        let cuda: Result<ComputeDevice, _> = serde_json::from_value(serde_json::json!("cuda"));
        assert!(matches!(cuda, Ok(ComputeDevice::Cuda)));
        let unknown: Result<ComputeDevice, _> = serde_json::from_value(serde_json::json!("mps"));
        assert!(unknown.is_err());
    }

    #[test]
    fn a_selection_pairs_a_preset_with_a_device() {
        let selection = EngineSelection {
            preset: EnginePreset::WhisperX,
            device: ComputeDevice::Cuda,
        };
        assert_eq!(selection.preset, EnginePreset::WhisperX);
        assert_eq!(selection.device, ComputeDevice::Cuda);
        assert_eq!(EngineSelection::default().preset, EnginePreset::Qwen3);
    }
}
