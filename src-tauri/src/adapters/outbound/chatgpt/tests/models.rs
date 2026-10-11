use super::support::{Harness, approve};
use crate::adapters::outbound::browser_sign_in::fake_server::FakeServer;
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
async fn each_model_carries_the_efforts_it_accepts() -> Result<(), Box<dyn std::error::Error>> {
    // Given levels in the catalog's shape, one unknown effort, one odd shape,
    // and a model that says nothing about effort
    let body = json!({"models": [
        {"slug": "gpt-r", "display_name": "R", "visibility": "list",
         "default_reasoning_level": "medium",
         "supported_reasoning_levels": [
            {"effort": "low", "description": "fast"},
            {"effort": "Medium", "description": "balanced"},
            {"effort": "turbo", "description": "not an API effort"},
            {"effort": "xhigh", "description": "deep"},
         ]},
        {"slug": "gpt-odd", "display_name": "Odd", "visibility": "list",
         "supported_reasoning_levels": {"effort": "low"}},
        {"slug": "gpt-plain", "display_name": "Plain", "visibility": "list"},
    ]})
    .to_string();
    let server = FakeServer::start(Arc::new(move |_| (200, body.clone()))).await?;
    let harness = Harness::with_server(server, approve(None), Arc::new(Mutex::new(String::new())))?;

    // When
    let models = harness.adapter.list_models("access-1").await?;

    // Then the known efforts keep the server's order and the list survives the odd shapes
    assert_eq!(models.len(), 3);
    assert_eq!(models[0].reasoning_efforts, ["low", "medium", "xhigh"]);
    assert_eq!(
        models[0].default_reasoning_effort.as_deref(),
        Some("medium")
    );
    assert_eq!(models[1].reasoning_efforts, Vec::<String>::new());
    assert_eq!(models[2].reasoning_efforts, Vec::<String>::new());
    assert_eq!(models[2].default_reasoning_effort, None);
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
