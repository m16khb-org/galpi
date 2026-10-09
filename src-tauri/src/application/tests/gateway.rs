use super::{FakePort, TranscriptionBehavior, request};
use crate::application::error::AppError;
use crate::application::ports::{GatewayAuthPort, GatewaySessionStore};
use crate::application::use_cases::Application;
use crate::domain::gateway::{AppAccess, GatewayGrant, GatewayRefreshFailure, GatewayTokens};
use crate::domain::job::{SetupRequest, SpeakerHint, TranscriptImportRequest};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{Notify, oneshot};
use uuid::Uuid;

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AppError> {
    mutex
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "gateway fake lock poisoned"))
}

#[derive(Default)]
struct Stored {
    tokens: Option<GatewayTokens>,
    email: Option<String>,
}

#[derive(Default)]
struct Calls {
    sign_ins: usize,
    refresh_tokens: Vec<String>,
    revoked: Vec<String>,
}

/// The gateway side of `FakePort`: an in-memory session store plus a
/// scripted gateway that records every call.
#[derive(Default)]
pub(super) struct GatewayFake {
    stored: Mutex<Stored>,
    calls: Mutex<Calls>,
    sign_in_result: Mutex<Option<Result<GatewayGrant, AppError>>>,
    block_sign_in: AtomicBool,
    sign_in_started: Notify,
    refreshes: Mutex<VecDeque<Result<GatewayTokens, GatewayRefreshFailure>>>,
    unreadable_tokens: AtomicBool,
    clear_failure: Mutex<Option<AppError>>,
}

impl GatewayFake {
    fn stored(&self) -> Result<MutexGuard<'_, Stored>, AppError> {
        lock(&self.stored)
    }

    fn calls(&self) -> Result<MutexGuard<'_, Calls>, AppError> {
        lock(&self.calls)
    }

    fn seed(&self, tokens: GatewayTokens, email: Option<&str>) -> Result<(), AppError> {
        let mut stored = self.stored()?;
        stored.tokens = Some(tokens);
        stored.email = email.map(str::to_owned);
        Ok(())
    }

    fn script_refresh(
        &self,
        result: Result<GatewayTokens, GatewayRefreshFailure>,
    ) -> Result<(), AppError> {
        lock(&self.refreshes)?.push_back(result);
        Ok(())
    }

    fn script_sign_in(&self, result: Result<GatewayGrant, AppError>) -> Result<(), AppError> {
        *lock(&self.sign_in_result)? = Some(result);
        Ok(())
    }
}

#[async_trait]
impl GatewayAuthPort for FakePort {
    async fn sign_in(&self, cancel: &mut oneshot::Receiver<()>) -> Result<GatewayGrant, AppError> {
        self.gateway.calls()?.sign_ins += 1;
        if self.gateway.block_sign_in.load(Ordering::Relaxed) {
            self.gateway.sign_in_started.notify_one();
            let _cancelled = cancel.await;
            return Err(AppError::new("CANCELLED", "사용자가 작업을 취소했습니다."));
        }
        lock(&self.gateway.sign_in_result)?
            .take()
            .unwrap_or_else(|| Err(AppError::new("TEST_ERROR", "no sign-in was scripted")))
    }

    async fn refresh(&self, refresh_token: &str) -> Result<GatewayTokens, GatewayRefreshFailure> {
        if let Ok(mut calls) = self.gateway.calls() {
            calls.refresh_tokens.push(refresh_token.to_owned());
        }
        tokio::task::yield_now().await;
        self.gateway
            .refreshes
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front())
            .unwrap_or(Err(GatewayRefreshFailure::Unavailable))
    }

    async fn revoke(&self, refresh_token: &str) {
        if let Ok(mut calls) = self.gateway.calls() {
            calls.revoked.push(refresh_token.to_owned());
        }
    }
}

#[async_trait]
impl GatewaySessionStore for FakePort {
    async fn load_email(&self) -> Result<Option<String>, AppError> {
        Ok(self.gateway.stored()?.email.clone())
    }

