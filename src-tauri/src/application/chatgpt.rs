//! Sign in with ChatGPT use cases.
//!
//! The host, not the worker, keeps the session alive: refresh tokens rotate on
//! every use, so exactly one place may spend them, and only the host owns the
//! store that must remember the replacement. The worker receives a short-lived
//! access token and nothing else.

use crate::application::error::AppError;
use crate::application::ports::{ChatGptAuthPort, ChatGptStore, ClockPort};
use crate::domain::chatgpt::{
    ACCESS_TOKEN_REFRESH_MARGIN_SECONDS, ChatGptAccountState, ChatGptModel, ChatGptPreferences,
    ChatGptSettings, RefreshFailure, RevocationOutcome, SignOutResult,
};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

mod sign_in;

/// What a ChatGPT-mode refinement needs: a usable access token and the model.
pub struct ChatGptAccess {
    pub token: String,
    pub model: String,
}

/// Account lifecycle on top of the authorization server and the store.
///
/// The waiting sign-in has its own slot rather than a job slot: a person who
/// leaves the browser tab open must not block transcription or refinement.
pub struct ChatGptAccounts {
    auth: Arc<dyn ChatGptAuthPort>,
    store: Arc<dyn ChatGptStore>,
    clock: Arc<dyn ClockPort>,
    /// Serializes refreshes so a rotated refresh token is spent exactly once.
    refresh_lock: tokio::sync::Mutex<()>,
    sign_in_active: AtomicBool,
    sign_in_cancel: Mutex<Option<oneshot::Sender<()>>>,
}

fn sign_in_required() -> AppError {
    AppError::new(
        "CHATGPT_SIGN_IN_REQUIRED",
        "ChatGPT에 로그인해 주세요. 설정의 AI 증강에서 로그인할 수 있습니다.",
    )
}

impl ChatGptAccounts {
    pub fn new(
        auth: Arc<dyn ChatGptAuthPort>,
        store: Arc<dyn ChatGptStore>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            auth,
            store,
            clock,
            refresh_lock: tokio::sync::Mutex::new(()),
            sign_in_active: AtomicBool::new(false),
            sign_in_cancel: Mutex::new(None),
        }
    }

    pub async fn settings(&self) -> Result<ChatGptSettings, AppError> {
        self.store.load_settings().await
    }

    pub async fn save_preferences(&self, preferences: ChatGptPreferences) -> Result<(), AppError> {
        self.store.save_preferences(preferences.trimmed()).await
    }

    /// The account's listed models in server order, with a fresh access token.
    pub async fn models(&self) -> Result<Vec<ChatGptModel>, AppError> {
        let token = self.fresh_access_token().await?;
        self.auth.list_models(&token).await
    }

    /// Revoke on the server when possible, then forget the session locally
    /// whatever the server said. The host id stays: it names this installation.
    pub async fn sign_out(&self) -> Result<SignOutResult, AppError> {
        self.cancel_sign_in()?;
        // Held so a refresh in flight cannot write tokens back after the clear.
        let _refreshing = self.refresh_lock.lock().await;
        let registration = self.store.load_registration().await?;
        // A record that cannot be read cannot be revoked either, and signing
        // out must still finish.
        let tokens = self.store.load_tokens().await.ok().flatten();
        let confirmed = match (registration, tokens) {
            (Some(registration), Some(tokens)) => {
                self.auth.revoke(&registration, &tokens.refresh_token).await
                    == RevocationOutcome::Confirmed
            }
            _ => true,
        };
        self.store.clear_session().await?;
        Ok(SignOutResult {
            settings: self.store.load_settings().await?,
            remote_revocation_confirmed: confirmed,
        })
    }

    /// A usable access token and the selected model for one refinement.
    pub async fn access_for_refinement(&self) -> Result<ChatGptAccess, AppError> {
        let settings = self.store.load_settings().await?;
        if settings.account.state != ChatGptAccountState::SignedIn {
            return Err(sign_in_required());
        }
        let model = settings.model.ok_or_else(|| {
            AppError::new(
                "CHATGPT_MODEL_REQUIRED",
                "회의록 정제에 쓸 ChatGPT 모델을 먼저 선택해 주세요.",
            )
        })?;
        let token = self.fresh_access_token().await?;
        Ok(ChatGptAccess { token, model })
    }

    /// An access token with at least the refresh margin left, refreshing once
    /// for any number of concurrent callers.
    async fn fresh_access_token(&self) -> Result<String, AppError> {
        let margin = ACCESS_TOKEN_REFRESH_MARGIN_SECONDS;
        let tokens = self
            .store
            .load_tokens()
            .await?
            .ok_or_else(sign_in_required)?;
        if !tokens.needs_refresh(self.clock.now_unix(), margin) {
            return Ok(tokens.access_token);
        }
        let _refreshing = self.refresh_lock.lock().await;
        // Whoever held the lock before us may already have refreshed.
        let tokens = self
            .store
            .load_tokens()
            .await?
            .ok_or_else(sign_in_required)?;
        if !tokens.needs_refresh(self.clock.now_unix(), margin) {
            return Ok(tokens.access_token);
        }
        let registration = self
            .store
            .load_registration()
            .await?
            .ok_or_else(sign_in_required)?;
        match self
            .auth
            .refresh(&registration, &tokens.refresh_token)
            .await
        {
            Ok(rotated) => {
                self.store.replace_tokens(rotated.clone()).await?;
                Ok(rotated.access_token)
            }
            Err(RefreshFailure::SessionUnusable) => {
                self.store.clear_tokens().await?;
                Err(sign_in_required())
            }
            Err(RefreshFailure::ClientRejected) => Err(AppError::new(
                "CHATGPT_CLIENT_INVALID",
                "ChatGPT가 이 앱의 등록을 거부했습니다. 로그아웃한 뒤 다시 로그인해 주세요.",
            )),
            Err(RefreshFailure::Transient) => Err(AppError::new(
                "CHATGPT_REFRESH_UNAVAILABLE",
                "ChatGPT 로그인을 갱신하지 못했습니다. 네트워크를 확인한 뒤 다시 시도해 주세요.",
            )),
        }
    }
}
