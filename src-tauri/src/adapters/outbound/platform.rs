//! Operating-system rules as values.
//!
//! Every place that used to hard-code a macOS fact (interpreter path, binary
//! names, `PATH` layout, which engines exist) asks an [`Os`] instead. Branching
//! on a value rather than on `cfg` keeps both branches type-checked on every
//! host, so the Windows rules are unit-tested on macOS. Only code that cannot
//! compile off its own platform (Win32 FFI, `nix`) is behind `#[cfg]`.

use super::paths::AppPaths;
use crate::domain::engine::{ComputeDevice, EnginePreset};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
}

/// Which lock file installs the `WhisperX` environment, and any extra `uv`
/// arguments that lock needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhisperxLock {
    pub file_name: &'static str,
    pub extra_args: &'static [&'static str],
}

const MAC_PRESETS: [EnginePreset; 2] = [EnginePreset::Qwen3, EnginePreset::WhisperX];
const WINDOWS_PRESETS: [EnginePreset; 1] = [EnginePreset::WhisperX];
const WINDOWS_DEVICES: [ComputeDevice; 2] = [ComputeDevice::Cpu, ComputeDevice::Cuda];

/// Host variables copied into the Windows worker environment. `env_clear`
/// starts from nothing, and Python cannot start without `SYSTEMROOT`.
const WINDOWS_HOST_KEYS: [&str; 11] = [
    "USERPROFILE",
    "SYSTEMROOT",
    "WINDIR",
    "TEMP",
    "TMP",
    "LOCALAPPDATA",
    "APPDATA",
    "COMSPEC",
    "PATHEXT",
    "HOMEDRIVE",
    "HOMEPATH",
];

impl Os {
    /// The only `cfg!(windows)` branch in the crate.
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::MacOs
        }
    }

    pub const fn python_relative(self) -> &'static [&'static str] {
        match self {
            Self::MacOs => &[".venv", "bin", "python"],
            Self::Windows => &[".venv", "Scripts", "python.exe"],
        }
    }

    /// Keep in step with `ffmpeg_link_name` in `worker/galpi_worker/preparation.py`.
    pub const fn ffmpeg_file_name(self) -> &'static str {
        match self {
            Self::MacOs => "ffmpeg",
            Self::Windows => "ffmpeg.exe",
        }
    }

    /// Name of the sidecar as staged under `src-tauri/binaries`.
    pub const fn uv_staged_name(self) -> &'static str {
        match self {
            Self::MacOs => "uv-aarch64-apple-darwin",
            Self::Windows => "uv-x86_64-pc-windows-msvc.exe",
        }
    }

    /// Name of the sidecar next to the executable once bundled.
    pub const fn uv_installed_name(self) -> &'static str {
        match self {
            Self::MacOs => "uv",
            Self::Windows => "uv.exe",
        }
    }

    /// Selectable presets; the first is the platform default.
    pub const fn presets(self) -> &'static [EnginePreset] {
        match self {
            Self::MacOs => &MAC_PRESETS,
            Self::Windows => &WINDOWS_PRESETS,
        }
    }

    /// Selectable compute devices; empty means there is no choice.
    pub const fn devices(self) -> &'static [ComputeDevice] {
        match self {
            Self::MacOs => &[],
            Self::Windows => &WINDOWS_DEVICES,
        }
    }

    pub const fn whisperx_lock(self, device: ComputeDevice) -> WhisperxLock {
        match (self, device) {
            (Self::MacOs, _) => WhisperxLock {
                file_name: "requirements.lock",
                extra_args: &[],
            },
            (Self::Windows, ComputeDevice::Cpu) => WhisperxLock {
                file_name: "requirements-windows-cpu.lock",
                extra_args: &[],
            },
            // The CUDA lock pins `+cu128` wheels that only the PyTorch index
            // carries, while everything else comes from PyPI. Every entry is
            // hash-pinned, so best-match adds no dependency-confusion risk.
            (Self::Windows, ComputeDevice::Cuda) => WhisperxLock {
                file_name: "requirements-windows-cuda.lock",
                extra_args: &["--index-strategy", "unsafe-best-match"],
            },
        }
    }

    /// Working directory for tools that must not pick up a project file.
    ///
    /// macOS keeps `/` so uv's `pyproject.toml` search behaves as before;
    /// on Windows `/` means "current drive root", so use the app data root.
    pub fn neutral_working_directory(self, paths: &AppPaths) -> PathBuf {
        match self {
            Self::MacOs => PathBuf::from("/"),
            Self::Windows => paths.root.clone(),
        }
    }
}

