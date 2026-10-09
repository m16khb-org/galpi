//! Sign in with ChatGPT outbound adapter: browser sign-in over a loopback
//! redirect, token refresh and revocation, and the account's model list.
//!
//! Nothing here logs a token, `code`, `state`, or authorization URL.

mod authorize;
mod id_token;
mod models;
mod oauth;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

use super::browser_sign_in::loopback::{Loopback, PREFERRED_PORT, SIGN_IN_TIMEOUT};
use super::browser_sign_in::{BrowserOpener, SignInCodes};
use crate::application::error::AppError;
use crate::application::ports::{ChatGptAuthPort, ChatGptEvents, ClockPort};
use crate::domain::chatgpt::{
    ChatGptModel, ChatGptRegistration, ChatGptSignInEvent, ChatGptSignInPhase, ChatGptTokens,
    RefreshFailure, RevocationOutcome, SignInGrant, SignInRequest,
};
use async_trait::async_trait;
use authorize::{Session, accept_callback, authorization_url};
use oauth::RevocationPolicy;
use reqwest::{Client, redirect::Policy};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

/// The ChatGPT codes for failures in the shared browser sign-in.
const CODES: SignInCodes = SignInCodes {
    failed: "CHATGPT_SIGN_IN_FAILED",
    timed_out: "CHATGPT_SIGN_IN_TIMEOUT",
};

/// The wall clock.
pub struct SystemClock;

impl ClockPort for SystemClock {
    fn now_unix(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs())
    }
}

/// Where the OpenAI endpoints live; tests point these at a fake server.
pub struct OpenAiEndpoints {
    pub authorize: String,
    pub token: String,
    pub revoke: String,
    pub jwks: String,
    pub issuer: String,
    pub models: String,
}

impl OpenAiEndpoints {
    pub fn production() -> Self {
        Self {
            authorize: "https://auth.openai.com/api/accounts/authorize".to_owned(),
            token: "https://auth.openai.com/oauth/token".to_owned(),
            revoke: "https://auth.openai.com/oauth/revoke".to_owned(),
            jwks: "https://auth.openai.com/.well-known/jwks.json".to_owned(),
            issuer: "https://auth.openai.com".to_owned(),
            models: "https://api.openai.com/v1/models".to_owned(),
        }
    }
}

/// Everything about an attempt that production fixes and tests shorten.
pub struct Tuning {
    pub endpoints: OpenAiEndpoints,
    pub revocation: RevocationPolicy,
    pub preferred_port: u16,
    pub sign_in_timeout: Duration,
}

impl Tuning {
    fn production() -> Self {
        Self {
            endpoints: OpenAiEndpoints::production(),
            revocation: RevocationPolicy::PRODUCTION,
            preferred_port: PREFERRED_PORT,
            sign_in_timeout: SIGN_IN_TIMEOUT,
        }
    }
}

pub struct ChatGptOAuthAdapter {
    http: Client,
    tuning: Tuning,
    browser: Arc<dyn BrowserOpener>,
    events: Arc<dyn ChatGptEvents>,
    clock: Arc<dyn ClockPort>,
}

impl ChatGptOAuthAdapter {
    pub fn new(
        browser: Arc<dyn BrowserOpener>,
        events: Arc<dyn ChatGptEvents>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, AppError> {
        Self::with_tuning(Tuning::production(), browser, events, clock)
    }

    fn with_tuning(
        tuning: Tuning,
        browser: Arc<dyn BrowserOpener>,
        events: Arc<dyn ChatGptEvents>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, AppError> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()
            .map_err(|_| {
                AppError::new(
                    "CHATGPT_SIGN_IN_FAILED",
                    "ChatGPT 연결을 준비하지 못했습니다.",
                )
            })?;
        Ok(Self {
            http,
            tuning,
            browser,
            events,
            clock,
        })
    }

    fn announce(&self, phase: ChatGptSignInPhase) {
        if self.events.emit(ChatGptSignInEvent { phase }).is_err() {
            eprintln!("chatgpt sign-in progress event was not delivered");
        }
    }
}

#[async_trait]
impl ChatGptAuthPort for ChatGptOAuthAdapter {
    async fn sign_in(
        &self,
        request: &SignInRequest,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<SignInGrant, AppError> {
        let endpoints = &self.tuning.endpoints;
        let session = Session::new()?;
        let loopback = Loopback::bind_preferring(self.tuning.preferred_port).await?;
        let redirect_uri = loopback.redirect_uri();
        let url = authorization_url(&endpoints.authorize, request, &session, &redirect_uri)?;
        self.browser.open(&url, CODES)?;
        self.announce(ChatGptSignInPhase::AwaitingBrowser);

        let callback = loopback
            .wait(cancel, self.tuning.sign_in_timeout, CODES)
            .await?;
        let accepted = accept_callback(&callback, request, &session)?;
        self.announce(ChatGptSignInPhase::Exchanging);
        let exchanged = oauth::exchange_code(
            &self.http,
            &endpoints.token,
            &accepted.client_id,
            &accepted.code,
            &session.verifier,
            &redirect_uri,
            self.clock.now_unix(),
        )
        .await?;

        let id_token = exchanged.id_token.ok_or_else(|| {
            AppError::new(
                "CHATGPT_ID_TOKEN_INVALID",
                "ChatGPT 계정 확인 정보가 응답에 없어 로그인을 마치지 못했습니다.",
            )
        })?;
        let jwks = id_token::fetch_jwks(&self.http, &endpoints.jwks).await?;
        let identity = id_token::verify(
            &id_token,
            &jwks,
            &id_token::Expected {
                issuer: &endpoints.issuer,
                client_id: &accepted.client_id,
                nonce: &session.nonce,
                now: self.clock.now_unix(),
            },
        )?;
        let email = identity.email.or_else(|| {
            request
                .registration
                .as_ref()
                .and_then(|stored| stored.email().map(str::to_owned))
        });
        let registration =
            ChatGptRegistration::new(accepted.client_id, identity.subject, email, false).map_err(
                |_| {
                    AppError::new(
                        "CHATGPT_REGISTRATION_INCOMPLETE",
                        "ChatGPT가 돌려준 앱 등록 정보가 올바르지 않습니다. 다시 시도해 주세요.",
                    )
                },
            )?;
        Ok(SignInGrant {
            registration,
            tokens: exchanged.tokens,
        })
    }

    async fn refresh(
        &self,
        registration: &ChatGptRegistration,
        refresh_token: &str,
    ) -> Result<ChatGptTokens, RefreshFailure> {
        oauth::refresh(
            &self.http,
            &self.tuning.endpoints.token,
            registration.client_id(),
            refresh_token,
            self.clock.now_unix(),
        )
        .await
    }

    async fn revoke(
        &self,
        registration: &ChatGptRegistration,
        refresh_token: &str,
    ) -> RevocationOutcome {
        oauth::revoke(
            &self.http,
            &self.tuning.endpoints.revoke,
            registration.client_id(),
            refresh_token,
            self.tuning.revocation,
        )
        .await
    }

    async fn list_models(&self, access_token: &str) -> Result<Vec<ChatGptModel>, AppError> {
        models::list_models(&self.http, &self.tuning.endpoints.models, access_token).await
    }
}
