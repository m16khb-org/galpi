//! Recording-scoped power request.
//!
//! Holding a [`SleepBlocker`] keeps the system awake while the display may
//! sleep, so an active CPAL capture stream is not suspended mid-recording.
//! Dropping the blocker releases the request immediately.
//!
//! Both implementations are handle/id based rather than thread based, because
//! the blocker is created on one `spawn_blocking` thread and dropped on another.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::SleepBlocker;
#[cfg(windows)]
pub use windows::SleepBlocker;
