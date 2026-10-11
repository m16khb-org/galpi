//! Google Drive for the desktop (RFC 8252): `drive.file` consent and the
//! desktop Picker through the system browser and a loopback redirect with
//! PKCE, the code exchange and renewal the auth-gateway performs for Galpi
//! (the desktop client secret lives only on the gateway), and Drive v3 calls.
//!
//! Nothing here logs a token, `code`, `state`, or consent URL, and no failed
//! request is retried.

mod authorize;
mod rest;
mod tokens;

#[cfg(test)]
mod tests;

pub use rest::DriveRestAdapter;

use super::browser_sign_in::loopback::{Loopback, PREFERRED_PORT, SIGN_IN_TIMEOUT};
use super::browser_sign_in::pkce::{NONCE_BYTES, VERIFIER_BYTES, random_urlsafe};
use super::browser_sign_in::{BrowserOpener, SignInCodes};
use super::gateway::gateway_url;
use crate::application::error::AppError;
use crate::application::ports::{ClockPort, DriveAuthPort, DriveAuthorize, DriveCallback};
use crate::domain::drive::{DriveTokenFailure, DriveTokens};
use async_trait::async_trait;
use reqwest::{Client, redirect::Policy};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

/// The `Galpi Desktop` OAuth client of the `galpi-511214` Cloud project.
/// A desktop client id is public; its secret stays on the auth-gateway.
const CLIENT_ID: &str = "747212408320-1io6clrt6t568ot424pv54lepsvfl9pf.apps.googleusercontent.com";
/// The only scope Galpi asks for: files it created or the user picked.
const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const DRIVE_API_URL: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3";

const CODES: SignInCodes = SignInCodes {
    failed: "DRIVE_CONNECT_FAILED",
    timed_out: "DRIVE_CONNECT_TIMEOUT",
};

fn connect_failed(message: &str) -> AppError {
    AppError::new(CODES.failed, message)
}

/// Every endpoint and limit that production fixes and tests shorten.
pub struct Tuning {
    pub gateway_url: String,
    pub google_auth_url: String,
    pub google_revoke_url: String,
    pub drive_api_url: String,
    pub drive_upload_url: String,
    pub preferred_port: u16,
    pub sign_in_timeout: Duration,
}

impl Tuning {
    fn production() -> Self {
        Self {
            gateway_url: gateway_url(),
            google_auth_url: GOOGLE_AUTH_URL.to_owned(),
            google_revoke_url: GOOGLE_REVOKE_URL.to_owned(),
            drive_api_url: DRIVE_API_URL.to_owned(),
            drive_upload_url: DRIVE_UPLOAD_URL.to_owned(),
            preferred_port: PREFERRED_PORT,
            sign_in_timeout: SIGN_IN_TIMEOUT,
        }
    }
}

pub struct DriveOAuthAdapter {
    http: Client,
    tuning: Tuning,
    browser: Arc<dyn BrowserOpener>,
    clock: Arc<dyn ClockPort>,
}

impl DriveOAuthAdapter {
    pub fn new(
        browser: Arc<dyn BrowserOpener>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, AppError> {
        Self::with_tuning(Tuning::production(), browser, clock)
    }

    fn with_tuning(
        tuning: Tuning,
        browser: Arc<dyn BrowserOpener>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, AppError> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()
            .map_err(|_| connect_failed("Drive 연결을 준비하지 못했습니다."))?;
        Ok(Self {
            http,
            tuning,
            browser,
            clock,
        })
    }

    fn token_endpoint(&self) -> String {
        format!("{}/drive/native/token", self.tuning.gateway_url)
    }
}

#[async_trait]
impl DriveAuthPort for DriveOAuthAdapter {
    async fn authorize(
        &self,
        request: &DriveAuthorize,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<DriveCallback, AppError> {
        let state = random_urlsafe(NONCE_BYTES, CODES)?;
        let verifier = random_urlsafe(VERIFIER_BYTES, CODES)?;
        let loopback = Loopback::bind_preferring(self.tuning.preferred_port).await?;
        let redirect_uri = loopback.redirect_uri();
        let url = authorize::consent_url(
            &self.tuning.google_auth_url,
            request,
            &redirect_uri,
            &verifier,
            &state,
        )?;
        self.browser.open(&url, CODES)?;
        let callback = loopback
            .wait(cancel, self.tuning.sign_in_timeout, CODES)
            .await?;
        let (code, picked) = authorize::read_callback(&callback, &state)?;
        Ok(DriveCallback {
            code,
            verifier,
            redirect_uri,
            picked,
        })
    }

    async fn exchange(
        &self,
        bearer: &str,
        callback: &DriveCallback,
    ) -> Result<DriveTokens, DriveTokenFailure> {
        tokens::exchange(
            &self.http,
            &self.token_endpoint(),
            bearer,
            callback,
            self.clock.now_unix(),
        )
        .await
    }

    async fn refresh(
        &self,
        bearer: &str,
        refresh_token: &str,
    ) -> Result<DriveTokens, DriveTokenFailure> {
        tokens::refresh(
            &self.http,
            &self.token_endpoint(),
            bearer,
            refresh_token,
            self.clock.now_unix(),
        )
        .await
    }

    async fn revoke(&self, refresh_token: &str) {
        // Best effort: the local connection is forgotten whatever Google says.
        let _answered = self
            .http
            .post(&self.tuning.google_revoke_url)
            .form(&[("token", refresh_token)])
            .send()
            .await;
    }
}
