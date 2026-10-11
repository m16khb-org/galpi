use serde::{Deserialize, Serialize};

/// What a recording captures. `Microphone` is the original behaviour; the
/// other source adds the computer's own output (the remote side of a video
/// call) so it ends up in the same file as the user's voice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingSource {
    #[default]
    Microphone,
    MicrophoneAndSystem,
}

impl RecordingSource {
    pub const fn includes_system_audio(self) -> bool {
        matches!(self, Self::MicrophoneAndSystem)
    }
}

#[cfg(test)]
mod tests {
    use super::RecordingSource;

    #[test]
    fn recording_source_defaults_to_the_microphone_and_round_trips_wire_names() {
        // Given / When / Then: the wire names are the camelCase the frontend sends
        assert!(matches!(
            serde_json::to_value(RecordingSource::default()),
            Ok(value) if value == serde_json::json!("microphone")
        ));
        assert!(matches!(
            serde_json::to_value(RecordingSource::MicrophoneAndSystem),
            Ok(value) if value == serde_json::json!("microphoneAndSystem")
        ));
        let parsed: Result<RecordingSource, _> =
            serde_json::from_value(serde_json::json!("microphoneAndSystem"));
        assert!(matches!(parsed, Ok(RecordingSource::MicrophoneAndSystem)));
    }

    #[test]
    fn recording_source_rejects_unknown_names() {
        let parsed: Result<RecordingSource, _> =
            serde_json::from_value(serde_json::json!("system"));
        assert!(parsed.is_err());
    }

    #[test]
    fn only_the_combined_source_includes_system_audio() {
        assert!(!RecordingSource::Microphone.includes_system_audio());
        assert!(RecordingSource::MicrophoneAndSystem.includes_system_audio());
    }
}
