//! The Google consent URL and its loopback callback.

use super::super::browser_sign_in::loopback::Callback;
use super::super::browser_sign_in::pkce::code_challenge;
use super::{CLIENT_ID, SCOPE, connect_failed};
use crate::application::error::AppError;
use crate::application::ports::DriveAuthorize;
use crate::domain::drive::{DriveFileId, parse_picked_ids};
use reqwest::Url;

/// What the desktop Picker offers: recordings people actually make.
const PICKER_MIME_TYPES: &str = "audio/mpeg,audio/mp4,audio/x-m4a,audio/aac,audio/wav,\
audio/x-wav,audio/flac,audio/ogg,video/mp4,video/quicktime";

/// The URL for the system browser. It carries `state`, so never log it.
pub(super) fn consent_url(
    base: &str,
    request: &DriveAuthorize,
    redirect_uri: &str,
    verifier: &str,
    state: &str,
) -> Result<String, AppError> {
    let mut url =
        Url::parse(base).map_err(|_| connect_failed("Drive 연결 주소를 만들지 못했습니다."))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("client_id", CLIENT_ID)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", SCOPE)
            .append_pair("code_challenge", &code_challenge(verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state)
            .append_pair("access_type", "offline")
            // The desktop Picker requires it; with offline access it also makes
            // Google return a refresh token.
            .append_pair("prompt", "consent");
        if request.picker {
            query
                .append_pair("trigger_onepick", "true")
                .append_pair("mimetypes", PICKER_MIME_TYPES);
        }
        if let Some(hint) = &request.login_hint {
            query.append_pair("login_hint", hint);
        }
    }
    Ok(url.into())
}

/// The code and the picked files of a callback that answers this request.
pub(super) fn read_callback(
    callback: &Callback,
    state: &str,
) -> Result<(String, Vec<DriveFileId>), AppError> {
    if callback.get("state") != Some(state) {
        return Err(AppError::new(
            "DRIVE_STATE_MISMATCH",
            "Google 응답이 이 요청과 맞지 않아 Drive 연결을 마치지 못했습니다. 다시 시도해 주세요.",
        ));
    }
    match callback.get("error") {
        Some("access_denied") => {
            return Err(AppError::new("CANCELLED", "사용자가 작업을 취소했습니다."));
        }
        Some(_) => {
            return Err(connect_failed(
                "Google에서 Drive 권한을 받지 못했습니다. 다시 시도해 주세요.",
            ));
        }
        None => {}
    }
    let code = callback
        .get("code")
        .filter(|code| !code.is_empty())
        .ok_or_else(|| connect_failed("Google 응답에 인증 코드가 없습니다. 다시 시도해 주세요."))?;
    let picked =
        parse_picked_ids(callback.get("picked_file_ids").unwrap_or_default()).map_err(|_| {
            AppError::new(
                "DRIVE_PICK_INVALID",
                "Drive에서 고른 파일 정보를 읽지 못했습니다. 다시 골라 주세요.",
            )
        })?;
    Ok((code.to_owned(), picked))
}
