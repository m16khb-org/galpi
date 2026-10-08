use super::Escalation;
use crate::application::error::AppError;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::os::unix::process::CommandExt;
use tokio::process::{Child, Command};

pub fn configure(command: &mut Command) {
    command.as_std_mut().process_group(0);
}

pub struct Inner {
    process_id: u32,
}

impl Inner {
    pub fn attach(child: &Child) -> Result<Self, AppError> {
        let process_id = child
            .id()
            .ok_or_else(|| AppError::new("PROCESS_ERROR", "프로세스 ID를 확인하지 못했습니다."))?;
        Ok(Self { process_id })
    }

    pub fn terminate(&self, escalation: Escalation) -> Result<(), AppError> {
        let signal = match escalation {
            Escalation::Graceful => Signal::SIGTERM,
            Escalation::Force => Signal::SIGKILL,
        };
        let raw_id = i32::try_from(self.process_id)
            .map_err(|_| AppError::new("PROCESS_ERROR", "프로세스 ID 범위를 벗어났습니다."))?;
        kill(Pid::from_raw(-raw_id), signal)
            .map_err(|error| AppError::new("PROCESS_ERROR", error.to_string()))
    }
}
