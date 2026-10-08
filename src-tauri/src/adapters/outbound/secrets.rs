//! Secret storage for the two credentials Galpi holds.
//!
//! Where a secret lives depends on the platform, and the composition root
//! picks the store:
//!
//! - Windows: the Credential Manager ([`credential_manager`]), the app's first
//!   OS-backed store.
//! - macOS: the settings file ([`SettingsFile`]). The Keychain ([`keychain`])
//!   is implemented but dormant: macOS ties access to a Keychain item to the
//!   code signature that stored it, and Galpi's ad-hoc signature changes on
//!   every build, so each release would ask every user to re-authorize a token
//!   they never touched. It switches on with a Developer ID signature.

use crate::application::error::AppError;

#[cfg(windows)]
mod credential;
#[cfg(all(test, not(windows)))]
mod credential;
#[cfg(windows)]
mod credential_manager;
#[cfg(target_os = "macos")]
mod keychain;

#[cfg(windows)]
pub use credential_manager::CredentialManager;

/// The service every Galpi secret is filed under.
#[cfg_attr(
    all(not(test), not(windows)),
    expect(dead_code, reason = "used once the app switches to Keychain storage")
)]
const SERVICE: &str = "com.m16khb.galpi";

/// One stored credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Secret {
    HuggingFaceToken,
    AssistantApiKey,
}

impl Secret {
    #[cfg_attr(
        all(not(test), not(windows)),
        expect(dead_code, reason = "used once the app switches to Keychain storage")
    )]
    const fn account(self) -> &'static str {
        match self {
            Self::HuggingFaceToken => "hugging-face-token",
            Self::AssistantApiKey => "assistant-api-key",
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
/// The file is created 0600 in the app's own Application Support directory
/// (on Windows it inherits the per-user app data ACL).
/// That is weaker than the keychain — it is readable by anything running as
/// this user, and it travels into backups — and it is what Galpi did before
/// and still does until the app is signed. `LocalSettingsStore` writes and
/// clears the fields; this type exists so the choice of destination stays in
/// one place.
#[cfg_attr(
    all(windows, not(test)),
    expect(dead_code, reason = "Windows keeps secrets in Credential Manager")
)]
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
