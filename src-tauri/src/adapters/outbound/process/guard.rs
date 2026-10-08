//! Whole-process-tree supervision for spawned workers.
//!
//! The face is platform neutral; the mechanism is not. Unix puts the child in
//! its own process group and signals the group. Windows has no group signals,
//! so the child joins a Job Object that kills every member on termination.

use crate::application::error::AppError;
use tokio::process::{Child, Command};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

/// How hard to end a process tree.
///
/// Windows has no polite group-wide request, so there `Graceful` is as final
/// as `Force`; workers publish artifacts with an atomic rename, which makes an
/// immediate stop safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escalation {
    Graceful,
    Force,
}

/// Apply the platform's spawn settings (process group, hidden console).
pub fn configure(command: &mut Command) {
    platform::configure(command);
}

pub struct ProcessGroupGuard {
    inner: platform::Inner,
    armed: bool,
}

impl ProcessGroupGuard {
    /// Bring a freshly spawned child under supervision.
    pub fn attach(child: &Child) -> Result<Self, AppError> {
        Ok(Self {
            inner: platform::Inner::attach(child)?,
            armed: true,
        })
    }

    pub fn terminate(&self, escalation: Escalation) -> Result<(), AppError> {
        self.inner.terminate(escalation)
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }

    #[cfg(all(test, windows))]
    pub fn active_processes(&self) -> Result<u32, AppError> {
        self.inner.active_processes()
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if self.armed
            && let Err(error) = self.terminate(Escalation::Force)
        {
            eprintln!("process group cleanup failed: {error}");
        }
    }
}
