//! Platform-neutral rules for Windows Credential Manager entries.
//!
//! The FFI lives in `credential_manager`; everything that can be decided
//! without calling Win32 is here so it is tested on every host.

use super::{SERVICE, Secret};
use crate::application::error::AppError;

/// `CRED_MAX_CREDENTIAL_BLOB_SIZE`: 5 * 512 bytes, i.e. 1,280 UTF-16 units.
pub const MAX_BLOB_BYTES: usize = 2560;

/// `com.m16khb.galpi:hugging-face-token` and friends.
pub fn credential_target(secret: Secret) -> String {
    credential_target_in(SERVICE, secret)
}

/// Like [`credential_target`] under another service, for isolated tests.
pub fn credential_target_in(service: &str, secret: Secret) -> String {
    format!("{service}:{}", secret.account())
}

/// UTF-16LE bytes of `value`, refusing what Credential Manager would reject.
pub fn encode_blob(value: &str) -> Result<Vec<u8>, AppError> {
    let bytes: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
    if bytes.len() > MAX_BLOB_BYTES {
        return Err(AppError::new(
            "CREDENTIAL_WRITE_FAILED",
            "자격 증명이 너무 길어 Windows 자격 증명 관리자에 저장할 수 없습니다.",
        ));
    }
    Ok(bytes)
}

pub fn decode_blob(bytes: &[u8]) -> Result<String, AppError> {
    let invalid = || {
        AppError::new(
            "CREDENTIAL_READ_FAILED",
            "저장된 자격 증명 형식이 올바르지 않습니다.",
        )
    };
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(invalid());
    }
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16(&units).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::{MAX_BLOB_BYTES, credential_target, decode_blob, encode_blob};
    use crate::adapters::outbound::secrets::Secret;

    #[test]
    fn targets_are_service_qualified_account_names() {
        assert_eq!(
            credential_target(Secret::HuggingFaceToken),
            "com.m16khb.galpi:hugging-face-token"
        );
        assert_eq!(
            credential_target(Secret::AssistantApiKey),
            "com.m16khb.galpi:assistant-api-key"
        );
    }

    #[test]
    fn blobs_round_trip_including_korean_text() -> Result<(), crate::application::error::AppError> {
        for value in ["galpi-test-ascii", "갈피-테스트-🙂", ""] {
            assert_eq!(decode_blob(&encode_blob(value)?)?, value);
        }
        Ok(())
    }

    #[test]
    fn a_blob_is_utf16_little_endian() -> Result<(), crate::application::error::AppError> {
        assert_eq!(encode_blob("a가")?, [0x61, 0x00, 0x00, 0xAC]);
        Ok(())
    }

    #[test]
    fn the_size_limit_is_1280_code_units() -> Result<(), crate::application::error::AppError> {
        assert_eq!(encode_blob(&"a".repeat(1280))?.len(), MAX_BLOB_BYTES);
        let error = encode_blob(&"a".repeat(1281)).err().map(|error| error.code);
        assert_eq!(error.as_deref(), Some("CREDENTIAL_WRITE_FAILED"));
        Ok(())
    }

    #[test]
    fn malformed_blobs_are_read_failures() {
        // Odd length, then a lone surrogate (0xD800)
        for bytes in [&[0x61_u8][..], &[0x00, 0xD8][..]] {
            assert_eq!(
                decode_blob(bytes).err().map(|error| error.code).as_deref(),
                Some("CREDENTIAL_READ_FAILED")
            );
        }
    }
}
