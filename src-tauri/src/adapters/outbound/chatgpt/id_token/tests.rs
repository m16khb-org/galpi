use super::{Expected, Jwks, verify};
use crate::adapters::outbound::chatgpt::testing::{KEY_ID, claims, jwks_json, sign};
use crate::application::error::AppError;
use serde_json::{Value, json};

const CLIENT: &str = "oaiapp_test";
const NOW: u64 = 1_000_000;

fn jwks() -> Result<Jwks, serde_json::Error> {
    serde_json::from_value(jwks_json())
}

fn expected() -> Expected<'static> {
    Expected {
        issuer: "https://auth.openai.com",
        client_id: CLIENT,
        nonce: "nonce-1",
        now: NOW,
    }
}

fn code_of(result: Result<super::Identity, AppError>) -> Option<String> {
    result.err().map(|error| error.code)
}

fn rejected(claims: &Value, kid: Option<&str>) -> Result<bool, serde_json::Error> {
    let result = verify(&sign(claims, kid), &jwks()?, &expected());
    Ok(code_of(result).as_deref() == Some("CHATGPT_ID_TOKEN_INVALID"))
}

#[test]
fn a_correctly_signed_token_passes() -> Result<(), Box<dyn std::error::Error>> {
    let token = sign(&claims(CLIENT, "nonce-1", NOW + 60), Some(KEY_ID));

    let identity = verify(&token, &jwks()?, &expected())?;

    assert_eq!(identity.subject, "user-1");
    assert_eq!(identity.email.as_deref(), Some("user@example.com"));
    Ok(())
}

#[test]
fn a_token_expired_beyond_the_leeway_is_invalid() -> Result<(), serde_json::Error> {
    assert!(rejected(&claims(CLIENT, "nonce-1", NOW - 6), Some(KEY_ID))?);
    Ok(())
}

#[test]
fn a_token_expired_within_the_leeway_still_passes() -> Result<(), Box<dyn std::error::Error>> {
    let token = sign(&claims(CLIENT, "nonce-1", NOW - 5), Some(KEY_ID));

    verify(&token, &jwks()?, &expected())?;
    Ok(())
}

#[test]
fn a_foreign_issuer_is_invalid() -> Result<(), serde_json::Error> {
    let mut wrong = claims(CLIENT, "nonce-1", NOW + 60);
    wrong["iss"] = json!("https://evil.example");
    assert!(rejected(&wrong, Some(KEY_ID))?);
    Ok(())
}

#[test]
fn a_foreign_audience_is_invalid() -> Result<(), serde_json::Error> {
    assert!(rejected(
        &claims("oaiapp_other", "nonce-1", NOW + 60),
        Some(KEY_ID)
    )?);
    Ok(())
}

#[test]
fn a_foreign_nonce_is_invalid() -> Result<(), serde_json::Error> {
    assert!(rejected(
        &claims(CLIENT, "replayed", NOW + 60),
        Some(KEY_ID)
    )?);
    Ok(())
}

#[test]
fn a_blank_subject_is_invalid() -> Result<(), serde_json::Error> {
    let mut blank = claims(CLIENT, "nonce-1", NOW + 60);
    blank["sub"] = json!(" ");
    assert!(rejected(&blank, Some(KEY_ID))?);
    Ok(())
}

#[test]
fn a_missing_or_unknown_kid_is_invalid() -> Result<(), serde_json::Error> {
    let good = claims(CLIENT, "nonce-1", NOW + 60);
    assert!(rejected(&good, None)?);
    assert!(rejected(&good, Some("rotated-away"))?);
    Ok(())
}

#[test]
fn a_tampered_token_is_invalid() -> Result<(), serde_json::Error> {
    let token = sign(&claims(CLIENT, "nonce-1", NOW + 60), Some(KEY_ID));
    let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
    parts[1] = parts[1].chars().rev().collect();

    let result = verify(&parts.join("."), &jwks()?, &expected());

    assert_eq!(code_of(result).as_deref(), Some("CHATGPT_ID_TOKEN_INVALID"));
    Ok(())
}
