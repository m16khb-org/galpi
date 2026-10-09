use super::{GatewayOAuthAdapter, Tuning};
use crate::adapters::outbound::browser_sign_in::fake_server::{FakeServer, Handler, Recorded};
use crate::adapters::outbound::browser_sign_in::pkce::code_challenge;
use crate::adapters::outbound::browser_sign_in::{BrowserOpener, SignInCodes};
use crate::application::error::AppError;
use crate::application::ports::{ClockPort, GatewayAuthPort};
use crate::domain::gateway::{GatewayRefreshFailure, GatewayTokens};
use reqwest::Url;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

const NOW: u64 = 1_000_000;

/// What the browser "user" sends back, given the `state` of the request.
type Behavior = Box<dyn Fn(&str) -> String + Send + Sync>;

struct FakeBrowser {
    behavior: Behavior,
    urls: Mutex<Vec<String>>,
}

impl BrowserOpener for FakeBrowser {
    fn open(&self, url: &str, _codes: SignInCodes) -> Result<(), AppError> {
        let parsed = Url::parse(url).map_err(|_| AppError::new("TEST", "bad url"))?;
        let param = |name: &str| query(&parsed, name).unwrap_or_default();
        if let Ok(mut urls) = self.urls.lock() {
            urls.push(url.to_owned());
        }
        let target = format!(
            "{}?{}",
            param("redirect_uri"),
            (self.behavior)(&param("state"))
        );
        tokio::spawn(async move {
            let _response = reqwest::get(target).await;
        });
        Ok(())
    }
}

struct FixedClock;

impl ClockPort for FixedClock {
    fn now_unix(&self) -> u64 {
        NOW
    }
}

