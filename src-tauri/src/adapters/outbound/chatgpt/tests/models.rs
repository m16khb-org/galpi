use super::support::{Harness, approve};
use crate::adapters::outbound::chatgpt::testing::server::FakeServer;
use crate::application::ports::ChatGptAuthPort;
use serde_json::json;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn only_listed_models_come_back_in_server_order_with_a_bearer_token()
-> Result<(), Box<dyn std::error::Error>> {
    // Given a server that mixes listed and hidden models
    let body = json!({"models": [
        {"slug": "gpt-z", "display_name": "Zed", "visibility": "list"},
        {"slug": "hidden", "display_name": "Hidden", "visibility": "hide"},
        {"slug": "gpt-a", "display_name": "Ay", "visibility": "list"},
        {"slug": "no-visibility", "display_name": "None"},
    ]})
    .to_string();
    let server = FakeServer::start(Arc::new(move |_| (200, body.clone()))).await?;
    let harness = Harness::with_server(server, approve(None), Arc::new(Mutex::new(String::new())))?;

    // When
    let models = harness.adapter.list_models("access-1").await?;

    // Then
    let slugs: Vec<&str> = models.iter().map(|model| model.slug.as_str()).collect();
    assert_eq!(slugs, ["gpt-z", "gpt-a"]);
    assert_eq!(models[0].display_name, "Zed");
    let requests = harness.server.requests();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer access-1")
    );
    Ok(())
}

#[tokio::test]
async fn a_failing_models_request_is_reported_without_the_token()
-> Result<(), Box<dyn std::error::Error>> {
    let server = FakeServer::start(Arc::new(|_| (503, "{}".to_owned()))).await?;
    let harness = Harness::with_server(server, approve(None), Arc::new(Mutex::new(String::new())))?;

    let error = harness.adapter.list_models("access-secret").await.err();

    let error = error.ok_or("expected an error")?;
    assert_eq!(error.code, "CHATGPT_MODELS_UNAVAILABLE");
    assert!(!error.message.contains("access-secret"));
    Ok(())
}
