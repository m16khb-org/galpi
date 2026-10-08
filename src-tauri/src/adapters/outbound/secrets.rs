//! Secret storage for the two credentials Galpi holds.
//!
//! The destination is the macOS Keychain, and [`Keychain`] implements it. It is
//! not what the app uses yet: macOS ties access to a Keychain item to the code
//! signature that stored it, and Galpi's ad-hoc signature changes on every
//! build, so each release would ask every user to re-authorize a token they
//! never touched. Until the app ships with a Developer ID signature, secrets
//! stay in the settings file, which [`SettingsFile`] represents, and the switch
//! is one line in `LocalSettingsStore::new`.

use crate::application::error::AppError;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

/// The Keychain service every Galpi secret is filed under.
#[expect(dead_code, reason = "used once the app switches to Keychain storage")]
const SERVICE: &str = "com.m16khb.galpi";

/// One stored credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Secret {
    HuggingFaceToken,
    AssistantApiKey,
    /// The serialized ChatGPT OAuth tokens.
    ChatGptTokens,
}

impl Secret {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used once the app switches to Keychain storage")
    )]
    const fn account(self) -> &'static str {
        match self {
            Self::HuggingFaceToken => "hugging-face-token",
            Self::AssistantApiKey => "assistant-api-key",
            Self::ChatGptTokens => "chatgpt-tokens",
        }
    }
}

/// Where secrets are kept.
///
/// A trait rather than free functions so the storage logic around it — reading
/// through to the legacy file, migrating, clearing — can be tested without
/// touching the login keychain of whoever runs the suite.
pub trait SecretStore: std::fmt::Debug + Send + Sync {
    fn read(&self, secret: Secret) -> Result<Option<String>, AppError>;
    fn write(&self, secret: Secret, value: Option<&str>) -> Result<(), AppError>;

    /// Whether the settings file is where the value actually lives.
    ///
    /// A store that holds the secret itself wants the file scrubbed; one that
    /// does not would be erasing the only copy.
    fn keeps_plaintext_in_settings(&self) -> bool {
        false
    }
}

/// The macOS login keychain.
///
/// Not wired up yet — see the module comment. Kept compiled so the switch is a
/// one-line change rather than a rewrite once the app is signed.
#[expect(dead_code, reason = "wired up once the app ships a stable signature")]
#[derive(Debug, Default)]
pub struct Keychain;

impl SecretStore for Keychain {
    fn read(&self, secret: Secret) -> Result<Option<String>, AppError> {
        match get_generic_password(SERVICE, secret.account()) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|error| AppError::new("KEYCHAIN_INVALID", error.to_string())),
            // Any read failure is treated as "nothing stored": the item may be
            // absent, or the user may have declined access. Either way there is
            // no token to work with, and the caller's own message about a
            // missing token is more useful than a Keychain error code.
            Err(_) => Ok(None),
        }
    }

    fn write(&self, secret: Secret, value: Option<&str>) -> Result<(), AppError> {
        let Some(value) = value else {
            // Deleting something that was never there is the desired end
            // state, not a failure.
            let _removed = delete_generic_password(SERVICE, secret.account());
            return Ok(());
        };
        set_generic_password(SERVICE, secret.account(), value.as_bytes()).map_err(|error| {
            AppError::new(
                "KEYCHAIN_WRITE_FAILED",
                format!("키체인에 토큰을 저장하지 못했습니다: {error}"),
            )
        })
    }
}

/// An in-process store used by tests, so the suite never reads or writes the
/// login keychain of whoever runs it.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct InMemorySecrets {
    values: std::sync::Mutex<std::collections::HashMap<&'static str, String>>,
    /// How many times the store has been asked to read or write, which stands
    /// in for how many times a real keychain would have asked the user.
    writes: std::sync::atomic::AtomicUsize,
    reads: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl InMemorySecrets {
    pub fn writes(&self) -> usize {
        self.writes.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn reads(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
impl SecretStore for InMemorySecrets {
    fn read(&self, secret: Secret) -> Result<Option<String>, AppError> {
        let _count = self
            .reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(self
            .values
            .lock()
            .map_err(|_| AppError::new("KEYCHAIN_LOCKED", "테스트 저장소 잠금이 손상되었습니다."))?
            .get(secret.account())
            .cloned())
    }

    fn write(&self, secret: Secret, value: Option<&str>) -> Result<(), AppError> {
        let _count = self
            .writes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut values = self.values.lock().map_err(|_| {
            AppError::new("KEYCHAIN_LOCKED", "테스트 저장소 잠금이 손상되었습니다.")
        })?;
        match value {
            Some(value) => {
                let _replaced = values.insert(secret.account(), value.to_owned());
            }
            None => {
                let _removed = values.remove(secret.account());
            }
        }
        Ok(())
    }
}

/// Keeps a secret in the settings file rather than the keychain.
///
/// The file is created 0600 in the app's own Application Support directory.
/// That is weaker than the keychain — it is readable by anything running as
/// this user, and it travels into backups — and it is what Galpi did before
/// and still does until the app is signed. `LocalSettingsStore` writes and
/// clears the fields; this type exists so the choice of destination stays in
/// one place.
#[derive(Debug, Default)]
pub struct SettingsFile;

impl SecretStore for SettingsFile {
    fn read(&self, _secret: Secret) -> Result<Option<String>, AppError> {
        Ok(None)
    }

    fn write(&self, _secret: Secret, _value: Option<&str>) -> Result<(), AppError> {
        Ok(())
    }

    fn keeps_plaintext_in_settings(&self) -> bool {
        true
    }
}
