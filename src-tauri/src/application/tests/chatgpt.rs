use super::{FakePort, SeenRefinement, TranscriptionBehavior, request};
use crate::application::error::AppError;
use crate::application::model::RefinementResult;
use crate::application::ports::{ChatGptAuthPort, ChatGptStore, ClockPort};
use crate::application::use_cases::Application;
use crate::domain::chatgpt::{
    AgentHostId, AssistantAuthMode, AssistantTransport, ChatGptAccountState, ChatGptAccountView,
    ChatGptModel, ChatGptPreferences, ChatGptRegistration, ChatGptSettings, ChatGptTokens,
    PLAN_USAGE_SCOPE, RefreshFailure, RevocationOutcome, SignInGrant, SignInRequest,
};
use crate::domain::job::SpeakerHint;
use crate::domain::roster::AssistantSettings;
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{Notify, oneshot};
use uuid::Uuid;

const NOW: u64 = 1_000_000;
const HOUR: u64 = 3_600;
const SOON: u64 = 10 * 60;

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AppError> {
    mutex
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "chatgpt fake lock poisoned"))
}

#[derive(Default)]
struct Account {
    preferences: ChatGptPreferences,
    host_id: Option<AgentHostId>,
    registration: Option<ChatGptRegistration>,
    tokens: Option<ChatGptTokens>,
}

#[derive(Default)]
struct Calls {
    sign_in_requests: Vec<Option<ChatGptRegistration>>,
    refresh_tokens: Vec<String>,
    revoked: Vec<String>,
    models_tokens: Vec<String>,
    log: Vec<&'static str>,
}

/// The ChatGPT side of `FakePort`: an in-memory store plus a scripted
/// authorization server that records every call.
pub(super) struct ChatGptFake {
    now: AtomicU64,
    account: Mutex<Account>,
    calls: Mutex<Calls>,
    sign_in_result: Mutex<Option<Result<SignInGrant, AppError>>>,
    block_sign_in: AtomicBool,
    sign_in_started: Notify,
    refreshes: Mutex<VecDeque<Result<ChatGptTokens, RefreshFailure>>>,
    revocation: Mutex<RevocationOutcome>,
    models: Mutex<Vec<ChatGptModel>>,
    refine_failure: Mutex<Option<AppError>>,
}

impl ChatGptFake {
    pub(super) fn new() -> Self {
        Self {
            now: AtomicU64::new(NOW),
            account: Mutex::new(Account::default()),
            calls: Mutex::new(Calls::default()),
            sign_in_result: Mutex::new(None),
            block_sign_in: AtomicBool::new(false),
            sign_in_started: Notify::new(),
            refreshes: Mutex::new(VecDeque::new()),
            revocation: Mutex::new(RevocationOutcome::Confirmed),
            models: Mutex::new(Vec::new()),
            refine_failure: Mutex::new(None),
        }
    }

    /// Mirrors a worker that fails the run after being asked once.
    pub(super) fn refine_outcome(&self) -> Result<(), AppError> {
        match lock(&self.refine_failure)?.as_ref() {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn account(&self) -> Result<MutexGuard<'_, Account>, AppError> {
        lock(&self.account)
    }

    fn calls(&self) -> Result<MutexGuard<'_, Calls>, AppError> {
        lock(&self.calls)
    }

    fn log(&self, entry: &'static str) -> Result<(), AppError> {
        self.calls()?.log.push(entry);
        Ok(())
    }

    fn script_sign_in(&self, result: Result<SignInGrant, AppError>) -> Result<(), AppError> {
        *lock(&self.sign_in_result)? = Some(result);
        Ok(())
    }

    fn script_refresh(
        &self,
        result: Result<ChatGptTokens, RefreshFailure>,
    ) -> Result<(), AppError> {
        lock(&self.refreshes)?.push_back(result);
        Ok(())
    }

    fn set_models(&self, models: &[(&str, &str)]) -> Result<(), AppError> {
        *lock(&self.models)? = models
            .iter()
            .map(|(slug, name)| ChatGptModel {
                slug: (*slug).to_owned(),
                display_name: (*name).to_owned(),
            })
            .collect();
        Ok(())
    }