/// Label of the lock whose fingerprint belongs in the readiness marker.
pub const fn whisperx_lock_hash_name(os: Os, device: ComputeDevice) -> &'static str {
    match (os, device) {
        (Os::MacOs, _) => "mac",
        (Os::Windows, ComputeDevice::Cpu) => "win-cpu",
        (Os::Windows, ComputeDevice::Cuda) => "win-cuda",
    }
}

pub fn home_directory(os: Os, lookup: &dyn Fn(&str) -> Option<OsString>) -> PathBuf {
    let key = match os {
        Os::MacOs => "HOME",
        Os::Windows => "USERPROFILE",
    };
    lookup(key).map_or_else(std::env::temp_dir, PathBuf::from)
}

/// The one place the worker's environment is built.
pub fn worker_environment(
    os: Os,
    lookup: &dyn Fn(&str) -> Option<OsString>,
    paths: &AppPaths,
    worker_root: &Path,
) -> HashMap<OsString, OsString> {
    let mut env: HashMap<OsString, OsString> = HashMap::from([
        ("PYTHONUTF8".into(), "1".into()),
        ("PYTHONSAFEPATH".into(), "1".into()),
        ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
        ("PYTHONPATH".into(), worker_root.as_os_str().to_owned()),
        (
            "HF_HOME".into(),
            paths.cache.join("huggingface").into_os_string(),
        ),
        (
            "TORCH_HOME".into(),
            paths.cache.join("torch").into_os_string(),
        ),
        ("HF_HUB_DISABLE_IMPLICIT_TOKEN".into(), "1".into()),
        ("HF_HUB_DISABLE_TELEMETRY".into(), "1".into()),
        ("PYANNOTE_METRICS_ENABLED".into(), "false".into()),
        ("DO_NOT_TRACK".into(), "1".into()),
        (
            "UV_PYTHON_INSTALL_DIR".into(),
            paths.python_installations.clone().into_os_string(),
        ),
        // Several gigabytes of wheels belong inside the app's own data, so
        // that removing Galpi's folder actually reclaims them.
        (
            "UV_CACHE_DIR".into(),
            paths.cache.join("uv").into_os_string(),
        ),
        // Only the interpreter uv installed itself is a known quantity; a
        // system 3.12 that happens to be on PATH is not.
        ("UV_PYTHON_PREFERENCE".into(), "only-managed".into()),
    ]);
    match os {
        Os::MacOs => {
            env.insert("HOME".into(), home_directory(os, lookup).into_os_string());
            env.insert("LANG".into(), "ko_KR.UTF-8".into());
            env.insert("LC_ALL".into(), "ko_KR.UTF-8".into());
            env.insert(
                "PATH".into(),
                format!(
                    "{}:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
                    paths.engine_bin.to_string_lossy()
                )
                .into(),
            );
            env.insert(
                "TMPDIR".into(),
                lookup("TMPDIR").unwrap_or_else(|| "/tmp".into()),
            );
        }
        Os::Windows => {
            for key in WINDOWS_HOST_KEYS {
                if let Some(value) = lookup(key) {
                    env.insert(key.into(), value);
                }
            }
            let system_root = lookup("SYSTEMROOT").map_or_else(
                || r"C:\Windows".to_owned(),
                |root| root.to_string_lossy().into_owned(),
            );
            env.insert(
                "PATH".into(),
                format!(
                    r"{};{system_root}\System32;{system_root}",
                    paths.engine_bin.to_string_lossy()
                )
                .into(),
            );
            env.insert("HF_HUB_DISABLE_SYMLINKS_WARNING".into(), "1".into());
        }
    }
    env
}

/// UTF-16 with a trailing NUL, as Win32 `W` functions expect.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows FFI modules call this")
)]
pub fn to_wide_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Whether an NVIDIA driver is installed: `<SystemRoot>\System32\nvcuda.dll`.
pub fn cuda_driver_present(system_root: Option<&OsStr>) -> bool {
    system_root.is_some_and(|root| {
        Path::new(root)
            .join("System32")
            .join("nvcuda.dll")
            .is_file()
    })
}

#[cfg(test)]
mod tests;
