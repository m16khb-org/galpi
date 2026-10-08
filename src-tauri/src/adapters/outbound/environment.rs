use super::paths::{AppPaths, QWEN3_ENGINE_VERSION};
use crate::application::error::AppError;
use crate::application::model::EnvironmentStatus;
use crate::domain::chatgpt::AssistantTransport;
use crate::domain::engine::EnginePreset;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

pub const ENGINE_VERSION: &str = "3.8.6";
/// What an installed engine's readiness marker must contain to count as
/// current: the display version plus the fingerprint of the requirements file
/// it was installed from. Editing a pin invalidates the existing virtualenv.
pub fn whisperx_marker() -> String {
    format!(
        "{ENGINE_VERSION}+{}",
        env!("GALPI_WHISPERX_REQUIREMENTS_HASH")
    )
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

pub fn diagnose(app: &AppHandle, preset: EnginePreset) -> Result<EnvironmentStatus, AppError> {
    let paths = AppPaths::resolve(app)?;
    Ok(status(&paths, preset))
}

pub fn status(paths: &AppPaths, preset: EnginePreset) -> EnvironmentStatus {
    let whisperx_engine = whisperx_engine_ready(paths);
    let whisperx_models = whisperx_models_ready(paths);
    let qwen3_engine = qwen3_engine_ready(paths);
    let qwen3_models = qwen3_models_ready(paths);
    let (engine_ready, models_ready, ffmpeg_ready, engine_version) = match preset {
        EnginePreset::Qwen3 => (
            qwen3_engine,
            qwen3_models,
            paths.qwen3_engine_bin.join("ffmpeg").is_file(),
            format!("Qwen3-ASR-1.7B · {QWEN3_ENGINE_VERSION}"),
        ),
        EnginePreset::WhisperX => (
            whisperx_engine,
            whisperx_models,
            paths.engine_bin.join("ffmpeg").is_file(),
            format!("WhisperX {ENGINE_VERSION}"),
        ),
    };
    EnvironmentStatus {
        engine_preset: preset,
        engine_ready,
        models_ready,
        ffmpeg_ready,
        qwen3_ready: qwen3_engine
            && qwen3_models
            && paths.qwen3_engine_bin.join("ffmpeg").is_file(),
        whisperx_ready: whisperx_engine
            && whisperx_models
            && paths.engine_bin.join("ffmpeg").is_file(),
        data_directory: paths.root.to_string_lossy().into_owned(),
        default_output_directory: home_directory()
            .join("Documents/Galpi")
            .to_string_lossy()
            .into_owned(),
        engine_version,
    }
}

fn whisperx_engine_ready(paths: &AppPaths) -> bool {
    paths.python.is_file()
        && std::fs::read_to_string(&paths.engine_manifest)
            .is_ok_and(|marker| marker == whisperx_marker())
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
    let mut env = HashMap::from([
        ("HOME".into(), home_directory().into_os_string()),
        ("LANG".into(), "ko_KR.UTF-8".into()),
        ("LC_ALL".into(), "ko_KR.UTF-8".into()),
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
        (
            "PATH".into(),
            format!(
                "{}:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
                paths.engine_bin.to_string_lossy()
            )
            .into(),
        ),
        (
            "TMPDIR".into(),
            std::env::var_os("TMPDIR").unwrap_or_else(|| "/tmp".into()),
        ),
    ]);
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

fn home_directory() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::assistant_environment;
    use crate::adapters::outbound::paths::AppPaths;
    use crate::domain::chatgpt::AssistantTransport;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    fn paths() -> AppPaths {
        let root = PathBuf::from("/tmp/galpi-environment-test");
        let engine = root.join("engine");
        let qwen3_root = engine.join("qwen3");
        AppPaths {
            python: engine.join(".venv/bin/python"),
            engine_manifest: engine.join("ready"),
            models_manifest: root.join("models/ready.json"),
            engine_bin: engine.join("bin"),
            cache: root.join("cache"),
            python_installations: root.join("python"),
            qwen3_python: qwen3_root.join(".venv/bin/python"),
            qwen3_engine_manifest: qwen3_root.join("ready"),
            qwen3_models_manifest: root.join("models/qwen3-ready.json"),
            qwen3_engine_bin: qwen3_root.join("bin"),
            root,
            engine,
            qwen3_root,
        }
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
