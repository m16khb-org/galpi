//! What every system-browser sign-in shares (RFC 8252): opening the browser,
//! the loopback redirect listener, and PKCE values.
//!
//! Each sign-in adapter passes its own [`SignInCodes`], so a failure here
//! carries that adapter's stable error codes.

pub mod browser;
#[cfg(test)]
pub mod fake_server;
pub mod loopback;
pub mod pkce;

pub use browser::{BrowserOpener, TauriBrowser};

/// The error codes a sign-in reports for failures in this module.
#[derive(Clone, Copy)]
pub struct SignInCodes {
    /// The browser could not be opened or a random value could not be made.
    pub failed: &'static str,
    /// The browser did not come back before the deadline.
    pub timed_out: &'static str,
}