    /// A signed-in installation: host id, registration, optional tokens, and
    /// ChatGPT selected with the given model.
    fn seed(&self, tokens: Option<ChatGptTokens>, model: Option<&str>) -> Result<(), AppError> {
        let mut account = self.account()?;
        account.host_id = Some(AgentHostId::generate());
        account.registration = Some(registration("subject-1", false)?);
        account.tokens = tokens;
        account.preferences = ChatGptPreferences {
            auth_mode: AssistantAuthMode::ChatGpt,
            model: model.map(str::to_owned),
            welcome_acknowledged: true,
        };
        Ok(())
    }
}

#[async_trait]
impl ChatGptAuthPort for FakePort {
    async fn sign_in(
        &self,
        request: &SignInRequest,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<SignInGrant, AppError> {
        self.chatgpt
            .calls()?
            .sign_in_requests
            .push(request.registration.clone());
        if self.chatgpt.block_sign_in.load(Ordering::Relaxed) {
            self.chatgpt.sign_in_started.notify_one();
            let _cancelled = cancel.await;
            return Err(AppError::new("CANCELLED", "로그인이 취소되었습니다."));
        }
        lock(&self.chatgpt.sign_in_result)?
            .take()
            .unwrap_or_else(|| Err(AppError::new("TEST_ERROR", "no sign-in was scripted")))
    }

    async fn refresh(
        &self,
        _registration: &ChatGptRegistration,
        refresh_token: &str,
    ) -> Result<ChatGptTokens, RefreshFailure> {
        if let Ok(mut calls) = self.chatgpt.calls() {
            calls.refresh_tokens.push(refresh_token.to_owned());
        }
        // A real refresh crosses the network, so concurrent callers interleave.
        tokio::task::yield_now().await;
        self.chatgpt
            .refreshes
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front())
            .unwrap_or(Err(RefreshFailure::Transient))
    }

    async fn revoke(
        &self,
        _registration: &ChatGptRegistration,
        refresh_token: &str,
    ) -> RevocationOutcome {
        if let Ok(mut calls) = self.chatgpt.calls() {
            calls.revoked.push(refresh_token.to_owned());
            calls.log.push("revoke");
        }
        self.chatgpt
            .revocation
            .lock()
            .map_or(RevocationOutcome::Unconfirmed, |outcome| *outcome)
    }

    async fn list_models(&self, access_token: &str) -> Result<Vec<ChatGptModel>, AppError> {
        self.chatgpt
            .calls()?
            .models_tokens
            .push(access_token.to_owned());
        Ok(lock(&self.chatgpt.models)?.clone())
    }
}

#[async_trait]
impl ChatGptStore for FakePort {
    async fn load_settings(&self) -> Result<ChatGptSettings, AppError> {
        let account = self.chatgpt.account()?;
        let state = match (&account.registration, &account.tokens) {
            (None, _) => ChatGptAccountState::SignedOut,
            (Some(_), None) => ChatGptAccountState::SignInRequired,
            (Some(_), Some(_)) => ChatGptAccountState::SignedIn,
        };
        Ok(ChatGptSettings {
            auth_mode: account.preferences.auth_mode,
            model: account.preferences.model.clone(),
            welcome_acknowledged: account.preferences.welcome_acknowledged,
            account: ChatGptAccountView {
                state,
                email: account
                    .registration
                    .as_ref()
                    .and_then(|registration| registration.email().map(str::to_owned)),
            },
        })
    }

    async fn save_preferences(&self, preferences: ChatGptPreferences) -> Result<(), AppError> {
        self.chatgpt.account()?.preferences = preferences;
        Ok(())
    }

    async fn load_or_create_host_id(&self) -> Result<AgentHostId, AppError> {
        self.chatgpt.log("host_id")?;
        Ok(self
            .chatgpt
            .account()?
            .host_id
            .get_or_insert_with(AgentHostId::generate)
            .clone())
    }

    async fn load_registration(&self) -> Result<Option<ChatGptRegistration>, AppError> {
        Ok(self.chatgpt.account()?.registration.clone())
    }

    async fn load_tokens(&self) -> Result<Option<ChatGptTokens>, AppError> {
        Ok(self.chatgpt.account()?.tokens.clone())
    }

