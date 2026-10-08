//! The browser sign-in: its single slot, cancellation, and adopting the grant.

use super::ChatGptAccounts;
use crate::application::error::AppError;
use crate::domain::chatgpt::{
    AssistantAuthMode, ChatGptModel, ChatGptPreferences, ChatGptSettings, SignInGrant,
    SignInRequest,
};
use std::sync::atomic::Ordering;
use tokio::sync::oneshot;

/// Releases the sign-in slot however the sign-in ends, including a dropped call.
struct SignInSlot<'a> {
    accounts: &'a ChatGptAccounts,
}

impl Drop for SignInSlot<'_> {
    fn drop(&mut self) {
        if let Ok(mut cancel) = self.accounts.sign_in_cancel.lock() {
            *cancel = None;
        }
        self.accounts.sign_in_active.store(false, Ordering::Release);
    }
}

impl ChatGptAccounts {
    /// Run the browser sign-in and, on success, switch to ChatGPT mode.
    pub async fn sign_in(&self) -> Result<ChatGptSettings, AppError> {
        let (_slot, mut cancel) = self.claim_sign_in()?;
        let host_id = self.store.load_or_create_host_id().await?;
        let stored = self.store.load_registration().await?;
        let request = SignInRequest {
            host_id,
            registration: stored.clone(),
        };
        let grant = self.auth.sign_in(&request, &mut cancel).await?;
        if let Some(stored) = &stored
            && grant.registration.subject() != stored.subject()
        {
            return Err(AppError::new(
                "CHATGPT_ACCOUNT_MISMATCH",
                "이전에 로그인한 ChatGPT 계정과 다른 계정입니다. 같은 계정으로 로그인하거나 먼저 로그아웃해 주세요.",
            ));
        }
        self.adopt(grant).await
    }

    /// Ask a waiting sign-in to stop. Harmless when none is waiting.
    pub fn cancel_sign_in(&self) -> Result<(), AppError> {
        let sender = self
            .sign_in_cancel
            .lock()
            .map_err(|_| {
                AppError::new(
                    "CHATGPT_SIGN_IN_FAILED",
                    "로그인 상태를 확인하지 못했습니다.",
                )
            })?
            .take();
        if let Some(sender) = sender {
            // The receiver is gone only if the sign-in already finished.
            let _already_finished = sender.send(());
        }
        Ok(())
    }

    fn claim_sign_in(&self) -> Result<(SignInSlot<'_>, oneshot::Receiver<()>), AppError> {
        if self.sign_in_active.swap(true, Ordering::AcqRel) {
            return Err(AppError::new(
                "CHATGPT_SIGN_IN_BUSY",
                "이미 ChatGPT 로그인이 진행 중입니다.",
            ));
        }
        let slot = SignInSlot { accounts: self };
        let (sender, receiver) = oneshot::channel();
        *self.sign_in_cancel.lock().map_err(|_| {
            AppError::new(
                "CHATGPT_SIGN_IN_FAILED",
                "로그인 상태를 확인하지 못했습니다.",
            )
        })? = Some(sender);
        Ok((slot, receiver))
    }

    /// Store a completed sign-in, unless the server withheld plan use.
    async fn adopt(&self, grant: SignInGrant) -> Result<ChatGptSettings, AppError> {
        if !grant.tokens.grants_plan_usage() {
            // The next attempt reuses this client and asks for consent again.
            self.store
                .save_session(grant.registration.with_needs_consent(true), None)
                .await?;
            return Err(AppError::new(
                "CHATGPT_PLAN_USE_NOT_GRANTED",
                "ChatGPT 요금제 사용 권한이 허용되지 않았습니다. 다시 로그인해 요금제 사용을 허용해 주세요.",
            ));
        }
        let access_token = grant.tokens.access_token.clone();
        self.store
            .save_session(
                grant.registration.with_needs_consent(false),
                Some(grant.tokens),
            )
            .await?;
        let current = self.store.load_settings().await?;
        // Signing in succeeded either way; a failed list only leaves the model
        // unchosen, and the sheet lists models again when it opens.
        let listed = self
            .auth
            .list_models(&access_token)
            .await
            .unwrap_or_default();
        self.store
            .save_preferences(ChatGptPreferences {
                auth_mode: AssistantAuthMode::ChatGpt,
                model: choose_model(&listed, current.model),
                welcome_acknowledged: current.welcome_acknowledged,
            })
            .await?;
        self.store.load_settings().await
    }
}

/// Keep the saved choice while the server still offers it, otherwise take the
/// server's first model. An empty list changes nothing.
fn choose_model(listed: &[ChatGptModel], saved: Option<String>) -> Option<String> {
    if listed.is_empty() {
        return saved;
    }
    saved
        .filter(|slug| listed.iter().any(|model| &model.slug == slug))
        .or_else(|| listed.first().map(|model| model.slug.clone()))
}
