use super::Escalation;
use crate::application::error::AppError;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use tokio::process::{Child, Command};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
#[cfg(test)]
use windows_sys::Win32::System::JobObjects::{
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
    QueryInformationJobObject,
};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

/// A console child of a GUI app would otherwise flash a black window.
pub fn configure(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

/// Owns a Job Object that kills every member when the handle closes.
pub struct Inner {
    job: HANDLE,
}

// SAFETY: a Job Object handle is a process-wide kernel handle with no thread
// affinity; the Win32 job APIs may be called from any thread.
unsafe impl Send for Inner {}
// SAFETY: as above; every operation takes the handle by value and the kernel
// serialises access.
unsafe impl Sync for Inner {}

fn failure(action: &str) -> AppError {
    AppError::new(
        "PROCESS_ERROR",
        format!("{action}: {}", std::io::Error::last_os_error()),
    )
}

impl Inner {
    pub fn attach(child: &Child) -> Result<Self, AppError> {
        let process = child.raw_handle().ok_or_else(|| {
            AppError::new("PROCESS_ERROR", "프로세스 핸들을 확인하지 못했습니다.")
        })?;
        // SAFETY: null attributes and name are documented as valid and create
        // an unnamed job with default security.
        let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if job.is_null() {
            return Err(failure("작업 개체를 만들지 못했습니다"));
        }
        // From here the guard owns the handle, so every early return closes it.
        let inner = Self { job };
        // SAFETY: all-zero is a valid value for this plain C struct.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size =
            u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).map_err(|_| {
                AppError::new(
                    "PROCESS_ERROR",
                    "작업 개체 구조체 크기가 올바르지 않습니다.",
                )
            })?;
        // SAFETY: `limits` is a live, correctly sized
        // JOBOBJECT_EXTENDED_LIMIT_INFORMATION for the stated class.
        let configured = unsafe {
            SetInformationJobObject(
                inner.job,
                JobObjectExtendedLimitInformation,
                ptr::from_ref(&limits).cast::<c_void>(),
                size,
            )
        };
        if configured == 0 {
            return Err(failure("작업 개체를 설정하지 못했습니다"));
        }
        // SAFETY: `inner.job` is the live job above and `process` is the
        // child's handle, which tokio keeps open while the `Child` exists.
        let assigned = unsafe { AssignProcessToJobObject(inner.job, process) };
        if assigned == 0 {
            return Err(failure("프로세스를 작업 개체에 넣지 못했습니다"));
        }
        Ok(inner)
    }

    pub fn terminate(&self, _escalation: Escalation) -> Result<(), AppError> {
        // SAFETY: `self.job` is a live job handle owned by this value.
        let terminated = unsafe { TerminateJobObject(self.job, 1) };
        if terminated == 0 {
            return Err(failure("작업 개체를 종료하지 못했습니다"));
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn active_processes(&self) -> Result<u32, AppError> {
        // SAFETY: all-zero is a valid value for this plain C struct.
        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        let size =
            u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>()).map_err(|_| {
                AppError::new(
                    "PROCESS_ERROR",
                    "작업 개체 구조체 크기가 올바르지 않습니다.",
                )
            })?;
        // SAFETY: `accounting` is a live, correctly sized out-buffer for the
        // stated class, and the returned-length pointer may be null.
        let queried = unsafe {
            QueryInformationJobObject(
                self.job,
                JobObjectBasicAccountingInformation,
                ptr::from_mut(&mut accounting).cast::<c_void>(),
                size,
                ptr::null_mut(),
            )
        };
        if queried == 0 {
            return Err(failure("작업 개체 상태를 읽지 못했습니다"));
        }
        Ok(accounting.ActiveProcesses)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by this value and closed exactly once;
        // KILL_ON_JOB_CLOSE ends any process still in the job.
        unsafe { CloseHandle(self.job) };
    }
}
