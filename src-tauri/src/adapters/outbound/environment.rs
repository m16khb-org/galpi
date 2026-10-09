use super::paths::{AppPaths, QWEN3_ENGINE_VERSION};
use super::platform::{Os, cuda_driver_present, whisperx_lock_hash_name, worker_environment};
use crate::application::error::AppError;
use crate::application::model::EnvironmentStatus;
use crate::domain::chatgpt::AssistantTransport;
use crate::domain::engine::{ComputeDevice, EnginePreset, EngineSelection};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use tauri::AppHandle;

pub const ENGINE_VERSION: &str = "3.8.6";
/// What an installed engine's readiness marker must contain to count as
/// current: the display version plus the fingerprint of the requirements file
/// it was installed from. Editing a pin invalidates the existing virtualenv.
///
/// macOS keeps its historical `<version>+<hash>` form byte for byte so an
/// update never forces a reinstall. Windows names the lock too, so switching
/// between the CPU and CUDA builds invalidates the environment.
pub fn whisperx_marker(os: Os, device: ComputeDevice) -> String {
    match os {
        Os::MacOs => format!(
            "{ENGINE_VERSION}+{}",
            env!("GALPI_WHISPERX_REQUIREMENTS_HASH")
        ),
        Os::Windows => {
            let hash = match device {
                ComputeDevice::Cpu => env!("GALPI_WHISPERX_WIN_CPU_LOCK_HASH"),
                ComputeDevice::Cuda => env!("GALPI_WHISPERX_WIN_CUDA_LOCK_HASH"),
            };
            format!(
                "{ENGINE_VERSION}+{}-{hash}",
                whisperx_lock_hash_name(os, device)
            )
        }
    }
}

pub fn qwen3_marker() -> String {
    format!(
        "{QWEN3_ENGINE_VERSION}+{}",
        env!("GALPI_QWEN3_REQUIREMENTS_HASH")
    )
}
pub const QWEN3_MODEL_ID: &str = "Qwen/Qwen3-ASR-1.7B";
pub const QWEN3_ALIGNER_ID: &str = "Qwen/Qwen3-ForcedAligner-0.6B";
const PYANNOTE_MODEL_DIR: &str = "models--pyannote--speaker-diarization-community-1";
const QWEN3_MLX_WEIGHTS: &str = "mlx/qwen3-asr-1.7b-8bit/weights.safetensors";

pub fn diagnose(
    app: &AppHandle,
    selection: EngineSelection,
) -> Result<EnvironmentStatus, AppError> {
    let paths = AppPaths::resolve(app)?;
    Ok(status(&paths, selection))
}

pub fn status(paths: &AppPaths, selection: EngineSelection) -> EnvironmentStatus {
    let system_root = std::env::var_os("SYSTEMROOT");
    status_for(Os::current(), system_root.as_deref(), paths, selection)
}

pub fn status_for(
    os: Os,
    system_root: Option<&OsStr>,
    paths: &AppPaths,
    selection: EngineSelection,
) -> EnvironmentStatus {
    let ffmpeg = os.ffmpeg_file_name();
    let whisperx_engine = whisperx_engine_ready(os, paths, selection.device);
    let whisperx_models = whisperx_models_ready(paths);
    let qwen3_engine = qwen3_engine_ready(paths);
    let qwen3_models = qwen3_models_ready(paths);
    let qwen3_ffmpeg = paths.qwen3_engine_bin.join(ffmpeg).is_file();
    let whisperx_ffmpeg = paths.engine_bin.join(ffmpeg).is_file();
    let (engine_ready, models_ready, ffmpeg_ready, engine_version) = match selection.preset {
        EnginePreset::Qwen3 => (
            qwen3_engine,
            qwen3_models,
            qwen3_ffmpeg,
            format!("Qwen3-ASR-1.7B · {QWEN3_ENGINE_VERSION}"),
        ),
        EnginePreset::WhisperX => (
            whisperx_engine,
            whisperx_models,
            whisperx_ffmpeg,
            format!("WhisperX {ENGINE_VERSION}"),
        ),
    };
    EnvironmentStatus {
        engine_preset: selection.preset,
        engine_ready,
        models_ready,
        ffmpeg_ready,
        qwen3_ready: qwen3_engine && qwen3_models && qwen3_ffmpeg,
        whisperx_ready: whisperx_engine && whisperx_models && whisperx_ffmpeg,
        data_directory: paths.root.to_string_lossy().into_owned(),
        default_output_directory: paths.default_output.to_string_lossy().into_owned(),
        engine_version,
        compute_device: selection.device,
        available_presets: os.presets().to_vec(),
        available_devices: os.devices().to_vec(),
        cuda_driver_detected: os == Os::Windows && cuda_driver_present(system_root),
    }
}

