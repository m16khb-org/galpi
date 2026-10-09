//! Windows Credential Manager as a [`SecretStore`].
//!
//! Entries are generic credentials persisted for the local machine, so they
//! survive restarts but never roam. A secret longer than one credential blob
//! spans several entries; the layout and what a write, a crash and a delete
//! each guarantee are documented in [`super::credential`].

use super::credential::{
    assemble, credential_target, credential_target_in, layout, orphans, piece_filter, piece_target,
};
use super::{Secret, SecretStore};
use crate::adapters::outbound::platform::to_wide_nul;
use crate::application::error::AppError;
use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr;
use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredEnumerateW,
    CredFree, CredReadW, CredWriteW,
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

    fn target(&self, secret: Secret) -> String {
        match &self.service {
            Some(service) => credential_target_in(service, secret),
            None => credential_target(secret),
        }
    }

    /// Delete every piece entry of `target` that `primary` (the entry now
    /// stored under it, if any) does not name.
    fn sweep(target: &str, primary: Option<&[u8]>) -> Result<(), AppError> {
        for name in orphans(target, primary, enumerate_pieces(target)?) {
            delete_entry(&name)?;
        }
        Ok(())
    }
}

fn last_error_is_not_found() -> bool {
    std::io::Error::last_os_error().raw_os_error() == i32::try_from(ERROR_NOT_FOUND).ok()
}

/// The raw blob stored under `target`, or `None` when there is no such entry.
fn read_entry(target: &str) -> Result<Option<Vec<u8>>, AppError> {
    let target = to_wide_nul(target);
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    // SAFETY: `target` is NUL-terminated and outlives the call, and
    // `credential` is a valid out-pointer for the allocated result.
    let found = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &raw mut credential) };
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
    let blob = unsafe { blob_of(&*credential) };
    // SAFETY: `credential` came from CredReadW and is freed exactly once,
    // after the last use above.
    unsafe { CredFree(credential.cast::<c_void>().cast_const()) };
    Ok(Some(blob))
}

/// A copy of the blob in `entry`.
///
/// # Safety
/// `entry.CredentialBlob` must be null or point to `CredentialBlobSize`
/// readable bytes, as it does for every CREDENTIALW the API hands out.
unsafe fn blob_of(entry: &CREDENTIALW) -> Vec<u8> {
    if entry.CredentialBlob.is_null() || entry.CredentialBlobSize == 0 {
        return Vec::new();
    }
    // SAFETY: the caller guarantees the pointer and size describe live memory.
    unsafe {
        std::slice::from_raw_parts(
            entry.CredentialBlob,
            usize::try_from(entry.CredentialBlobSize).unwrap_or(0),
        )
    }
    .to_vec()
}

