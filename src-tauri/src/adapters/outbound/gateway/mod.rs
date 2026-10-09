//! The auth-gateway desktop sign-in (RFC 8252): Google through the system
//! browser and a loopback redirect with PKCE, then a token exchange, renewal,
//! and sign-out against the gateway's `/auth/native` endpoints.
//!
//! Nothing here logs a token, `code`, `state`, or sign-in URL.

#[cfg(test)]
mod tests;

use super::browser_sign_in::loopback::{Loopback, PREFERRED_PORT, SIGN_IN_TIMEOUT};
use super::browser_sign_in::pkce::{NONCE_BYTES, VERIFIER_BYTES, code_challenge, random_urlsafe};
use super::browser_sign_in::{BrowserOpener, SignInCodes};
use crate::application::error::AppError;
use crate::application::ports::{ClockPort, GatewayAuthPort};
use crate::domain::gateway::{GatewayGrant, GatewayRefreshFailure, GatewayTokens};
use async_trait::async_trait;
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, RequestBuilder, StatusCode, Url, redirect::Policy};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

/// Where the gateway is deployed.
const PRODUCTION_URL: &str = "https://auth.m16khb.dev";
/// Debug builds only: point the app at a locally running gateway.
const URL_OVERRIDE: &str = "GALPI_AUTH_GATEWAY_URL";

const CODES: SignInCodes = SignInCodes {
    failed: "GATEWAY_SIGN_IN_FAILED",
    timed_out: "GATEWAY_SIGN_IN_TIMEOUT",
};

fn failed(message: &str) -> AppError {
    AppError::new(CODES.failed, message)
}

/// The gateway base URL: the production one, or in a debug build the override.
fn gateway_url() -> String {
    if cfg!(debug_assertions)
        && let Ok(url) = std::env::var(URL_OVERRIDE)
        && !url.trim().is_empty()
    {
        return url.trim().trim_end_matches('/').to_owned();
    }
    PRODUCTION_URL.to_owned()
}

/// Everything about an attempt that production fixes and tests shorten.
pub struct Tuning {
    pub base_url: String,
    pub preferred_port: u16,
    pub sign_in_timeout: Duration,
}

impl Tuning {
    fn production() -> Self {
        Self {
            base_url: gateway_url(),
            preferred_port: PREFERRED_PORT,
            sign_in_timeout: SIGN_IN_TIMEOUT,
        }
    }
}

#[derive(Deserialize)]
struct TokenBody {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Deserialize)]
struct MeBody {
    email: Option<String>,
}

/// An OAuth error body: `{"error": ".."}`.
#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

pub struct GatewayOAuthAdapter {
    http: Client,
    tuning: Tuning,
    browser: Arc<dyn BrowserOpener>,
    clock: Arc<dyn ClockPort>,
}

