//! Linux idle/sleep inhibitor through `systemd-inhibit`.
//!
//! The blocker is a `systemd-inhibit --what=idle:sleep … cat` child process:
//! logind holds the inhibitor lock for as long as that child lives. `cat`
//! reads the child's piped stdin, so the lock also ends when Galpi dies
//! without running `Drop` (the pipe closes and `cat` exits). That keeps this
//! free of `unsafe`, FFI and extra crates, and a process handle has no thread
//! affinity, which suits a blocker created and dropped on different threads.

use std::process::{Child, Command, Stdio};

/// RAII guard for a logind sleep inhibitor.
pub struct SleepBlocker {
    child: Child,
}

impl SleepBlocker {
    /// Keep the system awake under `name` until the returned guard is dropped.
    ///
    /// Returns `None` when `systemd-inhibit` cannot be started; recording
    /// proceeds without sleep protection in that case.
    pub fn acquire(name: &str) -> Option<Self> {
        let child = Command::new("systemd-inhibit")
            .args(["--what=idle:sleep", "--who=Galpi", "--mode=block"])
            .arg(format!("--why={name}"))
            .arg("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        Some(Self { child })
    }
}

impl Drop for SleepBlocker {
    fn drop(&mut self) {
        // Closing stdin would end `cat` on its own; killing makes it prompt.
        drop(self.child.stdin.take());
        let _killed = self.child.kill();
        let _reaped = self.child.wait();
    }
}
