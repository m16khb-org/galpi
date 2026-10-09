//! Opening the sign-in URL in the system browser.

use super::SignInCodes;
use crate::application::error::AppError;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Opens a URL in the user's default browser.
pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str, codes: SignInCodes) -> Result<(), AppError>;
}

/// The production opener, backed by the Tauri opener plugin.
pub struct TauriBrowser {
    app: AppHandle,
}

impl TauriBrowser {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl BrowserOpener for TauriBrowser {
    fn open(&self, url: &str, codes: SignInCodes) -> Result<(), AppError> {
        // The URL carries `state`, so the opener's error text is not forwarded.
        self.app.opener().open_url(url, None::<&str>).map_err(|_| {
            AppError::new(
                codes.failed,
                "시스템 브라우저를 열지 못했습니다. 기본 브라우저 설정을 확인해 주세요.",
            )
        })
    }
}