    async fn load_tokens(&self) -> Result<Option<GatewayTokens>, AppError> {
        if self.gateway.unreadable_tokens.load(Ordering::Relaxed) {
            return Err(AppError::new("SECRET_READ_FAILED", "unreadable"));
        }
        Ok(self.gateway.stored()?.tokens.clone())
    }

    async fn save_session(
        &self,
        tokens: GatewayTokens,
        email: Option<String>,
    ) -> Result<(), AppError> {
        let mut stored = self.gateway.stored()?;
        stored.tokens = Some(tokens);
        stored.email = email;
        Ok(())
    }

    async fn replace_tokens(&self, tokens: GatewayTokens) -> Result<(), AppError> {
        self.gateway.stored()?.tokens = Some(tokens);
        Ok(())
    }

    async fn clear(&self) -> Result<(), AppError> {
        if let Some(error) = lock(&self.gateway.clear_failure)?.clone() {
            return Err(error);
        }
        *self.gateway.stored()? = Stored::default();
        Ok(())
    }
}

fn tokens(refresh: &str) -> GatewayTokens {
    GatewayTokens {
        access_token: format!("galpi-test-access-{refresh}"),
        refresh_token: format!("galpi-test-refresh-{refresh}"),
        expires_at: 3_600,
    }
}

fn port() -> Arc<FakePort> {
    Arc::new(FakePort::new(TranscriptionBehavior::Success))
}

fn failure<T>(result: Result<T, AppError>) -> Result<AppError, AppError> {
    match result {
        Ok(_) => Err(AppError::new("TEST_ERROR", "expected a failure")),
        Err(error) => Ok(error),
    }
}

/// The codes of every gated feature, in a fixed order.
async fn feature_codes(app: &Application) -> Vec<String> {
    let code = |result: Result<(), AppError>| result.err().map(|error| error.code);
    vec![
        code(
            app.prepare(SetupRequest {
                job_id: Uuid::now_v7(),
                hugging_face_token: None,
            })
            .await
            .map(drop),
        ),
        code(
            app.refine_transcript(Uuid::now_v7(), Uuid::now_v7(), &[])
                .await
                .map(drop),
        ),
        code(app.transcribe(request(SpeakerHint::Auto)).await.map(drop)),
        code(
            app.import_transcript(TranscriptImportRequest {
                job_id: Uuid::now_v7(),
                input_path: "/tmp/meeting.txt".to_owned(),
                output_root: "/tmp".to_owned(),
            })
            .await
            .map(drop),
        ),
        code(app.start_recording("/tmp".to_owned()).await.map(drop)),
    ]
    .into_iter()
    .map(Option::unwrap_or_default)
    .collect()
}

const ALL_REQUIRE_SIGN_IN: [&str; 5] = [
    "AUTH_REQUIRED",
    "AUTH_REQUIRED",
    "AUTH_REQUIRED",
    "AUTH_REQUIRED",
    "AUTH_REQUIRED",
];

