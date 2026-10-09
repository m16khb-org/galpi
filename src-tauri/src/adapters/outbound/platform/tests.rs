use super::{
    Os, cuda_driver_present, home_directory, to_wide_nul, whisperx_lock_hash_name,
    worker_environment,
};
use crate::adapters::outbound::paths::AppPaths;
use crate::domain::engine::{ComputeDevice, EnginePreset};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

fn paths(os: Os) -> AppPaths {
    AppPaths::from_roots(os, PathBuf::from("/data"), Path::new("/docs"))
}

fn lookup(key: &str) -> Option<OsString> {
    let value = match key {
        "HOME" => "/Users/tester",
        "TMPDIR" => "/var/folders/tmp",
        "USERPROFILE" => r"C:\Users\tester",
        "SYSTEMROOT" | "WINDIR" => r"C:\Windows",
        "TEMP" | "TMP" => r"C:\Users\tester\AppData\Local\Temp",
        "LOCALAPPDATA" => r"C:\Users\tester\AppData\Local",
        "APPDATA" => r"C:\Users\tester\AppData\Roaming",
        "COMSPEC" => r"C:\Windows\System32\cmd.exe",
        "PATHEXT" => ".COM;.EXE",
        "HOMEDRIVE" => "C:",
        "HOMEPATH" => r"\Users\tester",
        _ => return None,
    };
    Some(value.into())
}

fn text(env: &std::collections::HashMap<OsString, OsString>, key: &str) -> Option<String> {
    env.get(OsStr::new(key))
        .map(|value| value.to_string_lossy().into_owned())
}

#[test]
fn file_names_follow_the_operating_system() {
    assert_eq!(Os::MacOs.python_relative(), [".venv", "bin", "python"]);
    assert_eq!(
        Os::Windows.python_relative(),
        [".venv", "Scripts", "python.exe"]
    );
    assert_eq!(Os::MacOs.ffmpeg_file_name(), "ffmpeg");
    assert_eq!(Os::Windows.ffmpeg_file_name(), "ffmpeg.exe");
    assert_eq!(Os::MacOs.uv_staged_name(), "uv-aarch64-apple-darwin");
    assert_eq!(
        Os::Windows.uv_staged_name(),
        "uv-x86_64-pc-windows-msvc.exe"
    );
    assert_eq!(Os::MacOs.uv_installed_name(), "uv");
    assert_eq!(Os::Windows.uv_installed_name(), "uv.exe");
}

#[test]
fn staged_uv_names_are_spelled_out_for_the_sidecar_script_drift_check() {
    // scripts/sidecar-targets.test.ts greps this file's source for both names.
    let source = include_str!("../platform.rs");
    assert!(source.contains("uv-aarch64-apple-darwin"));
    assert!(source.contains("uv-x86_64-pc-windows-msvc.exe"));
}

#[test]
fn the_first_preset_is_the_platform_default() {
    assert_eq!(
        Os::MacOs.presets(),
        [EnginePreset::Qwen3, EnginePreset::WhisperX]
    );
    assert_eq!(Os::Windows.presets(), [EnginePreset::WhisperX]);
    assert_eq!(Os::MacOs.devices(), []);
    assert_eq!(
        Os::Windows.devices(),
        [ComputeDevice::Cpu, ComputeDevice::Cuda]
    );
}

#[test]
fn lock_selection_depends_on_platform_and_device() {
    let mac = Os::MacOs.whisperx_lock(ComputeDevice::Cuda);
    assert_eq!(mac.file_name, "requirements.lock");
    assert_eq!(mac.extra_args, [] as [&str; 0]);

    let cpu = Os::Windows.whisperx_lock(ComputeDevice::Cpu);
    assert_eq!(cpu.file_name, "requirements-windows-cpu.lock");
    assert_eq!(cpu.extra_args, [] as [&str; 0]);

    let cuda = Os::Windows.whisperx_lock(ComputeDevice::Cuda);
    assert_eq!(cuda.file_name, "requirements-windows-cuda.lock");
    assert_eq!(cuda.extra_args, ["--index-strategy", "unsafe-best-match"]);
}

#[test]
fn only_windows_has_a_lock_fingerprint_name_per_device() {
    assert_eq!(
        whisperx_lock_hash_name(Os::MacOs, ComputeDevice::Cpu),
        "mac"
    );
    assert_eq!(
        whisperx_lock_hash_name(Os::Windows, ComputeDevice::Cpu),
        "win-cpu"
    );
    assert_eq!(
        whisperx_lock_hash_name(Os::Windows, ComputeDevice::Cuda),
        "win-cuda"
    );
}

#[test]
fn the_neutral_working_directory_is_root_on_macos_and_app_data_on_windows() {
    assert_eq!(
        Os::MacOs.neutral_working_directory(&paths(Os::MacOs)),
        PathBuf::from("/")
    );
    assert_eq!(
        Os::Windows.neutral_working_directory(&paths(Os::Windows)),
        PathBuf::from("/data")
    );
}

