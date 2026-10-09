use super::platform::{Os, home_directory};
use crate::application::error::AppError;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

pub const QWEN3_ENGINE_VERSION: &str = "2";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub engine: PathBuf,
    pub python: PathBuf,
    pub engine_manifest: PathBuf,
    pub models_manifest: PathBuf,
    pub engine_bin: PathBuf,
    pub cache: PathBuf,
    pub python_installations: PathBuf,
    pub qwen3_root: PathBuf,
    pub qwen3_python: PathBuf,
    pub qwen3_engine_manifest: PathBuf,
    pub qwen3_models_manifest: PathBuf,
    pub qwen3_engine_bin: PathBuf,
    /// Where meetings are written unless the user picks another folder.
    pub default_output: PathBuf,
}

impl AppPaths {
    pub fn resolve(app: &AppHandle) -> Result<Self, AppError> {
        let root = app
            .path()
            .app_local_data_dir()
            .map_err(|error| AppError::new("PATH_ERROR", error.to_string()))?;
        // Windows often redirects Documents (OneDrive), so ask the OS rather
        // than assuming `<home>/Documents`.
        let documents = app.path().document_dir().unwrap_or_else(|_| {
            home_directory(Os::current(), &|key| std::env::var_os(key)).join("Documents")
        });
        Ok(Self::from_roots(Os::current(), root, &documents))
    }

    /// Pure layout rules, so every platform's paths can be tested anywhere.
    pub fn from_roots(os: Os, root: PathBuf, documents: &Path) -> Self {
        let engine = root.join("engine");
        // The Qwen3 candidate stack lives in its own venv so the pinned
        // WhisperX environment never has to share dependency versions with it.
        let qwen3_root = engine.join("qwen3");
        let python = |base: &Path| {
            os.python_relative()
                .iter()
                .fold(base.to_owned(), |path, part| path.join(part))
        };
        Self {
            python: python(&engine),
            engine_manifest: engine.join("ready-3.8.6"),
            models_manifest: root.join("models/ready.json"),
            engine_bin: engine.join("bin"),
            cache: root.join("cache"),
            python_installations: root.join("python"),
            qwen3_python: python(&qwen3_root),
            qwen3_engine_manifest: qwen3_root.join(format!("ready-qwen3-{QWEN3_ENGINE_VERSION}")),
            qwen3_models_manifest: root.join("models/qwen3-ready.json"),
            qwen3_engine_bin: qwen3_root.join("bin"),
            default_output: documents.join("Galpi"),
            root,
            engine,
            qwen3_root,
        }
    }

    pub async fn create_directories(&self) -> Result<(), AppError> {
        for directory in [
            &self.root,
            &self.engine,
            &self.engine_bin,
            &self.cache,
            &self.python_installations,
        ] {
            tokio::fs::create_dir_all(directory)
                .await
                .map_err(|error| AppError::io("앱 데이터 디렉터리를 만들지 못했습니다", &error))?;
        }
        if let Some(parent) = self.models_manifest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| AppError::io("모델 디렉터리를 만들지 못했습니다", &error))?;
        }
        Ok(())
    }
}

pub fn uv_binary() -> Result<PathBuf, AppError> {
    let executable = if cfg!(debug_assertions) {
        None
    } else {
        Some(
            std::env::current_exe()
                .map_err(|error| AppError::io("실행 파일 위치를 확인하지 못했습니다", &error))?,
        )
    };
    uv_binary_for(
        Os::current(),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        executable.as_deref(),
    )
}

/// Debug builds use the staged sidecar in the source tree (`executable` is
/// `None`); release builds use the one the bundler put next to the executable.
pub fn uv_binary_for(
    os: Os,
    manifest_dir: &Path,
    executable: Option<&Path>,
) -> Result<PathBuf, AppError> {
    let Some(executable) = executable else {
        return Ok(manifest_dir.join("binaries").join(os.uv_staged_name()));
    };
    executable
        .parent()
        .map(|parent| parent.join(os.uv_installed_name()))
        .ok_or_else(|| AppError::new("PATH_ERROR", "번들된 uv 위치를 확인하지 못했습니다."))
}

/// `canonicalize` without the Windows `\\?\` extended-length prefix, so the
/// path is safe to hand to a worker, show in the UI, and compare by prefix.
/// Every containment check must build both sides with this same helper.
pub async fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    tokio::fs::canonicalize(path)
        .await
        .map(|resolved| dunce::simplified(&resolved).to_owned())
}

/// Blocking twin of [`canonical`].
pub fn canonical_blocking(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(|resolved| dunce::simplified(&resolved).to_owned())
}

pub fn worker_root(app: &AppHandle) -> Result<PathBuf, AppError> {
    if cfg!(debug_assertions) {
        return Ok(Path::new(env!("CARGO_MANIFEST_DIR")).join("../worker"));
    }
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| AppError::new("PATH_ERROR", error.to_string()))?;
    Ok(dunce::simplified(&resource_dir.join("resources/worker")).to_owned())
}

