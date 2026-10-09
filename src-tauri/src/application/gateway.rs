//! The auth-gateway sign-in that every feature requires.
//!
//! The gate lives in memory and starts signed out; `restore` decides at
//! startup whether the stored session still opens the app. Only an explicit
//! refusal from the gateway or a sign-out discards the session: an unreachable
//! gateway keeps the app usable with the stored one.

use crate::application::error::AppError;
use crate::application::ports::{GatewayAuthPort, GatewaySessionStore};
use crate::domain::gateway::{AppAccess, GatewayRefreshFailure};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::oneshot;

pub struct AppAccessGate {
    auth: Arc<dyn GatewayAuthPort>,
    store: Arc<dyn GatewaySessionStore>,
    /// Visible to the application tests, which open the gate without a sign-in.
    pub(super) access: Mutex<AppAccess>,
    /// Serializes every session write so a rotated refresh token is spent
    /// once and a sign-out is never undone by a write still in flight.
    writes: tokio::sync::Mutex<()>,
    sign_in_active: AtomicBool,
    sign_in_cancel: Mutex<Option<oneshot::Sender<()>>>,
}

fn sign_in_failed() -> AppError {
    AppError::new(
        "GATEWAY_SIGN_IN_FAILED",
        "로그인 상태를 확인하지 못했습니다.",
    )
}

/// Releases the sign-in slot however the sign-in ends, including a dropped call.
struct SignInSlot<'a> {
    gate: &'a AppAccessGate,
}

impl Drop for SignInSlot<'_> {
    fn drop(&mut self) {
        if let Ok(mut cancel) = self.gate.sign_in_cancel.lock() {
            *cancel = None;
        }
        self.gate.sign_in_active.store(false, Ordering::Release);
    }
}

impl AppAccessGate {
    pub fn new(auth: Arc<dyn GatewayAuthPort>, store: Arc<dyn GatewaySessionStore>) -> Self {
        Self {
            auth,
            store,
            access: Mutex::new(AppAccess::SignedOut),
            writes: tokio::sync::Mutex::new(()),
            sign_in_active: AtomicBool::new(false),
            sign_in_cancel: Mutex::new(None),
        }
    }

    fn set(&self, access: AppAccess) -> AppAccess {
        *self.access.lock().unwrap_or_else(PoisonError::into_inner) = access.clone();
        access
    }

    /// Fail with `AUTH_REQUIRED` unless someone is signed in.
    pub fn require(&self) -> Result<(), AppError> {
        match *self.access.lock().unwrap_or_else(PoisonError::into_inner) {
            AppAccess::SignedIn { .. } => Ok(()),
            AppAccess::SignedOut => Err(AppError::new(
                "AUTH_REQUIRED",
                "Galpi를 사용하려면 Google로 로그인해 주세요.",
            )),
        }
    }

    /// Renew the stored session once and open or close the app accordingly.
    pub async fn restore(&self) -> Result<AppAccess, AppError> {
        let _writing = self.writes.lock().await;
        let Some(tokens) = self.store.load_tokens().await? else {
            return Ok(self.set(AppAccess::SignedOut));
        };
        match self.auth.refresh(&tokens.refresh_token).await {
            Ok(rotated) => {
                self.store.replace_tokens(rotated).await?;
                let email = self.store.load_email().await?;
                Ok(self.set(AppAccess::SignedIn {
                    email,
                    offline: false,
                }))
            }
            Err(GatewayRefreshFailure::Rejected) => {
                self.store.clear().await?;
                Ok(self.set(AppAccess::SignedOut))
            }
            Err(GatewayRefreshFailure::Unavailable) => {
                let email = self.store.load_email().await?;
                Ok(self.set(AppAccess::SignedIn {
                    email,
                    offline: true,
                }))
            }
        }
    }

    /// Run the browser sign-in and, on success, open the app.
    pub async fn sign_in(&self) -> Result<AppAccess, AppError> {
        let (_slot, mut cancel) = self.claim_sign_in()?;
        let grant = self.auth.sign_in(&mut cancel).await?;
        let _writing = self.writes.lock().await;
        // A sign-out that arrived after the browser came back cancels too.
        if cancel.try_recv().is_ok() {
            return Err(AppError::new("CANCELLED", "사용자가 작업을 취소했습니다."));
        }
        self.store
            .save_session(grant.tokens, grant.email.clone())
            .await?;
        Ok(self.set(AppAccess::SignedIn {
            email: grant.email,
            offline: false,
        }))
    }

    /// Ask a waiting sign-in to stop. Harmless when none is waiting; the
    /// stored session and the gate are untouched.
    pub fn cancel_sign_in(&self) -> Result<(), AppError> {
        let sender = self
            .sign_in_cancel
            .lock()
            .map_err(|_| sign_in_failed())?
            .take();
        if let Some(sender) = sender {
            // The receiver is gone only if the sign-in already finished.
            let _already_finished = sender.send(());
        }
        Ok(())
    }

    /// Close the app at once, revoke on the gateway when possible, then
    /// forget the session locally whatever the gateway said.
    pub async fn sign_out(&self) -> Result<(), AppError> {
        self.cancel_sign_in()?;
        self.set(AppAccess::SignedOut);
        let _writing = self.writes.lock().await;
        // A restore that held the lock before us may have opened the app again.
        self.set(AppAccess::SignedOut);
        // A session that cannot be read cannot be revoked either, and signing
        // out must still finish.
        if let Some(tokens) = self.store.load_tokens().await.ok().flatten() {
            self.auth.revoke(&tokens.refresh_token).await;
        }
        self.store.clear().await.map_err(|error| {
            AppError::new(
                "GATEWAY_SIGN_OUT_INCOMPLETE",
                format!(
                    "로그아웃했지만 저장된 로그인 정보를 지우지 못했습니다. 다음 실행 때 로그인된 상태로 열릴 수 있습니다. ({})",
                    error.message
                ),
            )
        })
    }

    fn claim_sign_in(&self) -> Result<(SignInSlot<'_>, oneshot::Receiver<()>), AppError> {
        if self.sign_in_active.swap(true, Ordering::AcqRel) {
            return Err(AppError::new(
                "GATEWAY_SIGN_IN_BUSY",
                "이미 Google 로그인이 진행 중입니다.",
            ));
        }
        let slot = SignInSlot { gate: self };
        let (sender, receiver) = oneshot::channel();
        *self.sign_in_cancel.lock().map_err(|_| sign_in_failed())? = Some(sender);
        Ok((slot, receiver))
    }
}
