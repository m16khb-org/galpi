//! Sign in with ChatGPT value objects.
//!
//! The registration is not secret. `ChatGptTokens` is, and never travels in
//! any IPC type: its `Debug` hides every value and only the secret store
//! serializes it.

mod view;

pub use view::{
    ChatGptAccountState, ChatGptAccountView, ChatGptModel, ChatGptPreferences, ChatGptSettings,
    ChatGptSignInEvent, ChatGptSignInPhase, SignOutResult,
};

use serde::{Deserialize, Serialize};
use std::fmt::{self, Debug, Formatter};
use uuid::Uuid;

/// Refresh once the access token has less than this many seconds left, so a
/// refinement spawned now keeps a usable token for at least this long.
pub const ACCESS_TOKEN_REFRESH_MARGIN_SECONDS: u64 = 20 * 60;
/// The scope that grants inference against the user's ChatGPT plan.
pub const PLAN_USAGE_SCOPE: &str = "chatgpt.tokens.use.direct";
/// The client id an agent host sends before it has an issued one.
pub const DYNAMIC_CLIENT_ID: &str = "dynamic_agent_client";
const ISSUED_CLIENT_PREFIX: &str = "oaiapp_";
const HOST_ID_PREFIX: &str = "urn:uuid:";

/// Which credential pays for minutes refinement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssistantAuthMode {
    #[default]
    ApiKey,
    ChatGpt,
}

/// Which wire protocol the assistant worker speaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AssistantTransport {
    #[default]
    ChatCompletions,
    Responses,
}

/// The persistent `ext_agent_host_id` that identifies this installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentHostId(String);

impl AgentHostId {
    pub fn generate() -> Self {
        Self(format!("{HOST_ID_PREFIX}{}", Uuid::new_v4()))
    }