    async fn save_session(
        &self,
        registration: ChatGptRegistration,
        tokens: Option<ChatGptTokens>,
    ) -> Result<(), AppError> {
        self.chatgpt.log("save_session")?;
        let mut account = self.chatgpt.account()?;
        account.registration = Some(registration);
        if tokens.is_some() {
            account.tokens = tokens;
        }
        Ok(())
    }

    async fn replace_tokens(&self, tokens: ChatGptTokens) -> Result<(), AppError> {
        self.chatgpt.log("replace_tokens")?;
        self.chatgpt.account()?.tokens = Some(tokens);
        Ok(())
    }

    async fn clear_tokens(&self) -> Result<(), AppError> {
        self.chatgpt.log("clear_tokens")?;
        self.chatgpt.account()?.tokens = None;
        Ok(())
    }

    async fn clear_session(&self) -> Result<(), AppError> {
        self.chatgpt.log("clear_session")?;
        let mut account = self.chatgpt.account()?;
        account.registration = None;
        account.tokens = None;
        account.preferences.model = None;
        account.preferences.auth_mode = AssistantAuthMode::ApiKey;
        Ok(())
    }
}

impl ClockPort for FakePort {
    fn now_unix(&self) -> u64 {
        self.chatgpt.now.load(Ordering::Relaxed)
    }
}

fn port() -> Arc<FakePort> {
    Arc::new(FakePort::new(TranscriptionBehavior::Success))
}

fn tokens(access: &str, refresh: &str, expires_at: u64) -> ChatGptTokens {
    ChatGptTokens {
        access_token: access.to_owned(),
        refresh_token: refresh.to_owned(),
        scopes: vec!["openid".to_owned(), PLAN_USAGE_SCOPE.to_owned()],
        expires_at,
    }
}

fn registration(subject: &str, needs_consent: bool) -> Result<ChatGptRegistration, AppError> {
    ChatGptRegistration::new(
        "oaiapp_test".to_owned(),
        subject.to_owned(),
        Some("user@example.com".to_owned()),
        needs_consent,
    )
    .map_err(|error| AppError::new("TEST_ERROR", format!("{error:?}")))
}

fn grant(subject: &str, plan_usage: bool) -> Result<SignInGrant, AppError> {
    let mut tokens = tokens("access-new", "refresh-new", NOW + HOUR);
    if !plan_usage {
        tokens.scopes = vec!["openid".to_owned()];
    }
    Ok(SignInGrant {
        registration: registration(subject, false)?,
        tokens,
    })
}

/// The error a call must have failed with.
fn failure<T>(result: Result<T, AppError>) -> Result<AppError, AppError> {
    match result {
        Err(error) => Ok(error),
        Ok(_) => Err(AppError::new(
            "TEST_ERROR",
            "the call unexpectedly succeeded",
        )),
    }
}

/// Transcribe once so there is a meeting to refine, and return its job id.
async fn transcribed(app: &Application) -> Result<Uuid, AppError> {
    Ok(app.transcribe(request(SpeakerHint::Auto)).await?.job_id)
}

async fn refine(app: &Application) -> Result<RefinementResult, AppError> {
    let target = transcribed(app).await?;
    app.refine_transcript(Uuid::now_v7(), target, &[]).await
}

fn seen(port: &FakePort) -> Result<Vec<(String, Option<String>, AssistantTransport)>, AppError> {
    let refinements = lock(&port.refinements)?;
    Ok(refinements
        .iter()
        .map(|job: &SeenRefinement| (job.api_key.clone(), job.model.clone(), job.transport))
        .collect())
}

