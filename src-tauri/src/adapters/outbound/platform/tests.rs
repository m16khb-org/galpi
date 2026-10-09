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
    assert_eq!(Os::Mac.python_relative(), [".venv", "bin", "python"]);
    assert_eq!(
        Os::Windows.python_relative(),
        [".venv", "Scripts", "python.exe"]
    );
    assert_eq!(Os::Mac.ffmpeg_file_name(), "ffmpeg");
    assert_eq!(Os::Windows.ffmpeg_file_name(), "ffmpeg.exe");
    assert_eq!(Os::Mac.uv_staged_name(), "uv-aarch64-apple-darwin");
    assert_eq!(
        Os::Windows.uv_staged_name(),
        "uv-x86_64-pc-windows-msvc.exe"
    );
    assert_eq!(Os::Mac.uv_installed_name(), "uv");
    assert_eq!(Os::Windows.uv_installed_name(), "uv.exe");
    assert_eq!(Os::Linux.python_relative(), [".venv", "bin", "python"]);
    assert_eq!(Os::Linux.ffmpeg_file_name(), "ffmpeg");
    assert_eq!(Os::Linux.uv_staged_name(), "uv-x86_64-unknown-linux-gnu");
    assert_eq!(Os::Linux.uv_installed_name(), "uv");
    assert_eq!(Os::Linux.python_request(), "3.12");
    assert_eq!(Os::Windows.python_request(), "3.12.14");
}

#[test]
fn staged_uv_names_are_spelled_out_for_the_sidecar_script_drift_check() {
    // scripts/sidecar-targets.test.ts greps this file's source for both names.
    let source = include_str!("../platform.rs");
    assert!(source.contains("uv-aarch64-apple-darwin"));
    assert!(source.contains("uv-x86_64-pc-windows-msvc.exe"));
    assert!(source.contains("uv-x86_64-unknown-linux-gnu"));
}

#[test]
fn the_first_preset_is_the_platform_default() {
    assert_eq!(
        Os::Mac.presets(),
        [EnginePreset::WhisperX, EnginePreset::Qwen3]
    );
    assert_eq!(Os::Windows.presets(), [EnginePreset::WhisperX]);
    assert_eq!(Os::Linux.presets(), [EnginePreset::WhisperX]);
    assert_eq!(
        Os::Linux.devices(),
        [ComputeDevice::Cpu, ComputeDevice::Cuda]
    );
    assert_eq!(Os::Mac.devices(), []);
    assert_eq!(
        Os::Windows.devices(),
        [ComputeDevice::Cpu, ComputeDevice::Cuda]
    );
}

#[test]
fn cuda_is_the_default_device_only_where_offered_and_a_driver_is_installed() {
    assert_eq!(Os::Windows.default_device(true), ComputeDevice::Cuda);
    assert_eq!(Os::Windows.default_device(false), ComputeDevice::Cpu);
    assert_eq!(Os::Linux.default_device(true), ComputeDevice::Cuda);
    assert_eq!(Os::Linux.default_device(false), ComputeDevice::Cpu);
    assert_eq!(Os::Mac.default_device(true), ComputeDevice::Cpu);
}

#[test]
fn lock_selection_depends_on_platform_and_device() {
    let mac = Os::Mac.whisperx_lock(ComputeDevice::Cuda);
    assert_eq!(mac.file_name, "requirements.lock");
    assert_eq!(mac.extra_args, [] as [&str; 0]);

    let cpu = Os::Windows.whisperx_lock(ComputeDevice::Cpu);
    assert_eq!(cpu.file_name, "requirements-windows-cpu.lock");
    assert_eq!(cpu.extra_args, [] as [&str; 0]);

    let cuda = Os::Windows.whisperx_lock(ComputeDevice::Cuda);
    assert_eq!(cuda.file_name, "requirements-windows-cuda.lock");
    assert_eq!(cuda.extra_args, ["--index-strategy", "unsafe-best-match"]);

    // The CPU lock's `+cpu` wheels live on the PyTorch CPU index; the CUDA
    // lock is plain PyPI and needs no extra arguments.
    let cpu = Os::Linux.whisperx_lock(ComputeDevice::Cpu);
    assert_eq!(cpu.file_name, "requirements-linux-cpu.lock");
    assert_eq!(cpu.extra_args, ["--index-strategy", "unsafe-best-match"]);

    let cuda = Os::Linux.whisperx_lock(ComputeDevice::Cuda);
    assert_eq!(cuda.file_name, "requirements-linux-cuda.lock");
    assert_eq!(cuda.extra_args, [] as [&str; 0]);
}

#[test]
fn only_windows_has_a_lock_fingerprint_name_per_device() {
    assert_eq!(whisperx_lock_hash_name(Os::Mac, ComputeDevice::Cpu), "mac");
    assert_eq!(
        whisperx_lock_hash_name(Os::Windows, ComputeDevice::Cpu),
        "win-cpu"
    );
    assert_eq!(
        whisperx_lock_hash_name(Os::Windows, ComputeDevice::Cuda),
        "win-cuda"
    );
    assert_eq!(
        whisperx_lock_hash_name(Os::Linux, ComputeDevice::Cpu),
        "linux-cpu"
    );
    assert_eq!(
        whisperx_lock_hash_name(Os::Linux, ComputeDevice::Cuda),
        "linux-cuda"
    );
}