    /// Accept only `urn:uuid:` followed by a version 4 UUID.
    pub fn parse(value: &str) -> Option<Self> {
        let uuid = Uuid::parse_str(value.strip_prefix(HOST_ID_PREFIX)?).ok()?;
        (uuid.get_version_num() == 4).then(|| Self(format!("{HOST_ID_PREFIX}{uuid}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The account and issued client this installation signed in with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatGptRegistration {
    client_id: String,
    subject: String,
    email: Option<String>,
    needs_consent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationError {
    /// Only an issued `oaiapp_` client id may be stored, never the dynamic one.
    ClientId,
    Subject,
}

impl ChatGptRegistration {
    pub fn new(
        client_id: String,
        subject: String,
        email: Option<String>,
        needs_consent: bool,
    ) -> Result<Self, RegistrationError> {
        let client_id = trimmed(client_id);
        if !client_id.starts_with(ISSUED_CLIENT_PREFIX)
            || client_id.len() == ISSUED_CLIENT_PREFIX.len()
        {
            return Err(RegistrationError::ClientId);
        }
        let subject = trimmed(subject);
        if subject.is_empty() {
            return Err(RegistrationError::Subject);
        }
        let email = email.map(trimmed).filter(|email| !email.is_empty());
        Ok(Self {
            client_id,
            subject,
            email,
            needs_consent,
        })
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn needs_consent(&self) -> bool {
        self.needs_consent
    }

    #[must_use]
    pub fn with_needs_consent(self, needs_consent: bool) -> Self {
        Self {
            needs_consent,
            ..self
        }
    }
}

/// Trim in place of reallocating when there is nothing to trim.
fn trimmed(value: String) -> String {
    if value.trim().len() == value.len() {
        value
    } else {
        value.trim().to_owned()
    }
}

/// OAuth tokens for the signed-in account. Serialized only by the secret store.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub scopes: Vec<String>,
    /// Unix seconds.
    pub expires_at: u64,
}

impl ChatGptTokens {
    pub fn grants_plan_usage(&self) -> bool {
        self.scopes.iter().any(|scope| scope == PLAN_USAGE_SCOPE)
    }

    pub fn needs_refresh(&self, now: u64, margin: u64) -> bool {
        now >= self.expires_at.saturating_sub(margin)
    }
}

impl Debug for ChatGptTokens {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChatGptTokens")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("scopes", &self.scopes)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// What a sign-in attempt starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInRequest {
    pub host_id: AgentHostId,
    /// `None` registers a new client; `Some` signs in again with the issued one.
    pub registration: Option<ChatGptRegistration>,
}

/// A completed sign-in. Tokens carry the scopes the server actually granted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInGrant {
    pub registration: ChatGptRegistration,
    pub tokens: ChatGptTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshFailure {
    /// The refresh token can never work again: sign in again.
    SessionUnusable,
    /// The server rejected the issued client.
    ClientRejected,
    /// Network or server trouble; nothing should be discarded.
    Transient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationOutcome {
    Confirmed,
    Unconfirmed,
}

#[cfg(test)]
mod tests {
    use super::{
        AgentHostId, ChatGptRegistration, ChatGptTokens, DYNAMIC_CLIENT_ID, PLAN_USAGE_SCOPE,
        RegistrationError,
    };

    fn tokens(scopes: &[&str], expires_at: u64) -> ChatGptTokens {
        ChatGptTokens {
            access_token: "access-value-1".to_owned(),
            refresh_token: "refresh-value-1".to_owned(),
            scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
            expires_at,
        }
    }

    #[test]
    fn generated_host_id_is_a_version_four_uuid_urn() {
        let id = AgentHostId::generate();

        assert!(id.as_str().starts_with("urn:uuid:"));
        assert_eq!(AgentHostId::parse(id.as_str()), Some(id));
    }

    #[test]
    fn host_id_parse_rejects_other_shapes() {
        assert_eq!(AgentHostId::parse(""), None);
        assert_eq!(
            AgentHostId::parse("6f9619ff-8b86-4d11-b42d-00c04fc964ff"),
            None
        );
        assert_eq!(AgentHostId::parse("urn:uuid:not-a-uuid"), None);
        // A version 7 UUID is a valid UUID but not the version this host mints.
        assert_eq!(
            AgentHostId::parse("urn:uuid:01890a5d-ac96-774b-bcce-b302099a8057"),
            None
        );
    }

    #[test]
    fn refresh_is_needed_from_the_margin_before_expiry() {
        let tokens = tokens(&[], 10_000);

        assert!(!tokens.needs_refresh(8_799, 1_200));
        assert!(tokens.needs_refresh(8_800, 1_200));
        assert!(tokens.needs_refresh(20_000, 1_200));
        assert!(
            super::ChatGptTokens {
                expires_at: 5,
                ..tokens
            }
            .needs_refresh(0, 1_200)
        );
    }

    #[test]
    fn plan_usage_requires_the_direct_use_scope() {
        assert!(tokens(&["openid", PLAN_USAGE_SCOPE], 0).grants_plan_usage());
        assert!(!tokens(&["openid", "resource.invoke"], 0).grants_plan_usage());
    }

    #[test]
    fn debug_output_hides_token_values() {
        let printed = format!("{:?}", tokens(&["openid"], 42));

        assert!(!printed.contains("access-value-1"));
        assert!(!printed.contains("refresh-value-1"));
        assert!(printed.contains("expires_at: 42"));
    }

    #[test]
    fn registration_rejects_the_dynamic_client_and_empty_subjects() {
        assert_eq!(
            ChatGptRegistration::new(DYNAMIC_CLIENT_ID.to_owned(), "sub".to_owned(), None, false),
            Err(RegistrationError::ClientId)
        );
        assert_eq!(
            ChatGptRegistration::new("oaiapp_".to_owned(), "sub".to_owned(), None, false),
            Err(RegistrationError::ClientId)
        );
        assert_eq!(
            ChatGptRegistration::new("oaiapp_abc".to_owned(), " ".to_owned(), None, false),
            Err(RegistrationError::Subject)
        );
        let registration = ChatGptRegistration::new(
            "oaiapp_abc".to_owned(),
            "sub".to_owned(),
            Some(" ".to_owned()),
            false,
        );
        assert_eq!(registration.map(|value| value.email().is_none()), Ok(true));
    }
}