pub async fn prepare_job_directory(
    input: &Path,
    output_root: &Path,
) -> Result<(PathBuf, PathBuf), AppError> {
    let input = canonical(input)
        .await
        .map_err(|error| AppError::io("입력 파일을 확인하지 못했습니다", &error))?;
    let metadata = tokio::fs::metadata(&input)
        .await
        .map_err(|error| AppError::io("입력 파일 정보를 읽지 못했습니다", &error))?;
    if !metadata.is_file() {
        return Err(AppError::new(
            "INVALID_AUDIO",
            "입력 경로가 파일이 아닙니다.",
        ));
    }

    let output_root = prepare_output_root(output_root).await?;
    let stem = meeting_stem(&input);
    let job_directory = create_meeting_directory(input.parent(), &output_root, &stem).await?;
    seed_checkpoint(&output_root, &job_directory, &stem).await?;
    Ok((input, job_directory))
}

/// Create (or adopt) the Galpi output root and return its canonical path.
pub async fn prepare_output_root(output_root: &Path) -> Result<PathBuf, AppError> {
    tokio::fs::create_dir_all(output_root)
        .await
        .map_err(|error| AppError::io("출력 디렉터리를 만들지 못했습니다", &error))?;
    canonical(output_root)
        .await
        .map_err(|error| AppError::io("출력 디렉터리를 확인하지 못했습니다", &error))
}

/// Sanitized meeting name shared by the meeting folder and every artifact in it.
pub fn meeting_stem(input: &Path) -> String {
    input
        .file_stem()
        .and_then(|name| name.to_str())
        .map(sanitize_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "meeting".to_owned())
}

/// Meeting folder rule: the folder is named after the recording, so a Galpi
/// recording that already sits in `{stem}/{stem}.wav` reuses its own folder.
/// Any other input gets a fresh `{stem}` folder, deduplicated with ` 2`, ` 3`, …
pub async fn create_meeting_directory(
    input_parent: Option<&Path>,
    output_root: &Path,
    stem: &str,
) -> Result<PathBuf, AppError> {
    let recorded_folder = output_root.join(stem);
    if input_parent == Some(recorded_folder.as_path()) {
        return Ok(recorded_folder);
    }
    for attempt in 1..=100 {
        let candidate = if attempt == 1 {
            output_root.join(stem)
        } else {
            output_root.join(format!("{stem} {attempt}"))
        };
        match tokio::fs::create_dir(&candidate).await {
            Ok(()) => return canonical_job_directory(&candidate, output_root).await,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(AppError::io("작업 디렉터리를 만들지 못했습니다", &error));
            }
        }
    }
    Err(AppError::new(
        "OUTPUT_PATH_ERROR",
        "같은 이름의 회의 폴더가 너무 많아 새로 만들 수 없습니다.",
    ))
}

async fn canonical_job_directory(
    candidate: &Path,
    output_root: &Path,
) -> Result<PathBuf, AppError> {
    let directory = canonical(candidate)
        .await
        .map_err(|error| AppError::io("작업 디렉터리를 확인하지 못했습니다", &error))?;
    if !directory.starts_with(output_root) {
        return Err(AppError::new(
            "OUTPUT_PATH_ERROR",
            "작업 디렉터리가 선택한 출력 폴더를 벗어났습니다.",
        ));
    }
    Ok(directory)
}

/// Fresh folder name for a new microphone recording: `YYYY-MM-DD HHMMSS 녹음`.
pub fn recording_folder_name() -> String {
    format!("{} 녹음", chrono::Local::now().format("%Y-%m-%d %H%M%S"))
}

async fn seed_checkpoint(
    output_root: &Path,
    job_directory: &Path,
    stem: &str,
) -> Result<(), AppError> {
    let checkpoint_name = format!("{stem}.aligned.v2.json");
    let mut entries = tokio::fs::read_dir(output_root)
        .await
        .map_err(|error| AppError::io("기존 작업 디렉터리를 확인하지 못했습니다", &error))?;
    let mut latest: Option<(SystemTime, PathBuf)> = None;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| AppError::io("기존 작업 항목을 읽지 못했습니다", &error))?
    {
        if entry.path() == job_directory {
            continue;
        }
        let checkpoint = entry.path().join(&checkpoint_name);
        let Ok(metadata) = tokio::fs::symlink_metadata(&checkpoint).await else {
            continue;
        };
        if !metadata.file_type().is_file() {
            continue;
        }
        let resolved = canonical(&checkpoint)
            .await
            .map_err(|error| AppError::io("기존 체크포인트를 확인하지 못했습니다", &error))?;
        if !resolved.starts_with(output_root) {
            continue;
        }
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        if latest.as_ref().is_none_or(|(time, _)| modified > *time) {
            latest = Some((modified, resolved));
        }
    }
    if let Some((_, source)) = latest {
        tokio::fs::copy(source, job_directory.join(checkpoint_name))
            .await
            .map_err(|error| AppError::io("기존 체크포인트를 재사용하지 못했습니다", &error))?;
    }
    Ok(())
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(|character| character == '-' || character == ' ')
        .to_owned()
}

#[cfg(test)]
mod tests;
