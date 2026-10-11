//! The code exchange and renewal at the auth-gateway's `/drive/native/token`.
//!
//! The gateway holds the desktop client secret and forwards to Google. Galpi's
//! own gateway session vouches for the request as a bearer token.

use crate::application::ports::DriveCallback;
use crate::domain::drive::{DriveTokenFailure, DriveTokens};
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct TokenBody {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

/// An OAuth error body: `{"error": ".."}`.
#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

async fn post(
    http: &Client,
    endpoint: &str,
    bearer: &str,
    body: &Value,
) -> Result<TokenBody, DriveTokenFailure> {
    let response = http
        .post(endpoint)
        .bearer_auth(bearer)
        .header(CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|_| DriveTokenFailure::Unavailable)?;
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|_| DriveTokenFailure::Unavailable)?;
    if status.is_success() {
        return serde_json::from_slice(&bytes).map_err(|_| DriveTokenFailure::Unavailable);
    }
    if status == StatusCode::UNAUTHORIZED {
        return Err(DriveTokenFailure::AuthRequired);
    }
    if status == StatusCode::BAD_REQUEST
        && serde_json::from_slice::<ErrorBody>(&bytes)
            .is_ok_and(|body| body.error == "invalid_grant")
    {
        return Err(DriveTokenFailure::Rejected);
    }
    Err(DriveTokenFailure::Unavailable)
}

fn tokens_from(body: TokenBody, refresh_token: String, now: u64) -> DriveTokens {
    DriveTokens {
        access_token: body.access_token,
        refresh_token,
        expires_at: now.saturating_add(body.expires_in),
    }
}

pub(super) async fn exchange(
    http: &Client,
    endpoint: &str,
    bearer: &str,
    callback: &DriveCallback,
    now: u64,
) -> Result<DriveTokens, DriveTokenFailure> {
    let body = json!({
        "grant_type": "authorization_code",
        "code": callback.code,
        "code_verifier": callback.verifier,
        "redirect_uri": callback.redirect_uri,
    });
    let mut answer = post(http, endpoint, bearer, &body).await?;
    // With offline access and `prompt=consent` Google returns a refresh token;
    // an answer without one cannot keep the connection alive.
    let refresh_token = answer
        .refresh_token
        .take()
        .ok_or(DriveTokenFailure::Unavailable)?;
    Ok(tokens_from(answer, refresh_token, now))
}

pub(super) async fn refresh(
    http: &Client,
    endpoint: &str,
    bearer: &str,
    refresh_token: &str,
    now: u64,
) -> Result<DriveTokens, DriveTokenFailure> {
    let body = json!({"grant_type": "refresh_token", "refresh_token": refresh_token});
    let mut answer = post(http, endpoint, bearer, &body).await?;
    // Google rotates the refresh token only sometimes; otherwise the old one stays.
    let refresh_token = answer
        .refresh_token
        .take()
        .unwrap_or_else(|| refresh_token.to_owned());
    Ok(tokens_from(answer, refresh_token, now))
}
