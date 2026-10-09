//! The authorization request and the judgement of its callback.

use super::loopback::Callback;
use super::pkce::{NONCE_BYTES, VERIFIER_BYTES, code_challenge, random_urlsafe};
use crate::application::error::AppError;
use crate::domain::chatgpt::{DYNAMIC_CLIENT_ID, SignInRequest};
use reqwest::Url;

pub const SCOPE: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
pub const RESOURCE: &str = "https://api.openai.com/v1";
const AGENT_NAME: &str = "Galpi";

/// The per-attempt secrets that bind the callback to this attempt.
pub struct Session {
    pub state: String,
    pub nonce: String,
    pub verifier: String,
}

impl Session {
    pub fn new() -> Result<Self, AppError> {
        Ok(Self {
            state: random_urlsafe(NONCE_BYTES)?,
            nonce: random_urlsafe(NONCE_BYTES)?,
            verifier: random_urlsafe(VERIFIER_BYTES)?,
        })
    }
}

/// The URL for the system browser. It carries `state`, so never log it.
pub fn authorization_url(
    endpoint: &str,
    request: &SignInRequest,
    session: &Session,
    redirect_uri: &str,
) -> Result<String, AppError> {
    let mut url = Url::parse(endpoint)
        .map_err(|_| AppError::new("CHATGPT_SIGN_IN_FAILED", "로그인 주소를 만들지 못했습니다."))?;
    {
        let client_id = request
            .registration
            .as_ref()
            .map_or(DYNAMIC_CLIENT_ID, |registration| registration.client_id());
        let mut query = url.query_pairs_mut();
        query
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", SCOPE)
            .append_pair("resource", RESOURCE)
            .append_pair("state", &session.state)
            .append_pair("nonce", &session.nonce)
            .append_pair("code_challenge", &code_challenge(&session.verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("ext_agent_host_id", request.host_id.as_str());
        match &request.registration {
            None => {
                query.append_pair("agent_name_hint", AGENT_NAME);
            }
            Some(registration) => {
                if let Some(email) = registration.email() {
                    query.append_pair("login_hint", email);
                }
                if registration.needs_consent() {
                    query.append_pair("prompt", "consent");
                }
            }
        }
    }
    Ok(url.into())
}

/// A callback that passed every check made before the code exchange.
pub struct Accepted {
    pub code: String,
    pub client_id: String,
}

fn failed(message: &str) -> AppError {
    AppError::new("CHATGPT_SIGN_IN_FAILED", message)
}

/// Judge the callback; any `Err` means the token endpoint must not be called.
pub fn accept_callback(
    callback: &Callback,
    request: &SignInRequest,
    session: &Session,
) -> Result<Accepted, AppError> {
    if callback.get("state") != Some(session.state.as_str()) {
        return Err(AppError::new(
            "CHATGPT_STATE_MISMATCH",
            "로그인 응답이 이 요청의 것이 아니어서 무시했습니다. 다시 시도해 주세요.",
        ));
    }
    match callback.get("error") {
        Some("access_denied") => {
            return Err(AppError::new(
                "CHATGPT_CONSENT_DENIED",
                "ChatGPT 요금제 사용 동의가 거부되었습니다. 사용하려면 다시 로그인해 동의해 주세요.",
            ));
        }
        Some(_) => {
            return Err(failed(
                "ChatGPT가 로그인을 거절했습니다. 다시 시도해 주세요.",
            ));
        }
        None => {}
    }
    let returned = callback.get("client_id").filter(|value| !value.is_empty());
    let client_id = match (&request.registration, returned) {
        (None, Some(issued)) if issued != DYNAMIC_CLIENT_ID => issued.to_owned(),
        (None, _) => {
            return Err(AppError::new(
                "CHATGPT_REGISTRATION_INCOMPLETE",
                "ChatGPT가 앱 등록 정보를 돌려주지 않았습니다. 다시 시도해 주세요.",
            ));
        }
        (Some(stored), Some(issued)) if issued != stored.client_id() => {
            return Err(AppError::new(
                "CHATGPT_CLIENT_MISMATCH",
                "로그인 응답의 앱 정보가 저장된 정보와 달라 중단했습니다. 로그아웃 뒤 다시 로그인해 주세요.",
            ));
        }
        (Some(stored), _) => stored.client_id().to_owned(),
    };
    let code = callback
        .get("code")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| failed("로그인 응답에 인증 코드가 없습니다. 다시 시도해 주세요."))?;
    Ok(Accepted {
        code: code.to_owned(),
        client_id,
    })
}
