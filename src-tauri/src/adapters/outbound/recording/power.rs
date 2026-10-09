//! Recording-scoped power request.
//!
//! Holding a [`SleepBlocker`] keeps the system awake while the display may
//! sleep, so an active CPAL capture stream is not suspended mid-recording.
//! Dropping the blocker releases the request immediately.
//!
//! Every implementation is handle/id/process based rather than thread based, because
//! the blocker is created on one `spawn_blocking` thread and dropped on another.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::SleepBlocker;
#[cfg(target_os = "macos")]
pub use macos::SleepBlocker;
#[cfg(windows)]
pub use windows::SleepBlocker;