fn whisperx_engine_ready(os: Os, paths: &AppPaths, device: ComputeDevice) -> bool {
    paths.python.is_file()
        && std::fs::read_to_string(&paths.engine_manifest)
            .is_ok_and(|marker| marker == whisperx_marker(os, device))
}

fn qwen3_engine_ready(paths: &AppPaths) -> bool {
    paths.qwen3_python.is_file()
        && std::fs::read_to_string(&paths.qwen3_engine_manifest)
            .is_ok_and(|marker| marker == qwen3_marker())
}

pub fn process_environment(
    paths: &AppPaths,
    worker_root: &Path,
    token: Option<&str>,
) -> HashMap<OsString, OsString> {
    let mut env = worker_environment(
        Os::current(),
        &|key| std::env::var_os(key),
        paths,
        worker_root,
    );
    if let Some(token) = token.map(str::trim).filter(|token| !token.is_empty()) {
        env.insert("HF_TOKEN".into(), token.into());
    }
    env
}

/// Worker environment for assistant calls, carrying the assistant credential.
/// Chat Completions may also override the endpoint and reasoning effort; a
/// Responses call is named explicitly and uses neither.
pub fn assistant_environment(
    paths: &AppPaths,
    worker_root: &Path,
    api_key: &str,
    base_url: Option<&str>,
    reasoning_effort: Option<&str>,
    transport: AssistantTransport,
) -> HashMap<OsString, OsString> {
    let mut env = process_environment(paths, worker_root, None);
    env.insert("GALPI_ASSISTANT_API_KEY".into(), api_key.into());
    match transport {
        AssistantTransport::Responses => {
            env.insert("GALPI_ASSISTANT_TRANSPORT".into(), "responses".into());
        }
        AssistantTransport::ChatCompletions => {
            if let Some(base_url) = base_url {
                env.insert("GALPI_ASSISTANT_BASE_URL".into(), base_url.into());
            }
            if let Some(reasoning_effort) = reasoning_effort {
                env.insert(
                    "GALPI_ASSISTANT_REASONING_EFFORT".into(),
                    reasoning_effort.into(),
                );
            }
        }
    }
    env
}

fn whisperx_models_ready(paths: &AppPaths) -> bool {
    let manifest = std::fs::read_to_string(&paths.models_manifest)
        .ok()
        .and_then(|contents| serde_json::from_str::<serde_json::Value>(&contents).ok());
    let manifest_valid = manifest.is_some_and(|value| {
        value.get("protocol").and_then(serde_json::Value::as_u64) == Some(1)
            && value.get("whisperx").and_then(serde_json::Value::as_str) == Some(ENGINE_VERSION)
    });
    let hub = paths.cache.join("huggingface/hub");
    manifest_valid
        && [
            "models--mobiuslabsgmbh--faster-whisper-large-v3-turbo",
            "models--kresnik--wav2vec2-large-xlsr-korean",
            PYANNOTE_MODEL_DIR,
        ]
        .iter()
        .all(|model| hub.join(model).is_dir())
}