#[tokio::test]
async fn first_sign_in_registers_a_new_client_and_selects_the_first_model() -> Result<(), AppError>
{
    // Given: a fresh installation and a server that grants plan use
    let port = port();
    port.chatgpt.script_sign_in(grant("subject-1", true))?;
    port.chatgpt
        .set_models(&[("gpt-5", "GPT-5"), ("gpt-5-mini", "GPT-5 mini")])?;
    let app = port.application();

    // When
    let settings = app.sign_in_with_chatgpt().await?;

    // Then: signed in, ChatGPT selected, the server's first model chosen
    assert_eq!(settings.account.state, ChatGptAccountState::SignedIn);
    assert_eq!(settings.account.email.as_deref(), Some("user@example.com"));
    assert_eq!(settings.auth_mode, AssistantAuthMode::ChatGpt);
    assert_eq!(settings.model.as_deref(), Some("gpt-5"));
    let calls = port.chatgpt.calls()?;
    assert_eq!(calls.sign_in_requests, [None]);
    assert_eq!(calls.log.first(), Some(&"host_id"));
    assert_eq!(calls.models_tokens, ["access-new"]);
    let account = port.chatgpt.account()?;
    assert_eq!(
        account.tokens,
        Some(tokens("access-new", "refresh-new", NOW + HOUR))
    );
    assert_eq!(
        account
            .registration
            .as_ref()
            .map(ChatGptRegistration::client_id),
        Some("oaiapp_test")
    );
    Ok(())
}

#[tokio::test]
async fn signing_in_again_reuses_the_stored_registration_and_keeps_a_listed_model()
-> Result<(), AppError> {
    // Given: a stored registration and a saved model that is still offered
    let port = port();
    port.chatgpt.seed(None, Some("gpt-5-mini"))?;
    port.chatgpt.script_sign_in(grant("subject-1", true))?;
    port.chatgpt
        .set_models(&[("gpt-5", "GPT-5"), ("gpt-5-mini", "GPT-5 mini")])?;
    let app = port.application();

    // When
    let settings = app.sign_in_with_chatgpt().await?;

    // Then: the sign-in started from the stored registration and kept the choice
    assert_eq!(settings.account.state, ChatGptAccountState::SignedIn);
    assert_eq!(settings.model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(
        port.chatgpt.calls()?.sign_in_requests,
        [Some(registration("subject-1", false)?)]
    );
    Ok(())
}

#[tokio::test]
async fn signing_in_again_replaces_a_model_the_server_no_longer_lists() -> Result<(), AppError> {
    // Given
    let port = port();
    port.chatgpt.seed(None, Some("retired-model"))?;
    port.chatgpt.script_sign_in(grant("subject-1", true))?;
    port.chatgpt.set_models(&[("gpt-5", "GPT-5")])?;
    let app = port.application();

    // When
    let settings = app.sign_in_with_chatgpt().await?;

    // Then
    assert_eq!(settings.model.as_deref(), Some("gpt-5"));
    Ok(())
}

#[tokio::test]
async fn a_different_account_is_rejected_and_nothing_stored_changes() -> Result<(), AppError> {
    // Given: subject-1 is stored and the browser signs in subject-2
    let port = port();
    let before = tokens("access-old", "refresh-old", NOW + HOUR);
    port.chatgpt.seed(Some(before.clone()), Some("gpt-5"))?;
    port.chatgpt.script_sign_in(grant("subject-2", true))?;
    let app = port.application();

    // When
    let error = failure(app.sign_in_with_chatgpt().await)?;

    // Then
    assert_eq!(error.code, "CHATGPT_ACCOUNT_MISMATCH");
    let account = port.chatgpt.account()?;
    assert_eq!(
        account.registration,
        Some(registration("subject-1", false)?)
    );
    assert_eq!(account.tokens, Some(before));
    Ok(())
}

#[tokio::test]
async fn a_grant_without_plan_use_stores_no_tokens_and_asks_for_consent_next_time()
-> Result<(), AppError> {
    // Given: the server grants every scope except direct plan use
    let port = port();
    port.chatgpt.script_sign_in(grant("subject-1", false))?;
    let app = port.application();

    // When
    let error = failure(app.sign_in_with_chatgpt().await)?;

    // Then: no tokens, and the registration remembers that consent is needed
    assert_eq!(error.code, "CHATGPT_PLAN_USE_NOT_GRANTED");
    let account = port.chatgpt.account()?;
    assert_eq!(account.tokens, None);
    assert_eq!(account.registration, Some(registration("subject-1", true)?));
    Ok(())
}

#[tokio::test]
async fn a_denied_consent_stores_nothing_and_is_not_retried() -> Result<(), AppError> {
    // Given: the user denies consent in the browser
    let port = port();
    port.chatgpt.script_sign_in(Err(AppError::new(
        "CHATGPT_CONSENT_DENIED",
        "ChatGPT 요금제 사용 동의가 거부되었습니다.",
    )))?;
    let app = port.application();

    // When
    let error = failure(app.sign_in_with_chatgpt().await)?;

    // Then: the code survives, the server was asked once, nothing was saved
    assert_eq!(error.code, "CHATGPT_CONSENT_DENIED");
    let calls = port.chatgpt.calls()?;
    assert_eq!(calls.sign_in_requests.len(), 1);
    assert!(!calls.log.contains(&"save_session"));
    let account = port.chatgpt.account()?;
    assert_eq!(account.registration, None);
    assert_eq!(account.tokens, None);
    Ok(())
}

#[tokio::test]
async fn expired_access_token_is_refreshed_before_refinement() -> Result<(), AppError> {
    // Given: the access token is inside the refresh margin
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + SOON)),
        Some("gpt-5"),
    )?;
    port.chatgpt
        .script_refresh(Ok(tokens("access-new", "refresh-rotated", NOW + HOUR)))?;
    let app = port.application();

    // When
    refine(&app).await?;

    // Then: one refresh with the old refresh token, and the job got the new access token
    assert_eq!(port.chatgpt.calls()?.refresh_tokens, ["refresh-old"]);
    assert_eq!(
        seen(&port)?,
        [(
            "access-new".to_owned(),
            Some("gpt-5".to_owned()),
            AssistantTransport::Responses
        )]
    );
    Ok(())
}

