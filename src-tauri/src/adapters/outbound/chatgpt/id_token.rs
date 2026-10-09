//! ID token verification: RS256 only, key chosen by `kid` from the issuer's JWKS.

use crate::application::error::AppError;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use reqwest::Client;
use serde::Deserialize;

/// Clock skew tolerated on `exp`.
const LEEWAY_SECONDS: u64 = 5;

#[derive(Deserialize)]
pub struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kid: Option<String>,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    exp: u64,
    nonce: Option<String>,
    sub: String,
    email: Option<String>,
}

/// What the token must say, fixed before the browser opens.
pub struct Expected<'a> {
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub nonce: &'a str,
    /// Unix seconds from the injected clock.
    pub now: u64,
}

/// The verified identity.
pub struct Identity {
    pub subject: String,
    pub email: Option<String>,
}

fn invalid(reason: &str) -> AppError {
    AppError::new(
        "CHATGPT_ID_TOKEN_INVALID",
        format!("ChatGPT 계정 확인 정보가 올바르지 않아 로그인을 마치지 못했습니다. ({reason})"),
    )
}

pub async fn fetch_jwks(http: &Client, url: &str) -> Result<Jwks, AppError> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|_| invalid("서명 키를 가져오지 못했습니다"))?;
    if !response.status().is_success() {
        return Err(invalid("서명 키 응답이 비정상입니다"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| invalid("서명 키를 끝까지 받지 못했습니다"))?;
    serde_json::from_slice(&bytes).map_err(|_| invalid("서명 키 형식을 알 수 없습니다"))
}

pub fn verify(id_token: &str, jwks: &Jwks, expected: &Expected<'_>) -> Result<Identity, AppError> {
    let header = decode_header(id_token).map_err(|_| invalid("토큰 형식"))?;
    if header.alg != Algorithm::RS256 {
        return Err(invalid("허용되지 않은 서명 방식"));
    }
    let kid = header.kid.ok_or_else(|| invalid("kid 없음"))?;
    let jwk = jwks
        .keys
        .iter()
        .find(|jwk| jwk.kty == "RSA" && jwk.kid.as_deref() == Some(kid.as_str()))
        .ok_or_else(|| invalid("일치하는 서명 키 없음"))?;
    let (Some(modulus), Some(exponent)) = (jwk.n.as_deref(), jwk.e.as_deref()) else {
        return Err(invalid("서명 키 구성 요소 없음"));
    };
    let key = DecodingKey::from_rsa_components(modulus, exponent)
        .map_err(|_| invalid("서명 키 구성 요소 형식"))?;

    let mut validation = Validation::new(Algorithm::RS256);
    // `exp` is checked below against the injected clock, not the wall clock.
    validation.validate_exp = false;
    validation.set_issuer(&[expected.issuer]);
    validation.set_audience(&[expected.client_id]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let claims = decode::<Claims>(id_token, &key, &validation)
        .map_err(|_| invalid("서명·발급자·대상 검증 실패"))?
        .claims;

    if claims.exp.saturating_add(LEEWAY_SECONDS) < expected.now {
        return Err(invalid("만료됨"));
    }
    if claims.nonce.as_deref() != Some(expected.nonce) {
        return Err(invalid("nonce 불일치"));
    }
    if claims.sub.trim().is_empty() {
        return Err(invalid("sub 없음"));
    }
    Ok(Identity {
        subject: claims.sub,
        email: claims.email,
    })
}

#[cfg(test)]
mod tests;
