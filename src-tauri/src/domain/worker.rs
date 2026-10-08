use serde::{Deserialize, Serialize};
use thiserror::Error;

const PROTOCOL_VERSION: u8 = 1;

/// Assistant failures the worker reports with a stable code the host forwards
/// as the job error, instead of a generic process failure.
pub const ASSISTANT_ERROR_CODES: [&str; 9] = [
    "CHATGPT_USAGE_LIMIT_EXCEEDED",
    "CHATGPT_USAGE_UNAVAILABLE",
    "CHATGPT_NOT_ELIGIBLE",
    "CHATGPT_UNSUPPORTED_REQUEST",
    "CHATGPT_AUTH_REJECTED",
    "CHATGPT_ACCESS_FORBIDDEN",
    "CHATGPT_UNAVAILABLE",
    "CHATGPT_RESPONSE_INCOMPLETE",
    "CHATGPT_STREAM_INTERRUPTED",
];

pub fn is_assistant_error_code(code: &str) -> bool {
    ASSISTANT_ERROR_CODES.contains(&code)
}

/// ASR biasing lists handed to the worker as one JSON object on disk.
/// Keys and list order are the wire contract read by `parse_asr_context`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AsrContext {
    pub terms: Vec<String>,
    pub names: Vec<String>,
    pub aliases: Vec<String>,
}

impl AsrContext {
    /// Build the biasing lists; `None` when nothing can bias recognition.
    pub fn new(terms: Vec<String>, names: Vec<String>, aliases: Vec<String>) -> Option<Self> {
        if terms.is_empty() && names.is_empty() && aliases.is_empty() {
            return None;
        }
        Some(Self {
            terms,
            names,
            aliases,
        })
    }

    /// Serialize in the wire format the worker's `parse_asr_context` reads:
    /// glossary terms first, then participant names, then spoken aliases.
    /// Consumed once on the way to the worker context file.
    pub fn into_wire_json(self) -> String {
        serde_json::json!({
            "terms": self.terms,
            "names": self.names,
            "aliases": self.aliases,
        })
        .to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum WorkerEvent {
    Phase {
        phase: String,
        percent: f32,
        message: String,
    },
    Completed {
        srt: String,
        txt: String,
        checkpoint: String,
        segments: usize,
        filtered: usize,
    },
    Prepared {
        engine_version: String,
    },
    Refined {
        minutes: String,
    },
    Error {
        code: String,
        message: String,
    },
    Log {
        stream: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkerEnvelope {
    pub v: u8,
    pub seq: u64,
    #[serde(flatten)]
    pub event: WorkerEvent,
}

#[derive(Debug, Error, PartialEq)]
pub enum ProtocolError {
    #[error("invalid worker event: {0}")]
    InvalidJson(String),
    #[error("unsupported worker protocol version {0}")]
    UnsupportedVersion(u8),
}

pub fn parse_worker_event(line: &str) -> Result<WorkerEnvelope, ProtocolError> {
    let envelope: WorkerEnvelope = serde_json::from_str(line)
        .map_err(|error| ProtocolError::InvalidJson(error.to_string()))?;
    if envelope.v != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(envelope.v));
    }
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::{ProtocolError, WorkerEvent, is_assistant_error_code, parse_worker_event};

    #[test]
    fn only_the_nine_assistant_codes_are_forwarded() {
        assert!(is_assistant_error_code("CHATGPT_USAGE_LIMIT_EXCEEDED"));
        assert!(is_assistant_error_code("CHATGPT_STREAM_INTERRUPTED"));
        assert_eq!(super::ASSISTANT_ERROR_CODES.len(), 9);
        assert!(!is_assistant_error_code("PROCESS_FAILED"));
        assert!(!is_assistant_error_code("CHATGPT_SIGN_IN_REQUIRED"));
        assert!(!is_assistant_error_code("chatgpt_usage_limit_exceeded"));
    }

    #[test]
    fn parses_phase_event_when_protocol_version_is_supported() -> Result<(), ProtocolError> {
        // Given
        let line = r#"{"v":1,"seq":7,"type":"phase","phase":"aligning","percent":42.5,"message":"정렬 중"}"#;

        // When
        let event = parse_worker_event(line)?;

        // Then
        assert_eq!(event.seq, 7);
        assert_eq!(
            event.event,
            WorkerEvent::Phase {
                phase: "aligning".to_owned(),
                percent: 42.5,
                message: "정렬 중".to_owned(),
            }
        );
        Ok(())
    }

    #[test]
    fn rejects_event_when_protocol_version_is_newer() {
        // Given
        let line = r#"{"v":2,"seq":1,"type":"phase","phase":"setup","percent":0,"message":"시작"}"#;

        // When / Then
        assert_eq!(
            parse_worker_event(line),
            Err(ProtocolError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn rejects_event_when_json_is_malformed() {
        // Given
        let line = r#"{"v":1,"seq":"broken"}"#;

        // When
        // When / Then
        assert!(matches!(
            parse_worker_event(line),
            Err(ProtocolError::InvalidJson(_))
        ));
    }

    #[test]
    fn asr_context_serializes_the_worker_wire_format() -> Result<(), serde_json::Error> {
        // Given
        let context = super::AsrContext {
            terms: vec!["갈피".to_owned()],
            names: vec!["하빈".to_owned()],
            aliases: vec!["하빈이".to_owned()],
        };

        // When
        let parsed: serde_json::Value = context.into_wire_json().parse()?;

        // Then: same keys and lists `parse_asr_context` reads on the worker side
        assert_eq!(parsed["terms"], serde_json::json!(["갈피"]));
        assert_eq!(parsed["names"], serde_json::json!(["하빈"]));
        assert_eq!(parsed["aliases"], serde_json::json!(["하빈이"]));
        Ok(())
    }

    #[test]
    fn asr_context_is_built_only_when_a_list_can_bias_recognition() {
        // Given / When / Then
        assert!(
            super::AsrContext::new(Vec::new(), Vec::new(), Vec::new()).is_none(),
            "an all-empty context must stay absent from the wire"
        );
        assert!(super::AsrContext::new(Vec::new(), vec!["하빈".to_owned()], Vec::new()).is_some());
    }
}