#[test]
fn the_neutral_working_directory_is_root_on_macos_and_app_data_on_windows() {
    assert_eq!(
        Os::Mac.neutral_working_directory(&paths(Os::Mac)),
        PathBuf::from("/")
    );
    assert_eq!(
        Os::Windows.neutral_working_directory(&paths(Os::Windows)),
        PathBuf::from("/data")
    );
    assert_eq!(
        Os::Linux.neutral_working_directory(&paths(Os::Linux)),
        PathBuf::from("/")
    );
}

// The expected values are POSIX strings; a Windows host would join the same
// paths with `\`, so the characterisation only runs where it describes reality.
#[cfg(unix)]
#[test]
fn macos_worker_environment_is_the_historical_one() {
    // Given: the keys and values environment.rs produced before platform.rs
    let paths = paths(Os::Mac);
    let env = worker_environment(Os::Mac, &lookup, &paths, Path::new("/worker"));

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

#[cfg(unix)]
#[test]
fn linux_worker_environment_is_the_macos_one_with_a_locale_that_exists() {
    let paths = paths(Os::Linux);
    let env = worker_environment(Os::Linux, &lookup, &paths, Path::new("/worker"));

    let expected: [(&str, &str); 18] = [
        ("HOME", "/Users/tester"),
        ("LANG", "C.UTF-8"),
        ("LC_ALL", "C.UTF-8"),
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
fn linux_worker_environment_has_no_windows_keys_and_tmpdir_falls_back() {
    let env = worker_environment(Os::Linux, &|_| None, &paths(Os::Linux), Path::new("/w"));
    assert_eq!(text(&env, "TMPDIR").as_deref(), Some("/tmp"));
    for key in [
        "SYSTEMROOT",
        "USERPROFILE",
        "HF_HUB_DISABLE_SYMLINKS_WARNING",
    ] {
        assert!(!env.contains_key(OsStr::new(key)), "{key} must be absent");
    }
    assert!(
        text(&env, "PATH")
            .unwrap_or_default()
            .starts_with(&*paths(Os::Linux).engine_bin.to_string_lossy())
    );
}

#[test]
fn macos_tmpdir_falls_back_to_tmp() {
    let env = worker_environment(Os::Mac, &|_| None, &paths(Os::Mac), Path::new("/w"));
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
        home_directory(Os::Mac, &lookup),
        PathBuf::from("/Users/tester")
    );
    assert_eq!(
        home_directory(Os::Windows, &lookup),
        PathBuf::from(r"C:\Users\tester")
    );
    assert_eq!(
        home_directory(Os::Linux, &lookup),
        PathBuf::from("/Users/tester")
    );
    assert_eq!(home_directory(Os::Mac, &|_| None), std::env::temp_dir());
    assert_eq!(home_directory(Os::Linux, &|_| None), std::env::temp_dir());
}

#[test]
fn wide_strings_are_nul_terminated_utf16() {
    assert_eq!(to_wide_nul("가a"), [0xAC00, 0x61, 0]);
    assert_eq!(to_wide_nul(""), [0]);
}

fn scratch_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("galpi-{label}-{}", uuid::Uuid::now_v7()))
}

#[test]
fn cuda_is_detected_by_the_driver_library_under_system32() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a fake SystemRoot with and without nvcuda.dll
    let root = scratch_root("cuda");
    std::fs::create_dir_all(root.join("System32"))?;
    let none = Path::new("/nonexistent-galpi-root");

    // When / Then
    assert!(!cuda_driver_present(
        Os::Windows,
        Some(root.as_os_str()),
        none
    ));
    std::fs::write(root.join("System32/nvcuda.dll"), b"")?;
    assert!(cuda_driver_present(
        Os::Windows,
        Some(root.as_os_str()),
        none
    ));
    assert!(!cuda_driver_present(Os::Windows, None, none));
    // The Windows library means nothing to the other platforms.
    assert!(!cuda_driver_present(Os::Mac, Some(root.as_os_str()), none));
    assert!(!cuda_driver_present(
        Os::Linux,
        Some(root.as_os_str()),
        none
    ));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn cuda_is_detected_on_linux_by_the_driver_library_or_the_kernel_module()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a fake filesystem root with no NVIDIA files
    let root = scratch_root("cuda-linux");
    std::fs::create_dir_all(&root)?;
    assert!(!cuda_driver_present(Os::Linux, None, &root));

    // When / Then: each usual library directory counts, WSL's included
    for directory in ["usr/lib/x86_64-linux-gnu", "usr/lib64", "usr/lib/wsl/lib"] {
        let library = root.join(directory);
        std::fs::create_dir_all(&library)?;
        std::fs::write(library.join("libcuda.so.1"), b"")?;
        assert!(cuda_driver_present(Os::Linux, None, &root), "{directory}");
        std::fs::remove_file(library.join("libcuda.so.1"))?;
        assert!(!cuda_driver_present(Os::Linux, None, &root), "{directory}");
    }

    // A library by another name does not count
    std::fs::write(root.join("usr/lib64/libcudart.so.12"), b"")?;
    assert!(!cuda_driver_present(Os::Linux, None, &root));

    // The kernel module's version file does
    std::fs::create_dir_all(root.join("proc/driver/nvidia"))?;
    std::fs::write(root.join("proc/driver/nvidia/version"), b"")?;
    assert!(cuda_driver_present(Os::Linux, None, &root));
    std::fs::remove_dir_all(root)?;
    Ok(())
}
