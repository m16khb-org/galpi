use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum SpeakerHint {
    Auto,
    Exact { count: u8 },
    Range { min: u8, max: u8 },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRequest {
    /// Chosen by the window before the job starts, so every event this job
    /// emits can be told apart from a previous job's trailing events.
    pub job_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionRequest {
    pub job_id: Uuid,
    pub input_path: String,
    pub output_root: String,
    pub speaker_hint: SpeakerHint,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptImportRequest {
    pub job_id: Uuid,
    pub input_path: String,
    pub output_root: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RequestError {
    #[error("speaker count must be greater than zero")]
    InvalidSpeakerCount,
    #[error("speaker range minimum must not exceed maximum")]
    InvalidSpeakerRange,
}

pub fn validate_speaker_hint(hint: &SpeakerHint) -> Result<(), RequestError> {
    match hint {
        SpeakerHint::Exact { count: 0 } => Err(RequestError::InvalidSpeakerCount),
        SpeakerHint::Range { min: 0, .. } | SpeakerHint::Range { max: 0, .. } => {
            Err(RequestError::InvalidSpeakerCount)
        }
        SpeakerHint::Range { min, max } if min > max => Err(RequestError::InvalidSpeakerRange),
        SpeakerHint::Auto | SpeakerHint::Exact { .. } | SpeakerHint::Range { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{RequestError, SpeakerHint, validate_speaker_hint};

    #[test]
    fn rejects_exact_speaker_hint_when_count_is_zero() {
        // Given
        let hint = SpeakerHint::Exact { count: 0 };

        // When
        let result = validate_speaker_hint(&hint);

        // Then
        assert_eq!(result, Err(RequestError::InvalidSpeakerCount));
    }

    #[test]
    fn rejects_speaker_range_when_minimum_exceeds_maximum() {
        // Given
        let hint = SpeakerHint::Range { min: 4, max: 2 };

        // When
        let result = validate_speaker_hint(&hint);

        // Then
        assert_eq!(result, Err(RequestError::InvalidSpeakerRange));
    }

    #[test]
    fn accepts_supported_speaker_hints() {
        // Given
        let hints = [
            SpeakerHint::Auto,
            SpeakerHint::Exact { count: 3 },
            SpeakerHint::Range { min: 2, max: 6 },
        ];

        // When / Then
        for hint in hints {
            assert_eq!(validate_speaker_hint(&hint), Ok(()));
        }
    }
}
