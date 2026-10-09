//! The auth-gateway session that unlocks the app.
//!
//! `GatewayTokens` is secret and never travels in any IPC type: its `Debug`
//! hides every value and only the secret store serializes it. `AppAccess` is
//! what the window sees: the sign-in state and, at most, an email.

use serde::{Deserialize, Serialize};
use std::fmt::{self, Debug, Formatter};

/// The Supabase session the gateway issued. Serialized only by the secret store.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayTokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: u64,
}

impl Debug for GatewayTokens {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GatewayTokens")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// A completed browser sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayGrant {
    pub tokens: GatewayTokens,
    /// `None` when the gateway could not say who signed in; the sign-in stands.
    pub email: Option<String>,
}

/// Whether the app may be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum AppAccess {
    SignedOut,
    SignedIn {
        email: Option<String>,
        /// The gateway could not be reached at startup, so the stored session
        /// was trusted without being renewed.
        offline: bool,
    },
}

/// Why renewing the stored session failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayRefreshFailure {
    /// The gateway refused the session (`invalid_grant`): sign in again.
    Rejected,
    /// Anything else: network, server, or an unexpected answer. Keep the session.
    Unavailable,
}

#[cfg(test)]
mod tests {
    use super::{AppAccess, GatewayTokens};
    use serde_json::json;

    #[test]
    fn app_access_serializes_only_state_and_email() -> Result<(), serde_json::Error> {
        // Given
        let signed_in = AppAccess::SignedIn {
            email: Some("user@example.com".to_owned()),
            offline: true,
        };

        // When
        let signed_in = serde_json::to_value(&signed_in)?;
        let signed_out = serde_json::to_value(AppAccess::SignedOut)?;

        // Then
        assert_eq!(
            signed_in,
            json!({"state": "signedIn", "email": "user@example.com", "offline": true})
        );
        assert_eq!(signed_out, json!({"state": "signedOut"}));
        Ok(())
    }

    #[test]
    fn gateway_tokens_debug_hides_every_token() {
        let tokens = GatewayTokens {
            access_token: "galpi-test-access".to_owned(),
            refresh_token: "galpi-test-refresh".to_owned(),
            expires_at: 7,
        };

        let printed = format!("{tokens:?}");

        assert!(!printed.contains("galpi-test"));
        assert!(printed.contains("expires_at: 7"));
    }
}
