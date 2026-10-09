//! Random values and the S256 code challenge (RFC 7636).

use crate::application::error::AppError;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// PKCE code verifier length in random bytes (86 characters once encoded).
pub const VERIFIER_BYTES: usize = 64;
/// `state` and `nonce` length in random bytes.
pub const NONCE_BYTES: usize = 32;

/// `bytes` of OS randomness as unpadded base64url.
pub fn random_urlsafe(bytes: usize) -> Result<String, AppError> {
    let mut buffer = vec![0_u8; bytes];
    getrandom::fill(&mut buffer).map_err(|_| {
        AppError::new(
            "CHATGPT_SIGN_IN_FAILED",
            "보안 난수를 만들지 못해 로그인을 시작하지 못했습니다.",
        )
    })?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// The S256 challenge for a verifier.
pub fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{NONCE_BYTES, VERIFIER_BYTES, code_challenge, random_urlsafe};
    use crate::application::error::AppError;

    #[test]
    fn challenge_matches_the_rfc_7636_appendix_b_vector() {
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn random_values_are_unpadded_urlsafe_and_distinct() -> Result<(), AppError> {
        let verifier = random_urlsafe(VERIFIER_BYTES)?;
        let state = random_urlsafe(NONCE_BYTES)?;

        // RFC 7636 §4.1: 43..=128 characters from the unreserved set.
        assert!((43..=128).contains(&verifier.len()));
        assert_eq!(state.len(), 43);
        for value in [&verifier, &state] {
            assert!(
                value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
            );
        }
        assert_ne!(verifier, random_urlsafe(VERIFIER_BYTES)?);
        Ok(())
    }
}