// The expected values are POSIX strings; a Windows host would join the same
// paths with `\`, so the characterisation only runs where it describes reality.
#[cfg(unix)]
#[test]
fn macos_worker_environment_is_the_historical_one() {
    // Given: the keys and values environment.rs produced before platform.rs
    let paths = paths(Os::MacOs);
    let env = worker_environment(Os::MacOs, &lookup, &paths, Path::new("/worker"));

    // Then
    let expected: [(&str, &str); 18] = [
        ("HOME", "/Users/tester"),
        ("LANG", "ko_KR.UTF-8"),
        ("LC_ALL", "ko_KR.UTF-8"),
        ("PYTHONUTF8", "1"),
        ("PYTHONSAFEPATH", "1"),
        ("PYTHONDONTWRITEBYTECODE", "1"),
        ("PYTHONPATH", "/worker"),
        ("HF_HOME", "/data/cache/huggingface"),
        ("TORCH_HOME", "/data/cache/torch"),
        ("HF_HUB_DISABLE_IMPLICIT_TOKEN", "1"),
        ("HF_HUB_DISABLE_TELEMETRY", "1"),
        ("PYANNOTE_METRICS_ENABLED", "false"),
        ("DO_NOT_TRACK", "1"),
        ("UV_PYTHON_INSTALL_DIR", "/data/python"),
        ("UV_CACHE_DIR", "/data/cache/uv"),
        ("UV_PYTHON_PREFERENCE", "only-managed"),
        (
            "PATH",
            "/data/engine/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        ),
        ("TMPDIR", "/var/folders/tmp"),
    ];
    assert_eq!(env.len(), expected.len());
    for (key, value) in expected {
        assert_eq!(text(&env, key).as_deref(), Some(value), "{key}");
    }
}

#[test]
fn macos_tmpdir_falls_back_to_tmp() {
    let env = worker_environment(Os::MacOs, &|_| None, &paths(Os::MacOs), Path::new("/w"));
    assert_eq!(text(&env, "TMPDIR").as_deref(), Some("/tmp"));
}

#[test]
fn windows_worker_environment_copies_the_host_keys_the_interpreter_needs() {
    // Given / When
    let paths = paths(Os::Windows);
    let env = worker_environment(Os::Windows, &lookup, &paths, Path::new("/worker"));

    // Then: the Win32 essentials are copied, the POSIX ones are absent
    for key in [
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
    ] {
        assert!(env.contains_key(OsStr::new(key)), "{key} missing");
    }
    for key in ["HOME", "LANG", "LC_ALL", "TMPDIR"] {
        assert!(!env.contains_key(OsStr::new(key)), "{key} must be absent");
    }
    assert_eq!(text(&env, "SYSTEMROOT").as_deref(), Some(r"C:\Windows"));
    assert_eq!(text(&env, "PYTHONUTF8").as_deref(), Some("1"));
    assert_eq!(
        text(&env, "HF_HUB_DISABLE_SYMLINKS_WARNING").as_deref(),
        Some("1")
    );
    let path = text(&env, "PATH").unwrap_or_default();
    let expected = format!(
        r"{};C:\Windows\System32;C:\Windows",
        paths.engine_bin.to_string_lossy()
    );
    assert_eq!(path, expected);
}

#[test]
fn windows_worker_environment_skips_host_keys_that_are_not_set() {
    let env = worker_environment(
        Os::Windows,
        &|key| (key == "SYSTEMROOT").then(|| OsString::from(r"D:\Win")),
        &paths(Os::Windows),
        Path::new("/w"),
    );
    assert!(!env.contains_key(OsStr::new("TEMP")));
    assert!(
        text(&env, "PATH")
            .unwrap_or_default()
            .ends_with(r"D:\Win\System32;D:\Win")
    );
}

#[test]
fn the_home_directory_comes_from_the_platform_variable() {
    assert_eq!(
        home_directory(Os::MacOs, &lookup),
        PathBuf::from("/Users/tester")
    );
    assert_eq!(
        home_directory(Os::Windows, &lookup),
        PathBuf::from(r"C:\Users\tester")
    );
    assert_eq!(home_directory(Os::MacOs, &|_| None), std::env::temp_dir());
}

#[test]
fn wide_strings_are_nul_terminated_utf16() {
    assert_eq!(to_wide_nul("가a"), [0xAC00, 0x61, 0]);
    assert_eq!(to_wide_nul(""), [0]);
}

#[test]
fn cuda_is_detected_by_the_driver_library_under_system32() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a fake SystemRoot with and without nvcuda.dll
    let root = std::env::temp_dir().join(format!("galpi-cuda-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(root.join("System32"))?;

    // When / Then
    assert!(!cuda_driver_present(Some(root.as_os_str())));
    std::fs::write(root.join("System32/nvcuda.dll"), b"")?;
    assert!(cuda_driver_present(Some(root.as_os_str())));
    assert!(!cuda_driver_present(None));
    std::fs::remove_dir_all(root)?;
    Ok(())
}