#[tokio::test]
async fn rotated_refresh_token_is_stored_after_a_refresh() -> Result<(), AppError> {
    // Given
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + SOON)),
        Some("gpt-5"),
    )?;
    let rotated = tokens("access-new", "refresh-rotated", NOW + HOUR);
    port.chatgpt.script_refresh(Ok(rotated.clone()))?;
    let app = port.application();

    // When
    refine(&app).await?;

    // Then: the replacement is what the store holds, and a second run needs no refresh
    assert_eq!(port.chatgpt.account()?.tokens, Some(rotated));
    refine(&app).await?;
    assert_eq!(port.chatgpt.calls()?.refresh_tokens.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_fresh_access_token_is_not_refreshed() -> Result<(), AppError> {
    // Given
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + HOUR)),
        Some("gpt-5"),
    )?;
    let app = port.application();

    // When
    refine(&app).await?;

    // Then
    assert_eq!(port.chatgpt.calls()?.refresh_tokens, Vec::<String>::new());
    assert_eq!(
        seen(&port)?.first().map(|job| job.0.as_str()),
        Some("access-old")
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_refinements_with_an_expired_access_token_refresh_once() -> Result<(), AppError>
{
    // Given: two refinements that both find the token expired
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + SOON)),
        Some("gpt-5"),
    )?;
    port.chatgpt
        .script_refresh(Ok(tokens("access-new", "refresh-rotated", NOW + HOUR)))?;
    let app = port.application();
    let target = transcribed(&app).await?;

    // When
    let (first, second) = tokio::join!(
        app.refine_transcript(Uuid::now_v7(), target, &[]),
        app.refine_transcript(Uuid::now_v7(), target, &[]),
    );
    first?;
    second?;

    // Then: the second caller found the rotated token instead of refreshing again
    assert_eq!(port.chatgpt.calls()?.refresh_tokens, ["refresh-old"]);
    assert_eq!(
        seen(&port)?
            .iter()
            .map(|job| job.0.as_str())
            .collect::<Vec<_>>(),
        ["access-new", "access-new"]
    );
    Ok(())
}

#[tokio::test]
async fn an_unusable_refresh_token_clears_only_the_tokens() -> Result<(), AppError> {
    // Given
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + SOON)),
        Some("gpt-5"),
    )?;
    port.chatgpt
        .script_refresh(Err(RefreshFailure::SessionUnusable))?;
    let app = port.application();

    // When
    let error = failure(refine(&app).await)?;

    // Then: the registration stays so the next sign-in reuses the client
    assert_eq!(error.code, "CHATGPT_SIGN_IN_REQUIRED");
    assert_eq!(seen(&port)?.len(), 0);
    assert_eq!(
        app.load_chatgpt_settings().await?.account.state,
        ChatGptAccountState::SignInRequired
    );
    let account = port.chatgpt.account()?;
    assert_eq!(account.tokens, None);
    assert_eq!(
        account.registration,
        Some(registration("subject-1", false)?)
    );
    Ok(())
}

