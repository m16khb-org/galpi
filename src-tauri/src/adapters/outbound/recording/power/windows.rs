//! Windows Power Request: the Windows counterpart of macOS
//! `PreventUserIdleSystemSleep` (display may sleep, the system may not).
//!
//! `SetThreadExecutionState` is thread-bound, which does not fit a blocker
//! created and dropped on different pool threads. A request handle is not, and
//! its reason string shows up in `powercfg /requests`.

use crate::adapters::outbound::platform::to_wide_nul;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Power::{
    PowerClearRequest, PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
};
use windows_sys::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
use windows_sys::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};

/// RAII guard for a system-required power request.
pub struct SleepBlocker {
    request: HANDLE,
}

// SAFETY: a power request handle is a kernel handle with no thread affinity.
unsafe impl Send for SleepBlocker {}

impl SleepBlocker {
    /// Keep the system awake under `name` until the returned guard is dropped.
    ///
    /// Returns `None` when Windows rejects the request; recording proceeds
    /// without sleep protection in that case.
    pub fn acquire(name: &str) -> Option<Self> {
        let mut reason = to_wide_nul(name);
        let context = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: reason.as_mut_ptr(),
            },
        };
        // SAFETY: `context` is a fully initialised REASON_CONTEXT whose
        // string pointer stays valid for the call; Windows copies the string.
        let request = unsafe { PowerCreateRequest(&raw const context) };
        if request.is_null() || request == INVALID_HANDLE_VALUE {
            return None;
        }
        // SAFETY: `request` is a live handle just returned by PowerCreateRequest.
        let set = unsafe { PowerSetRequest(request, PowerRequestSystemRequired) };
        if set == 0 {
            // SAFETY: closing the handle created above, once.
            unsafe { CloseHandle(request) };
            return None;
        }
        Some(Self { request })
    }
}

impl Drop for SleepBlocker {
    fn drop(&mut self) {
        // SAFETY: `request` is the live handle owned by this value; the
        // request is cleared, then the handle is closed exactly once.
        unsafe {
            PowerClearRequest(self.request, PowerRequestSystemRequired);
            CloseHandle(self.request);
        }
    }
}
