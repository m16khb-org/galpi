//! Windows Credential Manager as a [`SecretStore`].
//!
//! Entries are generic credentials persisted for the local machine, so they
//! survive restarts but never roam.

use super::credential::{credential_target, credential_target_in, decode_blob, encode_blob};
use super::{Secret, SecretStore};
use crate::adapters::outbound::platform::to_wide_nul;
use crate::application::error::AppError;
use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr;
use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};

#[derive(Debug, Default)]
pub struct CredentialManager {
    /// Replaces the service in the target name; tests use it so they never
    /// touch a real Galpi entry.
    service: Option<String>,
}

impl CredentialManager {
    #[cfg(test)]
    fn with_service(service: String) -> Self {
        Self {
            service: Some(service),
        }
    }

    fn target(&self, secret: Secret) -> Vec<u16> {
        let target = match &self.service {
            Some(service) => credential_target_in(service, secret),
            None => credential_target(secret),
        };
        to_wide_nul(&target)
    }
}

fn last_error_is_not_found() -> bool {
    std::io::Error::last_os_error().raw_os_error() == i32::try_from(ERROR_NOT_FOUND).ok()
}

impl SecretStore for CredentialManager {
    fn read(&self, secret: Secret) -> Result<Option<String>, AppError> {
        let target = self.target(secret);
        let mut credential: *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: `target` is NUL-terminated and outlives the call, and
        // `credential` is a valid out-pointer for the allocated result.
        let found =
            unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &raw mut credential) };
        if found == 0 {
            if last_error_is_not_found() {
                return Ok(None);
            }
            return Err(AppError::new(
                "CREDENTIAL_READ_FAILED",
                format!(
                    "Windows 자격 증명 관리자에서 읽지 못했습니다: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
        // SAFETY: CredReadW succeeded, so `credential` points to a live
        // CREDENTIALW whose blob is `CredentialBlobSize` readable bytes (or
        // null with size 0). The bytes are copied before CredFree.
        let decoded = unsafe {
            let entry = &*credential;
            let blob = if entry.CredentialBlob.is_null() || entry.CredentialBlobSize == 0 {
                &[][..]
            } else {
                std::slice::from_raw_parts(
                    entry.CredentialBlob,
                    usize::try_from(entry.CredentialBlobSize).unwrap_or(0),
                )
            };
            decode_blob(blob)
        };
        // SAFETY: `credential` came from CredReadW and is freed exactly once,
        // after the last use above.
        unsafe { CredFree(credential.cast::<c_void>().cast_const()) };
        decoded.map(Some)
    }

    fn write(&self, secret: Secret, value: Option<&str>) -> Result<(), AppError> {
        let target = self.target(secret);
        let Some(value) = value else {
            // SAFETY: `target` is NUL-terminated and outlives the call.
            let deleted = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
            // Deleting what was never stored is the desired end state.
            if deleted == 0 && !last_error_is_not_found() {
                return Err(AppError::new(
                    "CREDENTIAL_WRITE_FAILED",
                    format!(
                        "Windows 자격 증명 관리자에서 지우지 못했습니다: {}",
                        std::io::Error::last_os_error()
                    ),
                ));
            }
            return Ok(());
        };
        let mut blob = encode_blob(value)?;
        let blob_size = u32::try_from(blob.len())
            .map_err(|_| AppError::new("CREDENTIAL_WRITE_FAILED", "자격 증명이 너무 깁니다."))?;
        // SAFETY: all-zero is a valid CREDENTIALW (null pointers, zero sizes).
        let mut credential: CREDENTIALW = unsafe { zeroed() };
        credential.Type = CRED_TYPE_GENERIC;
        credential.TargetName = target.as_ptr().cast_mut();
        credential.CredentialBlobSize = blob_size;
        credential.CredentialBlob = blob.as_mut_ptr();
        credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
        // SAFETY: `credential` points at `target` and `blob`, both alive for
        // the call; CredWriteW copies what it needs.
        let written = unsafe { CredWriteW(&raw const credential, 0) };
        if written == 0 {
            return Err(AppError::new(
                "CREDENTIAL_WRITE_FAILED",
                format!(
                    "Windows 자격 증명 관리자에 저장하지 못했습니다: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialManager, Secret, SecretStore};
    use crate::application::error::AppError;

    fn round_trip(store: &CredentialManager) -> Result<(), AppError> {
        let secret = Secret::AssistantApiKey;
        assert_eq!(store.read(secret)?, None);
        store.write(secret, Some("galpi-test-값-0123"))?;
        assert_eq!(store.read(secret)?.as_deref(), Some("galpi-test-값-0123"));
        store.write(secret, Some("galpi-test-replaced"))?;
        assert_eq!(store.read(secret)?.as_deref(), Some("galpi-test-replaced"));
        store.write(secret, None)?;
        assert_eq!(store.read(secret)?, None);
        // Deleting an absent entry is fine.
        store.write(secret, None)
    }

    #[test]
    fn credential_manager_round_trips_and_deletes_an_entry() -> Result<(), AppError> {
        // Given: a service no real Galpi entry uses
        let store = CredentialManager::with_service(format!("galpi-test-{}", uuid::Uuid::now_v7()));

        // When
        let outcome = round_trip(&store);

        // Then: whatever happened, the test entry is removed
        let cleanup = store.write(Secret::AssistantApiKey, None);
        outcome.and(cleanup)
    }
}
