//! A fake OpenAI server, fake browser, and recording events around the adapter.

use crate::adapters::outbound::browser_sign_in::fake_server::{FakeServer, Recorded};
use crate::adapters::outbound::browser_sign_in::{BrowserOpener, SignInCodes};
use crate::adapters::outbound::chatgpt::oauth::RevocationPolicy;
use crate::adapters::outbound::chatgpt::testing::{KEY_ID, claims, jwks_json, sign};
use crate::adapters::outbound::chatgpt::{ChatGptOAuthAdapter, OpenAiEndpoints, Tuning};
use crate::application::error::AppError;
use crate::application::ports::{ChatGptEvents, ClockPort};
use crate::domain::chatgpt::{AgentHostId, ChatGptRegistration, ChatGptSignInEvent, SignInRequest};
use reqwest::Url;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const NOW: u64 = 1_000_000;
pub const GRANTED: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

/// What the browser "user" sends back, given the `state` of the request.
pub type Behavior = Box<dyn Fn(&str) -> String + Send + Sync>;

pub fn approve(client_id: Option<&'static str>) -> Behavior {
    Box::new(move |state| {
        let issued = client_id.map_or(String::new(), |id| format!("&client_id={id}"));
        format!("state={state}&code=code-1{issued}")
    })
}

pub fn deny() -> Behavior {
    Box::new(|state| format!("state={state}&error=access_denied"))
}

pub fn wrong_state() -> Behavior {
    Box::new(|_| "state=forged&code=code-1&client_id=oaiapp_new".to_owned())
}

pub struct FakeBrowser {
    behavior: Behavior,
    pub urls: Mutex<Vec<String>>,
    nonce: Arc<Mutex<String>>,
}

impl BrowserOpener for FakeBrowser {
    fn open(&self, url: &str, _codes: SignInCodes) -> Result<(), AppError> {
        let parsed = Url::parse(url).map_err(|_| AppError::new("TEST", "bad url"))?;
        let param = |name: &str| {
            parsed
                .query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default()
        };
        if let Ok(mut nonce) = self.nonce.lock() {
            *nonce = param("nonce");
        }
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

#[derive(Default)]
pub struct RecordedEvents(pub Mutex<Vec<ChatGptSignInEvent>>);

impl ChatGptEvents for RecordedEvents {
    fn emit(&self, event: ChatGptSignInEvent) -> Result<(), AppError> {
        if let Ok(mut events) = self.0.lock() {
            events.push(event);
        }
        Ok(())
    }
}

struct FixedClock;

impl ClockPort for FixedClock {
    fn now_unix(&self) -> u64 {
        NOW
    }
}

pub struct Harness {
    pub server: FakeServer,
    pub browser: Arc<FakeBrowser>,
    pub events: Arc<RecordedEvents>,
    pub adapter: ChatGptOAuthAdapter,
}

/// Answers `/token`: the code exchange mints an ID token for the posted client.
pub type TokenHandler = Arc<dyn Fn(&Recorded, &str) -> (u16, String) + Send + Sync>;

pub fn issuing_tokens() -> TokenHandler {
    Arc::new(|request, nonce| {
        let id_token = sign(
            &claims(
                &request.field("client_id").unwrap_or_default(),
                nonce,
                NOW + 3600,
            ),
            Some(KEY_ID),
        );
        let body = json!({
            "access_token": "access-1", "refresh_token": "refresh-1", "id_token": id_token,
            "expires_in": 3600, "scope": GRANTED, "token_type": "Bearer",
        });
        (200, body.to_string())
    })
}

impl Harness {
    pub async fn start(behavior: Behavior, token: TokenHandler) -> Result<Self, std::io::Error> {
        let nonce = Arc::new(Mutex::new(String::new()));
        let seen = Arc::clone(&nonce);
        let server = FakeServer::start(Arc::new(move |request| match request.path.as_str() {
            "/jwks" => (200, jwks_json().to_string()),
            "/models" => (200, json!({"models": []}).to_string()),
            _ => token(request, &seen.lock().map(|n| n.clone()).unwrap_or_default()),
        }))
        .await?;
        Self::with_server(server, behavior, nonce)
    }

    pub fn with_server(
        server: FakeServer,
        behavior: Behavior,
        nonce: Arc<Mutex<String>>,
    ) -> Result<Self, std::io::Error> {
        let browser = Arc::new(FakeBrowser {
            behavior,
            urls: Mutex::new(Vec::new()),
            nonce,
        });
        let events = Arc::new(RecordedEvents::default());
        let tuning = Tuning {
            endpoints: OpenAiEndpoints {
                authorize: "https://auth.example.test/authorize".to_owned(),
                token: server.url("/token"),
                revoke: server.url("/revoke"),
                jwks: server.url("/jwks"),
                issuer: "https://auth.openai.com".to_owned(),
                models: server.url("/models"),
            },
            revocation: RevocationPolicy {
                attempts: 3,
                backoff: Duration::ZERO,
            },
            preferred_port: 0,
            sign_in_timeout: Duration::from_secs(30),
        };
        let adapter = ChatGptOAuthAdapter::with_tuning(
            tuning,
            Arc::clone(&browser) as Arc<dyn BrowserOpener>,
            Arc::clone(&events) as Arc<dyn ChatGptEvents>,
            Arc::new(FixedClock),
        )
        .map_err(|error| std::io::Error::other(error.message))?;
        Ok(Self {
            server,
            browser,
            events,
            adapter,
        })
    }
}

pub fn new_request() -> SignInRequest {
    SignInRequest {
        host_id: AgentHostId::generate(),
        registration: None,
    }
}

pub fn registration(needs_consent: bool) -> Result<ChatGptRegistration, std::io::Error> {
    ChatGptRegistration::new(
        "oaiapp_stored".to_owned(),
        "user-1".to_owned(),
        Some("user@example.com".to_owned()),
        needs_consent,
    )
    .map_err(|_| std::io::Error::other("registration"))
}
