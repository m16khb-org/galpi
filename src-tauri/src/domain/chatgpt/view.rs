//! What the settings sheet sends and receives. None of these types holds a
//! token, client id, or host id.

use super::AssistantAuthMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatGptAccountState {
    SignedOut,
    /// A registration is on file but no usable tokens: sign in again.
    SignInRequired,
    SignedIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptAccountView {
    pub state: ChatGptAccountState,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptSettings {
    pub auth_mode: AssistantAuthMode,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub welcome_acknowledged: bool,
    pub account: ChatGptAccountView,
}

/// `reasoning.effort` values offered for ChatGPT-plan minutes. The Responses
/// API also accepts `none` and `minimal`, which skip the deliberation minutes
/// benefit from; support for each value is model-dependent, and `None` lets
/// the model use its own default.
pub const CHATGPT_REASONING_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The choices the window owns and autosaves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptPreferences {
    #[serde(default)]
    pub auth_mode: AssistantAuthMode,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub welcome_acknowledged: bool,
}

impl ChatGptPreferences {
    /// Blank model ids drop out; an effort outside the offered set falls back
    /// to the model default rather than reaching the API as a 400.
    #[must_use]
    pub fn trimmed(self) -> Self {
        Self {
            model: self
                .model
                .map(|model| model.trim().to_owned())
                .filter(|model| !model.is_empty()),
            reasoning_effort: self
                .reasoning_effort
                .map(|effort| effort.trim().to_lowercase())
                .filter(|effort| CHATGPT_REASONING_EFFORTS.contains(&effort.as_str())),
            ..self
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptModel {
    pub slug: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignOutResult {
    pub settings: ChatGptSettings,
    pub remote_revocation_confirmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatGptSignInPhase {
    AwaitingBrowser,
    Exchanging,
}

/// Progress of a sign-in; the outcome arrives as the command result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptSignInEvent {
    pub phase: ChatGptSignInPhase,
}

#[cfg(test)]
mod tests {
    use super::{
        AssistantAuthMode, ChatGptAccountState, ChatGptAccountView, ChatGptPreferences,
        ChatGptSettings,
    };

    #[test]
    fn settings_serialize_without_any_credential() -> Result<(), serde_json::Error> {
        // Given
        let settings = ChatGptSettings {
            auth_mode: AssistantAuthMode::ChatGpt,
            model: Some("gpt-5".to_owned()),
            reasoning_effort: Some("high".to_owned()),
            welcome_acknowledged: true,
            account: ChatGptAccountView {
                state: ChatGptAccountState::SignedIn,
                email: Some("user@example.com".to_owned()),
            },
        };

        // When
        let value = serde_json::to_value(&settings)?;

        // Then
        let keys: Vec<&str> = value
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect())
            .unwrap_or_default();
        assert_eq!(
            keys,
            [
                "account",
                "authMode",
                "model",
                "reasoningEffort",
                "welcomeAcknowledged"
            ]
        );
        let account: Vec<&str> = value["account"]
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect())
            .unwrap_or_default();
        assert_eq!(account, ["email", "state"]);
        assert_eq!(value["authMode"], "chatGpt");
        assert_eq!(value["account"]["state"], "signedIn");
        Ok(())
    }

    #[test]
    fn trimming_normalizes_offered_efforts_and_drops_the_rest() {
        // Given / When / Then: casing and spaces are forgiven; values the
        // sheet does not offer (or the model-skipping `none`) become the
        // model default instead of a request the API would reject
        let effort = |value: &str| {
            ChatGptPreferences {
                reasoning_effort: Some(value.to_owned()),
                ..ChatGptPreferences::default()
            }
            .trimmed()
            .reasoning_effort
        };
        assert_eq!(effort(" XHigh ").as_deref(), Some("xhigh"));
        assert_eq!(effort("max").as_deref(), Some("max"));
        assert_eq!(effort("none"), None);
        assert_eq!(effort("extreme"), None);
        assert_eq!(effort(""), None);
    }
}
