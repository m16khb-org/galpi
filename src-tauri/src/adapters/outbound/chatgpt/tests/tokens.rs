use super::support::{Harness, approve, registration};
use crate::adapters::outbound::browser_sign_in::fake_server::FakeServer;
use crate::application::ports::ChatGptAuthPort;
use crate::domain::chatgpt::{RefreshFailure, RevocationOutcome};
use std::sync::{Arc, Mutex};

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn with_token_response(status: u16, body: &'static str) -> Result<Harness, std::io::Error> {
    let server = FakeServer::start(Arc::new(move |_| (status, body.to_owned()))).await?;
    Harness::with_server(server, approve(None), Arc::new(Mutex::new(String::new())))
}

#[tokio::test]
async fn refresh_posts_the_documented_form_and_returns_rotated_tokens() -> TestResult {
    // Given a server that rotates the refresh token
    let body = r#"{"access_token":"access-2","refresh_token":"refresh-2","expires_in":3600,
        "scope":"openid chatgpt.tokens.use.direct"}"#;
    let harness = with_token_response(200, body).await?;

    // When
    let tokens = harness
        .adapter
        .refresh(&registration(false)?, "refresh-1")
        .await;

    // Then
    let tokens = tokens.map_err(|_| "refresh failed")?;
    assert_eq!(tokens.refresh_token, "refresh-2");
    assert_eq!(tokens.access_token, "access-2");
    assert_eq!(tokens.expires_at, super::support::NOW + 3600);
    let request = harness.server.requests().remove(0);
    assert_eq!(
        request.field("grant_type").as_deref(),
        Some("refresh_token")
    );
    assert_eq!(request.field("client_id").as_deref(), Some("oaiapp_stored"));
    assert_eq!(request.field("refresh_token").as_deref(), Some("refresh-1"));
    assert_eq!(
        request.field("resource").as_deref(),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(request.field("scope"), None);
    Ok(())
}

#[tokio::test]
async fn refresh_failures_are_classified_by_what_can_still_work() -> TestResult {
    let unusable = [
        "invalid_grant",
        "invalid_refresh_token",
        "token_expired",
        "refresh_token_expired",
        "refresh_token_invalidated",
        "refresh_token_reused",
    ];
    for code in unusable {
        let body: &'static str = Box::leak(format!(r#"{{"error":"{code}"}}"#).into_boxed_str());
        let harness = with_token_response(400, body).await?;
        let result = harness.adapter.refresh(&registration(false)?, "r").await;
        assert_eq!(
            result.err(),
            Some(RefreshFailure::SessionUnusable),
            "{code}"
        );
    }
    let nested = with_token_response(401, r#"{"error":{"code":"refresh_token_reused"}}"#).await?;
    assert_eq!(
        nested
            .adapter
            .refresh(&registration(false)?, "r")
            .await
            .err(),
        Some(RefreshFailure::SessionUnusable)
    );

    let client = with_token_response(401, r#"{"error":"invalid_client"}"#).await?;
    assert_eq!(
        client
            .adapter
            .refresh(&registration(false)?, "r")
            .await
            .err(),
        Some(RefreshFailure::ClientRejected)
    );
    for (status, body) in [
        (503, "{}"),
        (500, r#"{"error":"invalid_grant"}"#),
        (429, "{}"),
        (200, "nope"),
    ] {
        let harness = with_token_response(status, body).await?;
        assert_eq!(
            harness
                .adapter
                .refresh(&registration(false)?, "r")
                .await
                .err(),
            Some(RefreshFailure::Transient),
            "{status}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_unreachable_server_is_a_transient_refresh_failure() -> TestResult {
    let harness = with_token_response(200, "{}").await?;
    let dead = Harness::with_server(
        FakeServer::start(Arc::new(|_| (200, String::new()))).await?,
        approve(None),
        Arc::new(Mutex::new(String::new())),
    )?;
    drop(dead.server);
    drop(harness);

    let result = dead.adapter.refresh(&registration(false)?, "r").await;

    assert_eq!(result.err(), Some(RefreshFailure::Transient));
    Ok(())
}

#[tokio::test]
async fn revoke_posts_the_documented_form_and_confirms() -> TestResult {
    let harness = with_token_response(200, "{}").await?;

    let outcome = harness
        .adapter
        .revoke(&registration(false)?, "refresh-1")
        .await;

    assert_eq!(outcome, RevocationOutcome::Confirmed);
    let requests = harness.server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/revoke");
    assert_eq!(requests[0].field("token").as_deref(), Some("refresh-1"));
    assert_eq!(
        requests[0].field("token_type_hint").as_deref(),
        Some("refresh_token")
    );
    assert_eq!(
        requests[0].field("client_id").as_deref(),
        Some("oaiapp_stored")
    );
    Ok(())
}

#[tokio::test]
async fn revoke_retries_server_errors_per_policy_then_gives_up() -> TestResult {
    let harness = with_token_response(503, "{}").await?;

    let outcome = harness.adapter.revoke(&registration(false)?, "r").await;

    assert_eq!(outcome, RevocationOutcome::Unconfirmed);
    assert_eq!(harness.server.count("/revoke"), 3);
    Ok(())
}

#[tokio::test]
async fn revoke_does_not_retry_a_client_error() -> TestResult {
    let harness = with_token_response(400, "{}").await?;

    let outcome = harness.adapter.revoke(&registration(false)?, "r").await;

    assert_eq!(outcome, RevocationOutcome::Unconfirmed);
    assert_eq!(harness.server.count("/revoke"), 1);
    Ok(())
}
