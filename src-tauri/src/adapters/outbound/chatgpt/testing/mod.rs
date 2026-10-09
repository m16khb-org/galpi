//! Test fixtures: a throwaway RS256 key pair.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs8::LineEnding;
use rsa::traits::PublicKeyParts;
use serde_json::{Value, json};
use std::sync::OnceLock;

pub const KEY_ID: &str = "test-key-1";

struct TestKey {
    pem: String,
    modulus: String,
    exponent: String,
}

/// Generated once per test run; never written to disk.
fn test_key() -> &'static TestKey {
    static KEY: OnceLock<TestKey> = OnceLock::new();
    KEY.get_or_init(|| {
        let private = RsaPrivateKey::new(&mut rand::thread_rng(), 2048)
            .unwrap_or_else(|_| std::process::abort());
        let pem = private
            .to_pkcs1_pem(LineEnding::LF)
            .map_or_else(|_| std::process::abort(), |pem| pem.to_string());
        TestKey {
            pem,
            modulus: URL_SAFE_NO_PAD.encode(private.n().to_bytes_be()),
            exponent: URL_SAFE_NO_PAD.encode(private.e().to_bytes_be()),
        }
    })
}

/// The JWKS document that verifies [`sign`].
pub fn jwks_json() -> Value {
    let key = test_key();
    json!({"keys": [{
        "kty": "RSA", "alg": "RS256", "use": "sig", "kid": KEY_ID,
        "n": key.modulus, "e": key.exponent,
    }]})
}

/// Sign `claims`; `kid` is omitted from the header when `None`.
pub fn sign(claims: &Value, kid: Option<&str>) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = kid.map(str::to_owned);
    EncodingKey::from_rsa_pem(test_key().pem.as_bytes())
        .and_then(|key| encode(&header, claims, &key))
        .unwrap_or_default()
}

/// Standard valid claims for `client_id`/`nonce`, expiring at `exp`.
pub fn claims(client_id: &str, nonce: &str, exp: u64) -> Value {
    json!({
        "iss": "https://auth.openai.com", "aud": client_id, "exp": exp,
        "nonce": nonce, "sub": "user-1", "email": "user@example.com",
    })
}