fn query(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

fn approve() -> Behavior {
    Box::new(|state| format!("state={state}&code=galpi-test-code"))
}

fn json_body(request: &Recorded) -> Value {
    serde_json::from_str(&request.body).unwrap_or(Value::Null)
}

fn token_body(refresh: &str) -> String {
    json!({
        "access_token": "galpi-test-access",
        "refresh_token": refresh,
        "expires_in": 3600,
        "token_type": "bearer",
    })
    .to_string()
}

/// A gateway that issues tokens and answers `/me` with `me`.
fn gateway(me: (u16, String)) -> Handler {
    Arc::new(move |request| match request.path.as_str() {
        "/auth/native/token" => (200, token_body("galpi-test-refresh-1")),
        "/me" => me.clone(),
        _ => (404, String::new()),
    })
}

struct Harness {
    server: FakeServer,
    browser: Arc<FakeBrowser>,
    adapter: GatewayOAuthAdapter,
}

impl Harness {
    async fn start(behavior: Behavior, handler: Handler) -> Result<Self, AppError> {
        let server = FakeServer::start(handler)
            .await
            .map_err(|error| AppError::io("server", &error))?;
        let browser = Arc::new(FakeBrowser {
            behavior,
            urls: Mutex::new(Vec::new()),
        });
        let adapter = GatewayOAuthAdapter::with_tuning(
            Tuning {
                base_url: server.base.clone(),
                preferred_port: 0,
                sign_in_timeout: Duration::from_secs(30),
            },
            Arc::clone(&browser) as Arc<dyn BrowserOpener>,
            Arc::new(FixedClock),
        )?;
        Ok(Self {
            server,
            browser,
            adapter,
        })
    }

    fn opened_url(&self) -> Result<Url, AppError> {
        let urls = self
            .browser
            .urls
            .lock()
            .map_err(|_| AppError::new("TEST", "urls lock"))?;
        let url = urls
            .first()
            .ok_or_else(|| AppError::new("TEST", "no browser was opened"))?;
        Url::parse(url).map_err(|_| AppError::new("TEST", "bad url"))
    }
}

async fn signed_in(harness: &Harness) -> Result<super::GatewayGrant, AppError> {
    let (_cancel, mut receiver) = oneshot::channel();
    harness.adapter.sign_in(&mut receiver).await
}

#[tokio::test]
async fn sign_in_opens_the_gateway_with_pkce_and_exchanges_the_code() -> Result<(), AppError> {
    // Given
    let me = (
        200,
        json!({"sub": "u1", "email": "user@example.com"}).to_string(),
    );
    let harness = Harness::start(approve(), gateway(me)).await?;

    // When
    let grant = signed_in(&harness).await?;

    // Then: the browser was sent to the gateway's Google entry point
    let url = harness.opened_url()?;
    assert_eq!(url.path(), "/auth/native/google");
    let redirect_uri = query(&url, "redirect_uri").unwrap_or_default();
    assert!(redirect_uri.starts_with("http://127.0.0.1:"));
    assert!(redirect_uri.ends_with("/auth/callback"));
    assert_eq!(
        query(&url, "code_challenge_method").as_deref(),
        Some("S256")
    );
    assert_eq!(query(&url, "state").map(|state| state.len()), Some(43));

    // And the code was exchanged with the verifier behind the challenge
    let exchange = harness
        .server
        .requests()
        .into_iter()
        .find(|request| request.path == "/auth/native/token")
        .ok_or_else(|| AppError::new("TEST", "no exchange"))?;
    let body = json_body(&exchange);
    assert_eq!(exchange.method, "POST");
    assert_eq!(body["grant_type"], "authorization_code");
    assert_eq!(body["code"], "galpi-test-code");
    assert_eq!(body["redirect_uri"], redirect_uri.as_str());
    let verifier = body["code_verifier"].as_str().unwrap_or_default();
    assert_eq!(
        query(&url, "code_challenge"),
        Some(code_challenge(verifier))
    );
    assert_eq!(body.as_object().map(serde_json::Map::len), Some(4));

    // And the grant carries the tokens and who signed in
    assert_eq!(
        grant.tokens,
        GatewayTokens {
            access_token: "galpi-test-access".to_owned(),
            refresh_token: "galpi-test-refresh-1".to_owned(),
            expires_at: NOW + 3600,
        }
    );
    assert_eq!(grant.email.as_deref(), Some("user@example.com"));
    let me = harness
        .server
        .requests()
        .into_iter()
        .find(|request| request.path == "/me")
        .ok_or_else(|| AppError::new("TEST", "no /me"))?;
    assert_eq!(
        me.authorization.as_deref(),
        Some("Bearer galpi-test-access")
    );
    Ok(())
}

#[tokio::test]
async fn sign_in_succeeds_without_an_email_when_me_fails() -> Result<(), AppError> {
    let harness = Harness::start(approve(), gateway((503, String::new()))).await?;

    let grant = signed_in(&harness).await?;

    assert_eq!(grant.email, None);
    assert_eq!(grant.tokens.refresh_token, "galpi-test-refresh-1");
    Ok(())
}

#[tokio::test]
async fn a_callback_for_another_attempt_is_refused_before_any_exchange() -> Result<(), AppError> {
    let forged: Behavior = Box::new(|_| "state=forged&code=galpi-test-code".to_owned());
    let harness = Harness::start(forged, gateway((200, "{}".to_owned()))).await?;

    let error = signed_in(&harness).await.err();

    assert_eq!(
        error.map(|error| error.code),
        Some("GATEWAY_SIGN_IN_FAILED".to_owned())
    );
    assert_eq!(harness.server.count("/auth/native/token"), 0);
    Ok(())
}

#[tokio::test]
async fn a_denied_sign_in_is_reported_without_any_exchange() -> Result<(), AppError> {
    let denied: Behavior = Box::new(|state| format!("state={state}&error=access_denied"));
    let harness = Harness::start(denied, gateway((200, "{}".to_owned()))).await?;

    let error = signed_in(&harness).await.err();

    assert_eq!(
        error.map(|error| error.code),
        Some("GATEWAY_SIGN_IN_FAILED".to_owned())
    );
    assert_eq!(harness.server.count("/auth/native/token"), 0);
    Ok(())
}

async fn refresh_against(
    status: u16,
    body: &str,
) -> Result<Result<GatewayTokens, GatewayRefreshFailure>, AppError> {
    let body = body.to_owned();
    let harness = Harness::start(approve(), Arc::new(move |_| (status, body.clone()))).await?;
    Ok(harness.adapter.refresh("galpi-test-refresh-old").await)
}

#[tokio::test]
async fn refresh_posts_the_refresh_token_and_returns_the_rotated_session() -> Result<(), AppError> {
    let harness = Harness::start(
        approve(),
        Arc::new(|_| (200, token_body("galpi-test-refresh-new"))),
    )
    .await?;

    let tokens = harness.adapter.refresh("galpi-test-refresh-old").await;

    assert_eq!(
        tokens.map(|tokens| tokens.refresh_token),
        Ok("galpi-test-refresh-new".to_owned())
    );
    let requests = harness.server.requests();
    let body = requests.first().map_or(Value::Null, json_body);
    assert_eq!(
        body,
        json!({"grant_type": "refresh_token", "refresh_token": "galpi-test-refresh-old"})
    );
    Ok(())
}

#[tokio::test]
async fn only_invalid_grant_rejects_the_session() -> Result<(), AppError> {
    let invalid_grant = json!({"error": "invalid_grant"}).to_string();
    let invalid_request = json!({"error": "invalid_request"}).to_string();
    let unavailable = json!({"error": "temporarily_unavailable"}).to_string();

    let outcomes = [
        refresh_against(400, &invalid_grant).await?,
        refresh_against(400, &invalid_request).await?,
        refresh_against(400, "").await?,
        refresh_against(401, &invalid_grant).await?,
        refresh_against(429, "").await?,
        refresh_against(503, &unavailable).await?,
        refresh_against(200, "{not json").await?,
    ];

    let rejected = GatewayRefreshFailure::Rejected;
    let kept = GatewayRefreshFailure::Unavailable;
    assert_eq!(
        outcomes.map(Result::err),
        [
            Some(rejected),
            Some(kept),
            Some(kept),
            Some(kept),
            Some(kept),
            Some(kept),
            Some(kept)
        ]
    );
    Ok(())
}

#[tokio::test]
async fn an_unreachable_gateway_keeps_the_session() -> Result<(), AppError> {
    // Given: a port nothing listens on
    let closed = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| AppError::io("bind", &error))?;
    let port = closed
        .local_addr()
        .map_err(|error| AppError::io("addr", &error))?
        .port();
    drop(closed);
    let adapter = GatewayOAuthAdapter::with_tuning(
        Tuning {
            base_url: format!("http://127.0.0.1:{port}"),
            preferred_port: 0,
            sign_in_timeout: Duration::from_secs(30),
        },
        Arc::new(FakeBrowser {
            behavior: approve(),
            urls: Mutex::new(Vec::new()),
        }),
        Arc::new(FixedClock),
    )?;

    // When
    let outcome = adapter.refresh("galpi-test-refresh-old").await;

    // Then
    assert_eq!(outcome, Err(GatewayRefreshFailure::Unavailable));
    Ok(())
}

#[tokio::test]
async fn revoke_posts_the_refresh_token_to_the_logout_endpoint() -> Result<(), AppError> {
    let harness = Harness::start(approve(), Arc::new(|_| (500, String::new()))).await?;

    harness.adapter.revoke("galpi-test-refresh-old").await;

    let requests = harness.server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests.first().map(|request| request.path.as_str()),
        Some("/auth/native/logout")
    );
    assert_eq!(
        requests.first().map(json_body),
        Some(json!({"refresh_token": "galpi-test-refresh-old"}))
    );
    Ok(())
}
