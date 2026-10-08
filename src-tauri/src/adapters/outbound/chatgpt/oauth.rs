//! The token and revocation endpoints: code exchange, refresh, revoke.

use super::authorize::{RESOURCE, SCOPE};
use crate::application::error::AppError;
use crate::domain::chatgpt::{ChatGptTokens, RefreshFailure, RevocationOutcome};
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// Refresh-token error codes after which only a new sign-in can help.
const UNUSABLE_SESSION_CODES: [&str; 6] = [
    "invalid_grant",
    "invalid_refresh_token",
    "token_expired",
    "refresh_token_expired",
    "refresh_token_invalidated",
    "refresh_token_reused",
];

/// How often and how patiently a revocation is retried.
#[derive(Clone, Copy)]
pub struct RevocationPolicy {
    pub attempts: u32,
    /// Wait before the second attempt; it doubles after each further one.
    pub backoff: Duration,
}

impl RevocationPolicy {
    pub const PRODUCTION: Self = Self {
        attempts: 3,
        backoff: Duration::from_millis(500),
    };
}

#[derive(Deserialize)]
struct TokenBody {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: u64,
    scope: Option<String>,
}

/// A token endpoint success. The refresh token is present on exchange.
pub struct Exchanged {
    pub tokens: ChatGptTokens,
    pub id_token: Option<String>,
}

fn sign_in_failed(message: &str) -> AppError {
    AppError::new("CHATGPT_SIGN_IN_FAILED", message)
}

/// The OAuth error code of an error body: `{"error": ".."}`, `{"error": {"code": ".."}}`,
/// or `{"code": ".."}`.
fn error_code(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let code = match value.get("error") {
        Some(Value::String(code)) => code.as_str(),
        Some(Value::Object(object)) => object.get("code")?.as_str()?,
        _ => value.get("code")?.as_str()?,
    };
    Some(
        code.chars()
            .take(64)
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect(),
    )
}

/// Log the status, protocol error code, and request id, never the body.
async fn rejection(response: Response, operation: &str) -> (StatusCode, Option<String>) {
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(64).collect::<String>());
    let code = response
        .bytes()
        .await
        .ok()
        .and_then(|body| error_code(&body));
    eprintln!(
        "chatgpt {operation} failed: status={status} error={} request_id={}",
        code.as_deref().unwrap_or("-"),
        request_id.as_deref().unwrap_or("-")
    );
    (status, code)
}

fn into_tokens(body: TokenBody, fallback_refresh: Option<&str>, now: u64) -> Option<Exchanged> {
    let refresh_token = body
        .refresh_token
        .or_else(|| fallback_refresh.map(str::to_owned))?;
    let scopes = body
        .scope
        .as_deref()
        .unwrap_or(SCOPE)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    Some(Exchanged {
        tokens: ChatGptTokens {
            access_token: body.access_token,
            refresh_token,
            scopes,
            expires_at: now.saturating_add(body.expires_in),
        },
        id_token: body.id_token,
    })
}

pub async fn exchange_code(
    http: &Client,
    endpoint: &str,
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    now: u64,
) -> Result<Exchanged, AppError> {
    let form = [
        ("grant_type", "authorization_code"),
        ("client_id", client_id),
        ("code", code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect_uri),
        ("resource", RESOURCE),
    ];
    let response = http.post(endpoint).form(&form).send().await.map_err(|_| {
        sign_in_failed("ChatGPT 서버에 연결하지 못했습니다. 네트워크를 확인해 주세요.")
    })?;
    if !response.status().is_success() {
        rejection(response, "code exchange").await;
        return Err(sign_in_failed(
            "ChatGPT가 로그인 코드를 받아들이지 않았습니다. 다시 시도해 주세요.",
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| sign_in_failed("ChatGPT 응답을 끝까지 받지 못했습니다."))?;
    serde_json::from_slice::<TokenBody>(&bytes)
        .ok()
        .and_then(|body| into_tokens(body, None, now))
        .ok_or_else(|| sign_in_failed("ChatGPT 응답 형식을 알 수 없습니다."))
}

/// Refresh without `scope`, so the server keeps the granted set.
pub async fn refresh(
    http: &Client,
    endpoint: &str,
    client_id: &str,
    refresh_token: &str,
    now: u64,
) -> Result<ChatGptTokens, RefreshFailure> {
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token),
        ("resource", RESOURCE),
    ];
    let response = http
        .post(endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|_| RefreshFailure::Transient)?;
    if response.status().is_success() {
        let bytes = response
            .bytes()
            .await
            .map_err(|_| RefreshFailure::Transient)?;
        return serde_json::from_slice::<TokenBody>(&bytes)
            .ok()
            .and_then(|body| into_tokens(body, Some(refresh_token), now))
            .map(|exchanged| exchanged.tokens)
            .ok_or(RefreshFailure::Transient);
    }
    let (status, code) = rejection(response, "refresh").await;
    Err(match code.as_deref() {
        _ if status.is_server_error() => RefreshFailure::Transient,
        Some("invalid_client") => RefreshFailure::ClientRejected,
        Some(code) if UNUSABLE_SESSION_CODES.contains(&code) => RefreshFailure::SessionUnusable,
        _ => RefreshFailure::Transient,
    })
}

/// Revoke the refresh token; network errors and 5xx retry per `policy`.
pub async fn revoke(
    http: &Client,
    endpoint: &str,
    client_id: &str,
    refresh_token: &str,
    policy: RevocationPolicy,
) -> RevocationOutcome {
    let form = [
        ("token", refresh_token),
        ("token_type_hint", "refresh_token"),
        ("client_id", client_id),
    ];
    let mut wait = policy.backoff;
    for attempt in 0..policy.attempts {
        if attempt > 0 && !wait.is_zero() {
            tokio::time::sleep(wait).await;
            wait = wait.saturating_mul(2);
        }
        let Ok(response) = http.post(endpoint).form(&form).send().await else {
            continue;
        };
        if response.status().is_success() {
            return RevocationOutcome::Confirmed;
        }
        let (status, _) = rejection(response, "revocation").await;
        if !status.is_server_error() {
            break;
        }
    }
    RevocationOutcome::Unconfirmed
}