fn qwen3_models_ready(paths: &AppPaths) -> bool {
    let manifest = std::fs::read_to_string(&paths.qwen3_models_manifest)
        .ok()
        .and_then(|contents| serde_json::from_str::<serde_json::Value>(&contents).ok());
    let manifest_valid = manifest.is_some_and(|value| {
        value.get("protocol").and_then(serde_json::Value::as_u64) == Some(1)
            && value.get("qwen3").and_then(serde_json::Value::as_str) == Some(QWEN3_ENGINE_VERSION)
    });
    let hub = paths.cache.join("huggingface/hub");
    // Diarization stays on pyannote community-1 in both presets, so the
    // shared model must be present for either engine to run meetings.
    manifest_valid
        && [
            cache_dir_name(QWEN3_MODEL_ID),
            cache_dir_name(QWEN3_ALIGNER_ID),
            PYANNOTE_MODEL_DIR.to_owned(),
        ]
        .iter()
        .all(|model| hub.join(model).is_dir())
        && paths.cache.join(QWEN3_MLX_WEIGHTS).is_file()
}

/// Hugging Face cache directories use `models--Org--Name` from the repo id.
fn cache_dir_name(repo_id: &str) -> String {
    format!("models--{}", repo_id.replace('/', "--"))
}

#[cfg(test)]
mod tests {
    use super::{ENGINE_VERSION, status_for, whisperx_marker};
    use crate::adapters::outbound::paths::AppPaths;
    use crate::adapters::outbound::platform::Os;
    use crate::domain::engine::{ComputeDevice, EnginePreset, EngineSelection};
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    fn selection(preset: EnginePreset, device: ComputeDevice) -> EngineSelection {
        EngineSelection { preset, device }
    }

    #[test]
    fn the_macos_marker_is_the_historical_version_plus_requirements_hash() {
        // Given / When / Then: existing installs must not be asked to reinstall
        let expected = format!(
            "{ENGINE_VERSION}+{}",
            env!("GALPI_WHISPERX_REQUIREMENTS_HASH")
        );
        assert_eq!(whisperx_marker(Os::MacOs, ComputeDevice::Cpu), expected);
        assert_eq!(whisperx_marker(Os::MacOs, ComputeDevice::Cuda), expected);
    }

    #[test]
    fn windows_markers_differ_by_device_and_from_macos() {
        let mac = whisperx_marker(Os::MacOs, ComputeDevice::Cpu);
        let cpu = whisperx_marker(Os::Windows, ComputeDevice::Cpu);
        let cuda = whisperx_marker(Os::Windows, ComputeDevice::Cuda);
        assert_ne!(cpu, cuda);
        assert_ne!(cpu, mac);
        assert_ne!(cuda, mac);
        assert!(cpu.contains("win-cpu-"));
        assert!(cuda.contains("win-cuda-"));
    }

    #[test]
    fn windows_status_looks_for_ffmpeg_exe_and_offers_whisperx_only()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a Windows layout with only ffmpeg.exe in the engine bin
        let root = std::env::temp_dir().join(format!("galpi-status-{}", uuid::Uuid::now_v7()));
        let paths = AppPaths::from_roots(Os::Windows, root.clone(), Path::new("/documents"));
        std::fs::create_dir_all(&paths.engine_bin)?;
        std::fs::write(paths.engine_bin.join("ffmpeg.exe"), b"")?;

        // When
        let status = status_for(
            Os::Windows,
            None,
            &paths,
            selection(EnginePreset::WhisperX, ComputeDevice::Cuda),
        );

