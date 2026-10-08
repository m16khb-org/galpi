use super::support::{
    Behavior, GRANTED, Harness, NOW, approve, deny, issuing_tokens, new_request, registration,
    wrong_state,
};
use crate::adapters::outbound::chatgpt::pkce::code_challenge;
use crate::application::error::AppError;
use crate::application::ports::ChatGptAuthPort;
use crate::domain::chatgpt::{ChatGptSignInEvent, ChatGptSignInPhase, SignInGrant, SignInRequest};
use reqwest::Url;
use std::sync::Arc;
use tokio::sync::oneshot;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn run(
    behavior: Behavior,
    request: SignInRequest,
) -> Result<(Harness, Result<SignInGrant, AppError>), Box<dyn std::error::Error>> {
    let harness = Harness::start(behavior, issuing_tokens()).await?;
    let (_keep, mut cancel) = oneshot::channel();
    let outcome = harness.adapter.sign_in(&request, &mut cancel).await;
    Ok((harness, outcome))
}

fn opened(harness: &Harness) -> Result<Url, Box<dyn std::error::Error>> {
    let urls = harness.browser.urls.lock().map_err(|_| "poisoned")?;
    Ok(Url::parse(urls.first().ok_or("browser never opened")?)?)
}

fn param(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

#[tokio::test]
async fn first_sign_in_registers_a_new_client_and_exchanges_with_the_issued_id() -> TestResult {
    // Given a new installation
    let request = new_request();

    // When the user approves and the browser reports an issued client id
    let (harness, outcome) = run(approve(Some("oaiapp_new")), request.clone()).await?;
    let grant = outcome?;

    // Then the authorization request carries the documented contract
    let url = opened(&harness)?;
    assert_eq!(
        param(&url, "client_id").as_deref(),
        Some("dynamic_agent_client")
    );
    assert_eq!(param(&url, "agent_name_hint").as_deref(), Some("Galpi"));
    assert_eq!(
        param(&url, "ext_agent_host_id").as_deref(),
        Some(request.host_id.as_str())
    );
    assert_eq!(param(&url, "scope").as_deref(), Some(GRANTED));
    assert_eq!(
        param(&url, "resource").as_deref(),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(param(&url, "response_type").as_deref(), Some("code"));
    assert_eq!(
        param(&url, "code_challenge_method").as_deref(),
        Some("S256")
    );
    assert_eq!(param(&url, "login_hint"), None);
    assert_eq!(param(&url, "prompt"), None);
    let redirect = param(&url, "redirect_uri").ok_or("redirect_uri")?;
    assert!(redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/auth/callback"));

    // And the code is exchanged with the issued client and the matching verifier
    let exchanges = harness.server.requests();
    let exchange = exchanges
        .iter()
        .find(|r| r.path == "/token")
        .ok_or("no exchange")?;
    assert_eq!(
        exchange.field("grant_type").as_deref(),
        Some("authorization_code")
    );
    assert_eq!(exchange.field("client_id").as_deref(), Some("oaiapp_new"));
    assert_eq!(exchange.field("code").as_deref(), Some("code-1"));
    assert_eq!(exchange.field("redirect_uri"), Some(redirect));
    assert_eq!(
        exchange.field("resource").as_deref(),
        Some("https://api.openai.com/v1")
    );
    let verifier = exchange.field("code_verifier").ok_or("verifier")?;
    assert_eq!(
        param(&url, "code_challenge"),
        Some(code_challenge(&verifier))
    );

    // And the grant is built from the verified token response
    assert_eq!(grant.registration.client_id(), "oaiapp_new");
    assert_eq!(grant.registration.subject(), "user-1");
    assert_eq!(grant.registration.email(), Some("user@example.com"));
    assert_eq!(grant.tokens.expires_at, NOW + 3600);
    assert!(grant.tokens.grants_plan_usage());
    let events = harness.events.0.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(
        events
            .iter()
            .map(|event: &ChatGptSignInEvent| event.phase)
            .collect::<Vec<_>>(),
        [
            ChatGptSignInPhase::AwaitingBrowser,
            ChatGptSignInPhase::Exchanging
        ]
    );
    Ok(())
}

#[tokio::test]
async fn sign_in_again_reuses_the_issued_client_without_a_name_hint() -> TestResult {
    // Given a stored registration that needs consent
    let request = SignInRequest {
        registration: Some(registration(true)?),
        ..new_request()
    };

    // When the callback omits the client id
    let (harness, outcome) = run(approve(None), request).await?;
    let grant = outcome?;

    // Then the stored client, a login hint, and prompt=consent are used
    let url = opened(&harness)?;
    assert_eq!(param(&url, "client_id").as_deref(), Some("oaiapp_stored"));
    assert_eq!(param(&url, "agent_name_hint"), None);
    assert_eq!(
        param(&url, "login_hint").as_deref(),
        Some("user@example.com")
    );
    assert_eq!(param(&url, "prompt").as_deref(), Some("consent"));
    let requests = harness.server.requests();
    let exchange = requests
        .iter()
        .find(|r| r.path == "/token")
        .ok_or("no exchange")?;
    assert_eq!(
        exchange.field("client_id").as_deref(),
        Some("oaiapp_stored")
    );
    assert_eq!(grant.registration.client_id(), "oaiapp_stored");
    Ok(())
}

#[tokio::test]
async fn no_prompt_is_sent_when_consent_is_not_needed() -> TestResult {
    let request = SignInRequest {
        registration: Some(registration(false)?),
        ..new_request()
    };

    let (harness, outcome) = run(approve(None), request).await?;
    outcome?;

    assert_eq!(param(&opened(&harness)?, "prompt"), None);
    Ok(())
}

async fn refused_without_exchange(
    behavior: Behavior,
    request: SignInRequest,
) -> Result<(Harness, AppError), Box<dyn std::error::Error>> {
    let (harness, outcome) = run(behavior, request).await?;
    let error = outcome.err().ok_or("expected a failure")?;
    assert_eq!(harness.server.count("/token"), 0);
    Ok((harness, error))
}

#[tokio::test]
async fn a_forged_state_never_reaches_the_token_endpoint() -> TestResult {
    let (_, error) = refused_without_exchange(wrong_state(), new_request()).await?;
    assert_eq!(error.code, "CHATGPT_STATE_MISMATCH");
    Ok(())
}

#[tokio::test]
async fn access_denied_never_reaches_the_token_endpoint() -> TestResult {
    let (_, error) = refused_without_exchange(deny(), new_request()).await?;
    assert_eq!(error.code, "CHATGPT_CONSENT_DENIED");
    Ok(())
}

#[tokio::test]
async fn a_new_registration_without_an_issued_client_id_is_incomplete() -> TestResult {
    let (_, missing) = refused_without_exchange(approve(None), new_request()).await?;
    let (_, dynamic) =
        refused_without_exchange(approve(Some("dynamic_agent_client")), new_request()).await?;
    assert_eq!(missing.code, "CHATGPT_REGISTRATION_INCOMPLETE");
    assert_eq!(dynamic.code, "CHATGPT_REGISTRATION_INCOMPLETE");
    Ok(())
}

#[tokio::test]
async fn a_different_client_id_on_sign_in_again_is_a_mismatch() -> TestResult {
    let request = SignInRequest {
        registration: Some(registration(false)?),
        ..new_request()
    };
    let (_, error) = refused_without_exchange(approve(Some("oaiapp_other")), request).await?;
    assert_eq!(error.code, "CHATGPT_CLIENT_MISMATCH");
    Ok(())
}

#[tokio::test]
async fn a_replayed_id_token_nonce_is_rejected() -> TestResult {
    // Given a server whose ID token was minted for another attempt
    let stale =
        Arc::new(|request: &_, _nonce: &str| issuing_tokens()(request, "someone-elses-nonce"));
    let harness = Harness::start(approve(Some("oaiapp_new")), stale).await?;
    let (_keep, mut cancel) = oneshot::channel();

    // When
    let outcome = harness.adapter.sign_in(&new_request(), &mut cancel).await;

    // Then
    assert_eq!(
        outcome.err().map(|e| e.code).as_deref(),
        Some("CHATGPT_ID_TOKEN_INVALID")
    );
    Ok(())
}

#[tokio::test]
async fn failures_never_echo_the_authorization_url_or_state() -> TestResult {
    // Given a token endpoint that rejects the code
    let reject = Arc::new(|_: &_, _: &str| (400, r#"{"error":"invalid_grant"}"#.to_owned()));
    let harness = Harness::start(approve(Some("oaiapp_new")), reject).await?;
    let (_keep, mut cancel) = oneshot::channel();

    // When
    let error = harness
        .adapter
        .sign_in(&new_request(), &mut cancel)
        .await
        .err()
        .ok_or("expected a failure")?;

    // Then neither the URL nor its secrets are in the error
    let url = opened(&harness)?;
    let state = param(&url, "state").ok_or("state")?;
    let shown = format!("{error} {error:?}");
    assert_eq!(error.code, "CHATGPT_SIGN_IN_FAILED");
    assert!(!shown.contains(url.as_str()));
    assert!(!shown.contains(&state));
    assert!(!shown.contains("code-1"));
    Ok(())
}