#[tokio::test]
async fn a_transient_refresh_failure_discards_nothing() -> Result<(), AppError> {
    // Given
    let port = port();
    let stored = tokens("access-old", "refresh-old", NOW + SOON);
    port.chatgpt.seed(Some(stored.clone()), Some("gpt-5"))?;
    port.chatgpt
        .script_refresh(Err(RefreshFailure::Transient))?;
    let app = port.application();

    // When
    let error = failure(refine(&app).await)?;

    // Then
    assert_eq!(error.code, "CHATGPT_REFRESH_UNAVAILABLE");
    assert_eq!(seen(&port)?.len(), 0);
    assert_eq!(port.chatgpt.account()?.tokens, Some(stored));
    assert!(!port.chatgpt.calls()?.log.contains(&"clear_tokens"));
    Ok(())
}

#[tokio::test]
async fn a_rejected_client_is_reported_without_discarding_anything() -> Result<(), AppError> {
    // Given
    let port = port();
    let stored = tokens("access-old", "refresh-old", NOW + SOON);
    port.chatgpt.seed(Some(stored.clone()), Some("gpt-5"))?;
    port.chatgpt
        .script_refresh(Err(RefreshFailure::ClientRejected))?;
    let app = port.application();

    // When
    let error = failure(refine(&app).await)?;

    // Then
    assert_eq!(error.code, "CHATGPT_CLIENT_INVALID");
    assert_eq!(port.chatgpt.account()?.tokens, Some(stored));
    Ok(())
}

#[tokio::test]
async fn chatgpt_refinement_ignores_the_api_key_endpoint_settings() -> Result<(), AppError> {
    // Given: ChatGPT mode, with leftover API-key endpoint settings saved
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + HOUR)),
        Some("gpt-5"),
    )?;
    let app = port.application();
    app.save_assistant_settings(AssistantSettings {
        api_key_stored: false,
        model: Some("glm-5-turbo".to_owned()),
        base_url: Some("https://openrouter.ai/api/v1".to_owned()),
        reasoning_effort: Some("max".to_owned()),
        background: None,
        participants: Vec::new(),
        glossary: Vec::new(),
    })
    .await?;

    // When
    refine(&app).await?;

    // Then: the selected slug, the access token, Responses, and no endpoint override
    let refinements = lock(&port.refinements)?;
    let job = refinements
        .first()
        .ok_or_else(|| AppError::new("TEST_ERROR", "refinement was not requested"))?;
    assert_eq!(job.api_key, "access-old");
    assert_eq!(job.model.as_deref(), Some("gpt-5"));
    assert_eq!(job.base_url, None);
    assert_eq!(job.reasoning_effort, None);
    assert_eq!(job.transport, AssistantTransport::Responses);
    Ok(())
}

#[tokio::test]
async fn chatgpt_refinement_needs_a_selected_model_before_any_token_work() -> Result<(), AppError> {
    // Given: signed in with an expired token but no model chosen
    let port = port();
    port.chatgpt
        .seed(Some(tokens("access-old", "refresh-old", NOW + SOON)), None)?;
    let app = port.application();

    // When
    let error = failure(refine(&app).await)?;

    // Then
    assert_eq!(error.code, "CHATGPT_MODEL_REQUIRED");
    assert_eq!(port.chatgpt.calls()?.refresh_tokens, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn chatgpt_refinement_while_signed_out_asks_to_sign_in() -> Result<(), AppError> {
    // Given: ChatGPT is selected but nobody is signed in
    let port = port();
    port.chatgpt.account()?.preferences.auth_mode = AssistantAuthMode::ChatGpt;
    let app = port.application();

    // When
    let error = failure(refine(&app).await)?;

    // Then
    assert_eq!(error.code, "CHATGPT_SIGN_IN_REQUIRED");
    assert_eq!(seen(&port)?.len(), 0);
    Ok(())
}

#[tokio::test]
async fn api_key_mode_never_touches_the_chatgpt_account() -> Result<(), AppError> {
    // Given: an expired ChatGPT session exists, but the API key is selected
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + SOON)),
        Some("gpt-5"),
    )?;
    port.chatgpt.account()?.preferences.auth_mode = AssistantAuthMode::ApiKey;
    let app = port.application();
    app.save_assistant_api_key("zai_key".to_owned()).await?;

    // When
    refine(&app).await?;

    // Then
    assert_eq!(port.chatgpt.calls()?.refresh_tokens, Vec::<String>::new());
    assert_eq!(
        seen(&port)?,
        [(
            "zai_key".to_owned(),
            None,
            AssistantTransport::ChatCompletions
        )]
    );
    Ok(())
}

