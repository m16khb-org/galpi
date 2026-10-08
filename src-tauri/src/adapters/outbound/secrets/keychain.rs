//! The macOS login keychain.
//!
//! Not wired up yet — see the parent module comment. Kept compiled so the
//! switch is a one-line change rather than a rewrite once the app is signed.

use super::{SERVICE, Secret, SecretStore};
use crate::application::error::AppError;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

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