/// Target names of every generic piece entry of `target`, any generation.
fn enumerate_pieces(target: &str) -> Result<Vec<String>, AppError> {
    let filter = to_wide_nul(&piece_filter(target));
    let mut count: u32 = 0;
    let mut entries: *mut *mut CREDENTIALW = ptr::null_mut();
    // SAFETY: `filter` is NUL-terminated and outlives the call; `count` and
    // `entries` are valid out-pointers.
    let found = unsafe { CredEnumerateW(filter.as_ptr(), 0, &raw mut count, &raw mut entries) };
    if found == 0 {
        if last_error_is_not_found() {
            return Ok(Vec::new());
        }
        return Err(AppError::new(
            "CREDENTIAL_READ_FAILED",
            format!(
                "Windows 자격 증명 관리자 항목을 나열하지 못했습니다: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    // SAFETY: CredEnumerateW succeeded, so `entries` points to `count`
    // pointers to live CREDENTIALWs, each with a NUL-terminated TargetName.
    // The names are copied before CredFree releases the whole buffer.
    let names = unsafe {
        std::slice::from_raw_parts(entries, usize::try_from(count).unwrap_or(0))
            .iter()
            .map(|entry| &**entry)
            .filter(|entry| entry.Type == CRED_TYPE_GENERIC && !entry.TargetName.is_null())
            .map(|entry| wide_to_string(entry.TargetName))
            .collect()
    };
    // SAFETY: `entries` came from CredEnumerateW and is freed exactly once,
    // after the last use above.
    unsafe { CredFree(entries.cast::<c_void>().cast_const()) };
    Ok(names)
}

/// Copy a NUL-terminated UTF-16 string.
///
/// # Safety
/// `text` must point to a readable, NUL-terminated UTF-16 string.
unsafe fn wide_to_string(text: *const u16) -> String {
    let mut length = 0;
    // SAFETY: the caller guarantees a terminating NUL, which stops the walk.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: `length` units before the terminator were just read.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) })
}

/// Store `blob` under `target`, replacing what was there. The caller keeps it
/// within `CRED_MAX_CREDENTIAL_BLOB_SIZE`.
fn write_entry(target: &str, blob: &[u8]) -> Result<(), AppError> {
    let target = to_wide_nul(target);
    let blob_size = u32::try_from(blob.len())
        .map_err(|_| AppError::new("CREDENTIAL_WRITE_FAILED", "자격 증명이 너무 깁니다."))?;
    // SAFETY: all-zero is a valid CREDENTIALW (null pointers, zero sizes).
    let mut credential: CREDENTIALW = unsafe { zeroed() };
    credential.Type = CRED_TYPE_GENERIC;
    credential.TargetName = target.as_ptr().cast_mut();
    credential.CredentialBlobSize = blob_size;
    credential.CredentialBlob = blob.as_ptr().cast_mut();
    credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
    // SAFETY: `credential` points at `target` and `blob`, both alive for the
    // call; CredWriteW copies what it needs and does not modify the blob.
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

/// Remove the entry under `target`. Deleting what was never stored is the
/// desired end state, so an absent entry is not an error.
fn delete_entry(target: &str) -> Result<(), AppError> {
    let target = to_wide_nul(target);
    // SAFETY: `target` is NUL-terminated and outlives the call.
    let deleted = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if deleted == 0 && !last_error_is_not_found() {
        return Err(AppError::new(
            "CREDENTIAL_WRITE_FAILED",
            format!(
                "Windows 자격 증명 관리자에서 지우지 못했습니다: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    Ok(())
}

impl SecretStore for CredentialManager {
    fn read(&self, secret: Secret) -> Result<Option<String>, AppError> {
        let target = self.target(secret);
        let Some(primary) = read_entry(&target)? else {
            return Ok(None);
        };
        assemble(&target, &primary, read_entry).map(Some)
    }

    fn write(&self, secret: Secret, value: Option<&str>) -> Result<(), AppError> {
        let target = self.target(secret);
        let Some(value) = value else {
            // The manifest goes first, so the secret is gone for readers at
            // once; the sweep then removes every piece, named or orphaned.
            delete_entry(&target)?;
            return Self::sweep(&target, None);
        };
        // A fresh generation, so no piece of the live secret is overwritten.
        let generation = uuid::Uuid::now_v7().simple().to_string();
        let layout = layout(value, &generation);
        let committed = layout
            .pieces
            .iter()
            .enumerate()
            .try_for_each(|(index, piece)| {
                write_entry(&piece_target(&target, &generation, index), piece)
            })
            // The manifest is the commit point: it goes last.
            .and_then(|()| write_entry(&target, &layout.primary));
        // Whatever the outcome, drop pieces the entry now in place does not
        // name: the old generation after a commit, this one after a failure.
        // The write's own result wins; a cleanup that fails leaves only
        // unreachable pieces, which the next write or delete sweeps.
        let swept =
            read_entry(&target).and_then(|primary| Self::sweep(&target, primary.as_deref()));
        committed?;
        let _ = swept;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CredentialManager, Secret, SecretStore, enumerate_pieces, read_entry, write_entry,
    };
    use crate::adapters::outbound::secrets::credential::{
        MAX_BLOB_BYTES, encode_blob, layout, live_pieces,
    };
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

    fn long(units: usize) -> String {
        format!("galpi-test-{}-값", "x".repeat(units))
    }

    /// Piece entries on disk, sorted.
    fn pieces_on_disk(store: &CredentialManager, secret: Secret) -> Result<Vec<String>, AppError> {
        let mut names = enumerate_pieces(&store.target(secret))?;
        names.sort();
        Ok(names)
    }

    /// Piece entries the stored manifest names, sorted.
    fn pieces_named(store: &CredentialManager, secret: Secret) -> Result<Vec<String>, AppError> {
        let target = store.target(secret);
        let primary = read_entry(&target)?.unwrap_or_default();
        let mut names = live_pieces(&target, &primary);
        names.sort();
        Ok(names)
    }

    fn pieces_needed(value: &str) -> usize {
        layout(value, "count").pieces.len()
    }

    fn long_round_trips(store: &CredentialManager) -> Result<(), AppError> {
        let secret = Secret::ChatGptTokens;
        let big = long(MAX_BLOB_BYTES * 2);
        let medium = long(MAX_BLOB_BYTES);
        assert!(pieces_needed(&big) > pieces_needed(&medium));
        assert!(pieces_needed(&medium) > 1);

        // short -> long: the value reads back and every piece is named
        store.write(secret, Some("galpi-test-short"))?;
        store.write(secret, Some(&big))?;
        assert_eq!(store.read(secret)?.as_deref(), Some(big.as_str()));
        assert_eq!(pieces_on_disk(store, secret)?.len(), pieces_needed(&big));
        assert_eq!(pieces_on_disk(store, secret)?, pieces_named(store, secret)?);

        // long -> long (a token refresh): only the new generation remains,
        // and it is a different set of entries from the old one
        let before = pieces_on_disk(store, secret)?;
        store.write(secret, Some(&medium))?;
        assert_eq!(store.read(secret)?.as_deref(), Some(medium.as_str()));
        let after = pieces_on_disk(store, secret)?;
        assert_eq!(after.len(), pieces_needed(&medium));
        assert_eq!(after, pieces_named(store, secret)?);
        assert!(before.iter().all(|name| !after.contains(name)));

        // long -> short: no piece survives
        store.write(secret, Some("galpi-test-short"))?;
        assert_eq!(store.read(secret)?.as_deref(), Some("galpi-test-short"));
        assert_eq!(pieces_on_disk(store, secret)?.len(), 0);

        // delete: nothing under the target remains
        store.write(secret, Some(&big))?;
        store.write(secret, None)?;
        assert_eq!(store.read(secret)?, None);
        assert_eq!(pieces_on_disk(store, secret)?.len(), 0);
        Ok(())
    }

    #[test]
    fn credential_manager_round_trips_a_secret_longer_than_one_blob() -> Result<(), AppError> {
        // Given: a service no real Galpi entry uses
        let store = CredentialManager::with_service(format!("galpi-test-{}", uuid::Uuid::now_v7()));

        // When
        let outcome = long_round_trips(&store);

        // Then: whatever happened, the test entry and its pieces are removed
        let cleanup = store.write(Secret::ChatGptTokens, None);
        outcome.and(cleanup)
    }

    #[test]
    fn credential_manager_reads_and_replaces_an_entry_in_the_single_entry_format()
    -> Result<(), AppError> {
        // Given: an entry exactly as an earlier build wrote it
        let store = CredentialManager::with_service(format!("galpi-test-{}", uuid::Uuid::now_v7()));
        let secret = Secret::ChatGptTokens;
        let outcome = (|| {
            write_entry(&store.target(secret), &encode_blob("galpi-test-legacy"))?;
            assert_eq!(store.read(secret)?.as_deref(), Some("galpi-test-legacy"));

            // When: replaced by a long value, then deleted
            let big = long(MAX_BLOB_BYTES);
            store.write(secret, Some(&big))?;
            assert_eq!(store.read(secret)?.as_deref(), Some(big.as_str()));
            store.write(secret, None)?;
            assert_eq!(store.read(secret)?, None);
            Ok(())
        })();

        // Then
        let cleanup = store.write(secret, None);
        outcome.and(cleanup)
    }
}