#[tokio::test]
async fn gateway_without_a_stored_session_keeps_every_feature_closed() -> Result<(), AppError> {
    // Given
    let port = port();
    let app = port.signed_out_application();

    // When
    let access = app.load_app_access().await?;

    // Then
    assert_eq!(access, AppAccess::SignedOut);
    assert_eq!(feature_codes(&app).await, ALL_REQUIRE_SIGN_IN);
    assert_eq!(port.prepare_calls.load(Ordering::SeqCst), 0);
    assert_eq!(port.gateway.calls()?.refresh_tokens, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn gateway_renewal_stores_the_rotated_session_and_opens_the_app() -> Result<(), AppError> {
    // Given
    let port = port();
    port.gateway.seed(tokens("old"), Some("user@example.com"))?;
    port.gateway.script_refresh(Ok(tokens("rotated")))?;
    let app = port.signed_out_application();

    // When
    let access = app.load_app_access().await?;

    // Then
    assert_eq!(
        access,
        AppAccess::SignedIn {
            email: Some("user@example.com".to_owned()),
            offline: false,
        }
    );
    assert_eq!(
        port.gateway.calls()?.refresh_tokens,
        ["galpi-test-refresh-old"]
    );
    assert_eq!(port.gateway.stored()?.tokens, Some(tokens("rotated")));
    app.transcribe(request(SpeakerHint::Auto)).await?;
    Ok(())
}

#[tokio::test]
async fn gateway_refusal_forgets_the_session_and_keeps_the_app_closed() -> Result<(), AppError> {
    // Given
    let port = port();
    port.gateway.seed(tokens("old"), Some("user@example.com"))?;
    port.gateway
        .script_refresh(Err(GatewayRefreshFailure::Rejected))?;
    let app = port.signed_out_application();

    // When
    let access = app.load_app_access().await?;

    // Then
    assert_eq!(access, AppAccess::SignedOut);
    assert_eq!(port.gateway.stored()?.tokens, None);
    assert_eq!(port.gateway.stored()?.email, None);
    assert_eq!(feature_codes(&app).await, ALL_REQUIRE_SIGN_IN);
    Ok(())
}

#[tokio::test]
async fn gateway_unreachable_opens_the_app_offline_with_the_stored_session() -> Result<(), AppError>
{
    // Given
    let port = port();
    port.gateway.seed(tokens("old"), Some("user@example.com"))?;
    port.gateway
        .script_refresh(Err(GatewayRefreshFailure::Unavailable))?;
    let app = port.signed_out_application();

    // When
    let access = app.load_app_access().await?;

    // Then
    assert_eq!(
        access,
        AppAccess::SignedIn {
            email: Some("user@example.com".to_owned()),
            offline: true,
        }
    );
    assert_eq!(port.gateway.stored()?.tokens, Some(tokens("old")));
    app.transcribe(request(SpeakerHint::Auto)).await?;
    Ok(())
}

#[tokio::test]
async fn gateway_sign_in_stores_the_session_and_opens_the_app() -> Result<(), AppError> {
    // Given
    let port = port();
    port.gateway.script_sign_in(Ok(GatewayGrant {
        tokens: tokens("new"),
        email: Some("user@example.com".to_owned()),
    }))?;
    let app = port.signed_out_application();

    // When
    let access = app.sign_in_to_gateway().await?;

    // Then
    assert_eq!(
        access,
        AppAccess::SignedIn {
            email: Some("user@example.com".to_owned()),
            offline: false,
        }
    );
    assert_eq!(port.gateway.stored()?.tokens, Some(tokens("new")));
    assert_eq!(
        port.gateway.stored()?.email.as_deref(),
        Some("user@example.com")
    );
    app.transcribe(request(SpeakerHint::Auto)).await?;
    Ok(())
}

#[tokio::test]
async fn gateway_sign_in_allows_one_attempt_at_a_time_and_can_be_cancelled() -> Result<(), AppError>
{
    // Given: a sign-in waiting for the browser
    let port = port();
    port.gateway.block_sign_in.store(true, Ordering::Relaxed);
    let app = Arc::new(port.signed_out_application());
    let first = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.sign_in_to_gateway().await })
    };
    port.gateway.sign_in_started.notified().await;

    // When
    let second = failure(app.sign_in_to_gateway().await)?;
    app.cancel_gateway_sign_in()?;
    let first = failure(
        first
            .await
            .map_err(|error| AppError::new("TEST_JOIN", error.to_string()))?,
    )?;

    // Then
    assert_eq!(second.code, "GATEWAY_SIGN_IN_BUSY");
    assert_eq!(first.code, "CANCELLED");
    assert_eq!(port.gateway.calls()?.sign_ins, 1);
    assert_eq!(port.gateway.stored()?.tokens, None);
    assert_eq!(feature_codes(&app).await, ALL_REQUIRE_SIGN_IN);
    Ok(())
}