        // Then
        assert!(status.ffmpeg_ready);
        assert!(!status.engine_ready);
        assert_eq!(status.available_presets, [EnginePreset::WhisperX]);
        assert_eq!(
            status.available_devices,
            [ComputeDevice::Cpu, ComputeDevice::Cuda]
        );
        assert_eq!(status.compute_device, ComputeDevice::Cuda);
        assert!(!status.cuda_driver_detected);
        assert_eq!(
            PathBuf::from(&status.default_output_directory),
            PathBuf::from("/documents/Galpi")
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn macos_status_offers_both_presets_and_no_device_choice() {
        let paths = AppPaths::from_roots(
            Os::MacOs,
            PathBuf::from("/nonexistent-galpi"),
            Path::new("/docs"),
        );
        let status = status_for(
            Os::MacOs,
            Some(OsStr::new("/ignored")),
            &paths,
            selection(EnginePreset::Qwen3, ComputeDevice::Cpu),
        );
        assert_eq!(
            status.available_presets,
            [EnginePreset::Qwen3, EnginePreset::WhisperX]
        );
        assert_eq!(status.available_devices, []);
        assert!(!status.cuda_driver_detected);
    }
}

#[cfg(test)]
mod assistant_environment_tests {
    use super::assistant_environment;
    use crate::adapters::outbound::paths::AppPaths;
    use crate::adapters::outbound::platform::Os;
    use crate::domain::chatgpt::AssistantTransport;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    fn paths() -> AppPaths {
        AppPaths::from_roots(
            Os::MacOs,
            PathBuf::from("/tmp/galpi-environment-test"),
            Path::new("/tmp/documents"),
        )
    }

    fn environment(
        base_url: Option<&str>,
        reasoning_effort: Option<&str>,
        transport: AssistantTransport,
    ) -> HashMap<OsString, OsString> {
        assistant_environment(
            &paths(),
            Path::new("/tmp/worker"),
            "bearer-value",
            base_url,
            reasoning_effort,
            transport,
        )
    }

    fn value<'a>(env: &'a HashMap<OsString, OsString>, key: &str) -> Option<&'a OsString> {
        env.get(&OsString::from(key))
    }

    #[test]
    fn responses_transport_is_named_and_drops_endpoint_and_effort() {
        // Given: a ChatGPT job that still carries stale API-key settings
        let env = environment(
            Some("https://example.invalid/v1"),
            Some("max"),
            AssistantTransport::Responses,
        );

        // Then: the worker is told to use Responses and gets only the credential
        assert_eq!(
            value(&env, "GALPI_ASSISTANT_TRANSPORT"),
            Some(&OsString::from("responses"))
        );
        assert_eq!(
            value(&env, "GALPI_ASSISTANT_API_KEY"),
            Some(&OsString::from("bearer-value"))
        );
        assert_eq!(value(&env, "GALPI_ASSISTANT_BASE_URL"), None);
        assert_eq!(value(&env, "GALPI_ASSISTANT_REASONING_EFFORT"), None);
    }

    #[test]
    fn chat_completions_transport_keeps_the_existing_keys() {
        // Given
        let env = environment(
            Some("https://example.invalid/v1"),
            Some("max"),
            AssistantTransport::ChatCompletions,
        );

        // Then: the three assistant keys are unchanged and no transport is named
        assert_eq!(
            value(&env, "GALPI_ASSISTANT_API_KEY"),
            Some(&OsString::from("bearer-value"))
        );
        assert_eq!(
            value(&env, "GALPI_ASSISTANT_BASE_URL"),
            Some(&OsString::from("https://example.invalid/v1"))
        );
        assert_eq!(
            value(&env, "GALPI_ASSISTANT_REASONING_EFFORT"),
            Some(&OsString::from("max"))
        );
        assert_eq!(value(&env, "GALPI_ASSISTANT_TRANSPORT"), None);
    }

    #[test]
    fn chat_completions_without_overrides_sets_only_the_key() {
        let env = environment(None, None, AssistantTransport::ChatCompletions);

        assert!(value(&env, "GALPI_ASSISTANT_API_KEY").is_some());
        assert_eq!(value(&env, "GALPI_ASSISTANT_BASE_URL"), None);
        assert_eq!(value(&env, "GALPI_ASSISTANT_REASONING_EFFORT"), None);
        assert_eq!(value(&env, "GALPI_ASSISTANT_TRANSPORT"), None);
    }
}
