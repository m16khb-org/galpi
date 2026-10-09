//! The signed-in account's model list (`GET /v1/models`).

use crate::application::error::AppError;
use crate::domain::chatgpt::ChatGptModel;
use reqwest::Client;
use serde::Deserialize;

const LISTED: &str = "list";

#[derive(Deserialize)]
struct ModelsBody {
    models: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    slug: String,
    display_name: Option<String>,
    visibility: Option<String>,
}

fn unavailable(reason: &str) -> AppError {
    AppError::new(
        "CHATGPT_MODELS_UNAVAILABLE",
        format!("ChatGPT 모델 목록을 가져오지 못했습니다. {reason}"),
    )
}

/// Models with `visibility == "list"`, in the server's order.
pub async fn list_models(
    http: &Client,
    url: &str,
    access_token: &str,
) -> Result<Vec<ChatGptModel>, AppError> {
    let response = http
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| unavailable("네트워크 연결을 확인해 주세요."))?;
    let status = response.status();
    if !status.is_success() {
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("-");
        eprintln!("chatgpt models request failed: status={status} request_id={request_id}");
        return Err(unavailable("잠시 뒤 다시 시도하거나 다시 로그인해 주세요."));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| unavailable("응답을 끝까지 받지 못했습니다."))?;
    let body: ModelsBody =
        serde_json::from_slice(&bytes).map_err(|_| unavailable("응답 형식을 알 수 없습니다."))?;
    Ok(body
        .models
        .into_iter()
        .filter(|entry| entry.visibility.as_deref() == Some(LISTED))
        .map(|entry| ChatGptModel {
            display_name: entry.display_name.unwrap_or_else(|| entry.slug.clone()),
            slug: entry.slug,
        })
        .collect())
}