#[tokio::test]
async fn a_usage_limit_failure_is_neither_retried_nor_refreshed_again() -> Result<(), AppError> {
    // Given: the worker reports that the plan's usage limit is reached
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + HOUR)),
        Some("gpt-5"),
    )?;
    *lock(&port.chatgpt.refine_failure)? = Some(AppError::new(
        "CHATGPT_USAGE_LIMIT_EXCEEDED",
        "ChatGPT 사용량 한도에 도달했습니다.",
    ));
    let app = port.application();

    // When
    let error = failure(refine(&app).await)?;

    // Then: the code reaches the caller, and the worker ran exactly once
    assert_eq!(error.code, "CHATGPT_USAGE_LIMIT_EXCEEDED");
    assert_eq!(seen(&port)?.len(), 1);
    assert_eq!(port.chatgpt.calls()?.refresh_tokens, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn sign_out_revokes_then_clears_the_session_and_returns_to_api_key_mode()
-> Result<(), AppError> {
    // Given
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + HOUR)),
        Some("gpt-5"),
    )?;
    let host_id = port.chatgpt.account()?.host_id.clone();
    let app = port.application();

    // When
    let result = app.sign_out_of_chatgpt().await?;

    // Then: revoked first, then cleared; the host id is kept
    assert!(result.remote_revocation_confirmed);
    assert_eq!(
        result.settings.account.state,
        ChatGptAccountState::SignedOut
    );
    assert_eq!(result.settings.auth_mode, AssistantAuthMode::ApiKey);
    assert_eq!(result.settings.model, None);
    let calls = port.chatgpt.calls()?;
    assert_eq!(calls.revoked, ["refresh-old"]);
    assert_eq!(calls.log, ["revoke", "clear_session"]);
    let account = port.chatgpt.account()?;
    assert_eq!(account.registration, None);
    assert_eq!(account.tokens, None);
    assert_eq!(account.host_id, host_id);
    Ok(())
}

#[tokio::test]
async fn sign_out_clears_the_session_even_when_revocation_is_unconfirmed() -> Result<(), AppError> {
    // Given: the server never confirms the revocation
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + HOUR)),
        Some("gpt-5"),
    )?;
    *lock(&port.chatgpt.revocation)? = RevocationOutcome::Unconfirmed;
    let app = port.application();

    // When
    let result = app.sign_out_of_chatgpt().await?;

    // Then: the user is told it is unconfirmed, and the local copy is gone anyway
    assert!(!result.remote_revocation_confirmed);
    assert_eq!(
        result.settings.account.state,
        ChatGptAccountState::SignedOut
    );
    assert_eq!(port.chatgpt.account()?.tokens, None);
    Ok(())
}

#[tokio::test]
async fn sign_out_without_tokens_has_nothing_to_revoke() -> Result<(), AppError> {
    // Given: a registration whose tokens were already discarded
    let port = port();
    port.chatgpt.seed(None, Some("gpt-5"))?;
    let app = port.application();

    // When
    let result = app.sign_out_of_chatgpt().await?;

    // Then
    assert!(result.remote_revocation_confirmed);
    assert_eq!(port.chatgpt.calls()?.revoked, Vec::<String>::new());
    assert_eq!(
        result.settings.account.state,
        ChatGptAccountState::SignedOut
    );
    Ok(())
}