#[tokio::test]
async fn gateway_sign_out_revokes_once_forgets_the_session_and_closes_the_app()
-> Result<(), AppError> {
    // Given
    let port = port();
    port.gateway.seed(tokens("old"), Some("user@example.com"))?;
    port.gateway.script_refresh(Ok(tokens("rotated")))?;
    let app = port.signed_out_application();
    app.load_app_access().await?;

    // When
    app.sign_out_of_gateway().await?;

    // Then
    assert_eq!(
        port.gateway.calls()?.revoked,
        ["galpi-test-refresh-rotated"]
    );
    assert_eq!(port.gateway.stored()?.tokens, None);
    assert_eq!(port.gateway.stored()?.email, None);
    assert_eq!(feature_codes(&app).await, ALL_REQUIRE_SIGN_IN);
    Ok(())
}

#[tokio::test]
async fn gateway_sign_out_clears_even_when_the_session_cannot_be_read() -> Result<(), AppError> {
    // Given: tokens the store can no longer read, so nothing can be revoked
    let port = port();
    port.gateway.seed(tokens("old"), Some("user@example.com"))?;
    port.gateway
        .unreadable_tokens
        .store(true, Ordering::Relaxed);
    let app = port.application();

    // When
    app.sign_out_of_gateway().await?;

    // Then
    assert_eq!(port.gateway.calls()?.revoked, Vec::<String>::new());
    assert_eq!(port.gateway.stored()?.email, None);
    assert_eq!(feature_codes(&app).await, ALL_REQUIRE_SIGN_IN);
    Ok(())
}

#[tokio::test]
async fn gateway_sign_out_that_cannot_clear_still_closes_the_app() -> Result<(), AppError> {
    // Given
    let port = port();
    port.gateway.seed(tokens("old"), None)?;
    *lock(&port.gateway.clear_failure)? = Some(AppError::new("SECRET_WRITE_FAILED", "locked"));
    let app = port.application();

    // When
    let error = failure(app.sign_out_of_gateway().await)?;

    // Then
    assert_eq!(error.code, "GATEWAY_SIGN_OUT_INCOMPLETE");
    assert!(error.message.contains("다음 실행"));
    assert_eq!(feature_codes(&app).await, ALL_REQUIRE_SIGN_IN);
    Ok(())
}

#[tokio::test]
async fn gateway_sign_out_during_sign_in_cancels_it_and_stores_nothing() -> Result<(), AppError> {
    // Given: a sign-in waiting for the browser
    let port = port();
    port.gateway.block_sign_in.store(true, Ordering::Relaxed);
    let app = Arc::new(port.signed_out_application());
    let signing_in = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.sign_in_to_gateway().await })
    };
    port.gateway.sign_in_started.notified().await;

    // When
    app.sign_out_of_gateway().await?;
    let signed_in = failure(
        signing_in
            .await
            .map_err(|error| AppError::new("TEST_JOIN", error.to_string()))?,
    )?;

    // Then
    assert_eq!(signed_in.code, "CANCELLED");
    assert_eq!(port.gateway.stored()?.tokens, None);
    assert_eq!(port.gateway.calls()?.revoked, Vec::<String>::new());
    assert_eq!(feature_codes(&app).await, ALL_REQUIRE_SIGN_IN);
    Ok(())
}

#[tokio::test]
async fn gateway_cancelled_sign_in_keeps_the_stored_session() -> Result<(), AppError> {
    // Given: a stored session and a second sign-in waiting for the browser
    let port = port();
    port.gateway.seed(tokens("old"), Some("user@example.com"))?;
    port.gateway.block_sign_in.store(true, Ordering::Relaxed);
    let app = Arc::new(port.application());
    let signing_in = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.sign_in_to_gateway().await })
    };
    port.gateway.sign_in_started.notified().await;

    // When
    app.cancel_gateway_sign_in()?;
    let signed_in = failure(
        signing_in
            .await
            .map_err(|error| AppError::new("TEST_JOIN", error.to_string()))?,
    )?;

    // Then
    assert_eq!(signed_in.code, "CANCELLED");
    assert_eq!(port.gateway.calls()?.revoked, Vec::<String>::new());
    assert_eq!(port.gateway.stored()?.tokens, Some(tokens("old")));
    assert_eq!(
        port.gateway.stored()?.email.as_deref(),
        Some("user@example.com")
    );
    app.transcribe(request(SpeakerHint::Auto)).await?;
    Ok(())
}
