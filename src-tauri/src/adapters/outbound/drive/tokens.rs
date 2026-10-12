//! The code exchange and renewal at Google's token endpoint.
//!
//! Both send the desktop client id and secret with the grant as a form.

use crate::application::ports::DriveCallback;
use crate::domain::drive::{DriveClient, DriveTokenFailure, DriveTokens};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

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
    form: &[(&str, &str)],
) -> Result<TokenBody, DriveTokenFailure> {
    let response = http
        .post(endpoint)
        .form(form)
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
    callback: &DriveCallback,
    now: u64,
) -> Result<DriveTokens, DriveTokenFailure> {
    let (id, secret) = (callback.client.id(), callback.client.secret());
    let form = [
        ("client_id", id),
        ("client_secret", secret),
        ("grant_type", "authorization_code"),
        ("code", callback.code.as_str()),
        ("code_verifier", callback.verifier.as_str()),
        ("redirect_uri", callback.redirect_uri.as_str()),
    ];
    let mut answer = post(http, endpoint, &form).await?;
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
    client: &DriveClient,
    refresh_token: &str,
    now: u64,
) -> Result<DriveTokens, DriveTokenFailure> {
    let (id, secret) = (client.id(), client.secret());
    let form = [
        ("client_id", id),
        ("client_secret", secret),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];
    let mut answer = post(http, endpoint, &form).await?;
    // Google rotates the refresh token only sometimes; otherwise the old one stays.
    let refresh_token = answer
        .refresh_token
        .take()
        .unwrap_or_else(|| refresh_token.to_owned());
    Ok(tokens_from(answer, refresh_token, now))
}
