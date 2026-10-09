//! macOS `PreventUserIdleSystemSleep` assertion through `IOKit`.

use std::ffi::c_void;
use std::ptr;

type CFAllocatorRef = *const c_void;
type CFStringRef = *const c_void;
type CFIndex = isize;
type CFStringEncoding = u32;
type IOPMAssertionID = u32;
type IOReturn = i32;

const UTF8_ENCODING: CFStringEncoding = 0x0800_0100;
const ASSERTION_LEVEL_ON: u32 = 255;
const RETURN_SUCCESS: IOReturn = 0;
const ASSERTION_TYPE: &str = "PreventUserIdleSystemSleep";

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(
        alloc: CFAllocatorRef,
        bytes: *const u8,
        num_bytes: CFIndex,
        encoding: CFStringEncoding,
        is_external_representation: u8,
    ) -> CFStringRef;
    fn CFRelease(cf: *const c_void);
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: CFStringRef,
        assertion_level: u32,
        assertion_name: CFStringRef,
        assertion_id: *mut IOPMAssertionID,
    ) -> IOReturn;
    fn IOPMAssertionRelease(assertion_id: IOPMAssertionID) -> IOReturn;
}

/// RAII guard for a `PreventUserIdleSystemSleep` assertion.
pub struct SleepBlocker {
    assertion: IOPMAssertionID,
}

impl SleepBlocker {
    /// Keep the system awake under `name` until the returned guard is dropped.
    ///
    /// Returns `None` when powerd rejects the assertion; recording proceeds
    /// without sleep protection in that case.
    pub fn acquire(name: &str) -> Option<Self> {
        let assertion_type = cf_string(ASSERTION_TYPE)?;
        let Some(assertion_name) = cf_string(name) else {
            // SAFETY: `assertion_type` is a live CFString created above and
            // released exactly once here.
            unsafe { CFRelease(assertion_type) };
            return None;
        };
        let mut assertion: IOPMAssertionID = 0;
        // SAFETY: both CFStringRefs are valid non-null strings owned by this
        // frame, and `assertion` is a valid out-pointer for the id.
        let status = unsafe {
            IOPMAssertionCreateWithName(
                assertion_type,
                ASSERTION_LEVEL_ON,
                assertion_name,
                &raw mut assertion,
            )
        };
        // SAFETY: releasing the two CFStrings this function created, once each.
        unsafe {
            CFRelease(assertion_name);
            CFRelease(assertion_type);
        }
        (status == RETURN_SUCCESS).then_some(Self { assertion })
    }
}

impl Drop for SleepBlocker {
    fn drop(&mut self) {
        // SAFETY: the id came from a successful IOPMAssertionCreateWithName
        // and is released exactly once.
        unsafe { IOPMAssertionRelease(self.assertion) };
    }
}

fn cf_string(value: &str) -> Option<CFStringRef> {
    let length = CFIndex::try_from(value.len()).ok()?;
    // SAFETY: the pointer and length describe `value`'s UTF-8 bytes, which
    // outlive the call; CoreFoundation copies the bytes.
    let string =
        unsafe { CFStringCreateWithBytes(ptr::null(), value.as_ptr(), length, UTF8_ENCODING, 0) };
    (!string.is_null()).then_some(string)
}