#[tokio::test]
async fn a_second_sign_in_is_busy_while_one_waits_and_cancel_releases_it() -> Result<(), AppError> {
    // Given: a sign-in waiting for the browser
    let port = port();
    port.chatgpt.block_sign_in.store(true, Ordering::Relaxed);
    let app = port.application();

    // When: a second attempt arrives, then the user cancels the first
    let first = app.sign_in_with_chatgpt();
    let interference = async {
        // A guard against a hung sign-in, not a pause the test waits for.
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            port.chatgpt.sign_in_started.notified(),
        )
        .await
        .map_err(|_| AppError::new("TEST_ERROR", "the sign-in never started waiting"))?;
        let busy = failure(app.sign_in_with_chatgpt().await)?;
        app.cancel_chatgpt_sign_in()?;
        Ok::<_, AppError>(busy)
    };
    let (first, busy) = tokio::join!(first, interference);

    // Then: the second was refused, the first ended as cancelled
    assert_eq!(busy?.code, "CHATGPT_SIGN_IN_BUSY");
    assert_eq!(failure(first)?.code, "CANCELLED");
    assert_eq!(port.chatgpt.calls()?.sign_in_requests.len(), 1);

    // And the slot is free again
    port.chatgpt.block_sign_in.store(false, Ordering::Relaxed);
    port.chatgpt.script_sign_in(grant("subject-1", true))?;
    let settings = app.sign_in_with_chatgpt().await?;
    assert_eq!(settings.account.state, ChatGptAccountState::SignedIn);
    Ok(())
}

#[tokio::test]
async fn cancelling_without_a_sign_in_is_harmless() -> Result<(), AppError> {
    let port = port();
    let app = port.application();

    app.cancel_chatgpt_sign_in()?;

    assert_eq!(port.chatgpt.calls()?.sign_in_requests.len(), 0);
    Ok(())
}

#[tokio::test]
async fn expired_access_token_refreshes_before_listing_models() -> Result<(), AppError> {
    // Given: the sheet is opened after the access token expired
    let port = port();
    port.chatgpt.seed(
        Some(tokens("access-old", "refresh-old", NOW + SOON)),
        Some("gpt-5"),
    )?;
    port.chatgpt
        .script_refresh(Ok(tokens("access-new", "refresh-rotated", NOW + HOUR)))?;
    port.chatgpt
        .set_models(&[("gpt-5", "GPT-5"), ("gpt-5-mini", "GPT-5 mini")])?;
    let app = port.application();

    // When
    let models = app.list_chatgpt_models().await?;

    // Then: the list was fetched with the rotated access token, in server order
    let slugs: Vec<&str> = models.iter().map(|model| model.slug.as_str()).collect();
    assert_eq!(slugs, ["gpt-5", "gpt-5-mini"]);
    let calls = port.chatgpt.calls()?;
    assert_eq!(calls.refresh_tokens, ["refresh-old"]);
    assert_eq!(calls.models_tokens, ["access-new"]);
    Ok(())
}

#[tokio::test]
async fn listing_models_while_signed_out_asks_to_sign_in() -> Result<(), AppError> {
    // Given
    let port = port();
    let app = port.application();

    // When
    let error = failure(app.list_chatgpt_models().await)?;

    // Then: no request went out
    assert_eq!(error.code, "CHATGPT_SIGN_IN_REQUIRED");
    assert_eq!(port.chatgpt.calls()?.models_tokens, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn saved_preferences_are_trimmed_and_a_blank_model_is_cleared() -> Result<(), AppError> {
    // Given
    let port = port();
    let app = port.application();

    // When
    app.save_chatgpt_preferences(ChatGptPreferences {
        auth_mode: AssistantAuthMode::ChatGpt,
        model: Some("  gpt-5  ".to_owned()),
        welcome_acknowledged: true,
    })
    .await?;
    let trimmed = app.load_chatgpt_settings().await?;
    app.save_chatgpt_preferences(ChatGptPreferences {
        auth_mode: AssistantAuthMode::ChatGpt,
        model: Some("   ".to_owned()),
        welcome_acknowledged: true,
    })
    .await?;
    let blank = app.load_chatgpt_settings().await?;

    // Then
    assert_eq!(trimmed.model.as_deref(), Some("gpt-5"));
    assert_eq!(blank.model, None);
    assert!(blank.welcome_acknowledged);
    Ok(())
}