impl GatewayOAuthAdapter {
    pub fn new(
        browser: Arc<dyn BrowserOpener>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, AppError> {
        Self::with_tuning(Tuning::production(), browser, clock)
    }

    fn with_tuning(
        tuning: Tuning,
        browser: Arc<dyn BrowserOpener>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, AppError> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()
            .map_err(|_| failed("로그인 서버 연결을 준비하지 못했습니다."))?;
        Ok(Self {
            http,
            tuning,
            browser,
            clock,
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.tuning.base_url)
    }

    fn post_json(&self, path: &str, body: &Value) -> RequestBuilder {
        self.http
            .post(self.endpoint(path))
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
    }

    /// The URL for the system browser. It carries `state`, so never log it.
    fn sign_in_url(
        &self,
        redirect_uri: &str,
        verifier: &str,
        state: &str,
    ) -> Result<String, AppError> {
        let mut url = Url::parse(&self.endpoint("/auth/native/google"))
            .map_err(|_| failed("로그인 주소를 만들지 못했습니다."))?;
        url.query_pairs_mut()
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("code_challenge", &code_challenge(verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state);
        Ok(url.into())
    }

    fn session_from(
        &self,
        body: TokenBody,
        fallback_refresh: Option<&str>,
    ) -> Option<GatewayTokens> {
        let refresh_token = body
            .refresh_token
            .or_else(|| fallback_refresh.map(str::to_owned))?;
        Some(GatewayTokens {
            access_token: body.access_token,
            refresh_token,
            expires_at: self.clock.now_unix().saturating_add(body.expires_in),
        })
    }

    async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<GatewayTokens, AppError> {
        let body = json!({
            "grant_type": "authorization_code",
            "code": code,
            "code_verifier": verifier,
            "redirect_uri": redirect_uri,
        });
        let response = self
            .post_json("/auth/native/token", &body)
            .send()
            .await
            .map_err(|_| failed("로그인 서버에 연결하지 못했습니다. 네트워크를 확인해 주세요."))?;
        if !response.status().is_success() {
            return Err(failed(
                "로그인 서버가 로그인 코드를 받아들이지 않았습니다. 다시 시도해 주세요.",
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| failed("로그인 서버 응답을 끝까지 받지 못했습니다."))?;
        serde_json::from_slice::<TokenBody>(&bytes)
            .ok()
            .and_then(|body| self.session_from(body, None))
            .ok_or_else(|| failed("로그인 서버 응답 형식을 알 수 없습니다."))
    }

    /// Who signed in; `None` when the gateway cannot say.
    async fn email(&self, access_token: &str) -> Option<String> {
        let response = self
            .http
            .get(self.endpoint("/me"))
            .bearer_auth(access_token)
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let bytes = response.bytes().await.ok()?;
        serde_json::from_slice::<MeBody>(&bytes)
            .ok()?
            .email
            .map(|email| email.trim().to_owned())
            .filter(|email| !email.is_empty())
    }
}

#[async_trait]
impl GatewayAuthPort for GatewayOAuthAdapter {
    async fn sign_in(&self, cancel: &mut oneshot::Receiver<()>) -> Result<GatewayGrant, AppError> {
        let state = random_urlsafe(NONCE_BYTES, CODES)?;
        let verifier = random_urlsafe(VERIFIER_BYTES, CODES)?;
        let loopback = Loopback::bind_preferring(self.tuning.preferred_port).await?;
        let redirect_uri = loopback.redirect_uri();
        let url = self.sign_in_url(&redirect_uri, &verifier, &state)?;
        self.browser.open(&url, CODES)?;

        let callback = loopback
            .wait(cancel, self.tuning.sign_in_timeout, CODES)
            .await?;
        if callback.get("state") != Some(state.as_str()) {
            return Err(failed(
                "로그인 응답이 이 요청과 맞지 않아 로그인을 마치지 못했습니다. 다시 시도해 주세요.",
            ));
        }
        if callback.get("error").is_some() {
            return Err(failed(
                "Google 로그인이 취소되었거나 완료되지 않았습니다. 다시 시도해 주세요.",
            ));
        }
        let code = callback
            .get("code")
            .filter(|code| !code.is_empty())
            .ok_or_else(|| failed("로그인 응답에 로그인 코드가 없습니다. 다시 시도해 주세요."))?;
        let tokens = self.exchange_code(code, &verifier, &redirect_uri).await?;
        let email = self.email(&tokens.access_token).await;
        Ok(GatewayGrant { tokens, email })
    }

    /// Only `400 invalid_grant` ends the session; every other failure keeps it.
    async fn refresh(&self, refresh_token: &str) -> Result<GatewayTokens, GatewayRefreshFailure> {
        let body = json!({"grant_type": "refresh_token", "refresh_token": refresh_token});
        let response = self
            .post_json("/auth/native/token", &body)
            .send()
            .await
            .map_err(|_| GatewayRefreshFailure::Unavailable)?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| GatewayRefreshFailure::Unavailable)?;
        if status.is_success() {
            return serde_json::from_slice::<TokenBody>(&bytes)
                .ok()
                .and_then(|body| self.session_from(body, Some(refresh_token)))
                .ok_or(GatewayRefreshFailure::Unavailable);
        }
        if status == StatusCode::BAD_REQUEST
            && serde_json::from_slice::<ErrorBody>(&bytes)
                .is_ok_and(|body| body.error == "invalid_grant")
        {
            return Err(GatewayRefreshFailure::Rejected);
        }
        Err(GatewayRefreshFailure::Unavailable)
    }

    async fn revoke(&self, refresh_token: &str) {
        let body = json!({"refresh_token": refresh_token});
        // Best effort: the local session is forgotten whatever the gateway says.
        let _answered = self.post_json("/auth/native/logout", &body).send().await;
    }
}
