pub use super::environment::diagnose;
use super::environment::{process_environment, qwen3_marker, status, whisperx_marker};
use super::model_cache::{can_use_offline_cache, import_standard_cache};
use super::platform::Os;
use crate::adapters::outbound::paths::{AppPaths, uv_binary, worker_root};
use crate::adapters::outbound::process::{ProcessSpec, emit, run_process};
use crate::application::error::AppError;
use crate::application::model::EnvironmentStatus;
use crate::application::ports::JobEvents;
use crate::domain::engine::{ComputeDevice, EnginePreset, EngineSelection};
use crate::domain::job::SetupRequest;
use crate::domain::worker::WorkerEvent;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use tauri::AppHandle;
use tokio::sync::oneshot;
use uuid::Uuid;

pub async fn prepare(
    app: &AppHandle,
    events: &dyn JobEvents,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    request: &SetupRequest,
    selection: EngineSelection,
) -> Result<EnvironmentStatus, AppError> {
    let paths = AppPaths::resolve(app)?;
    paths.create_directories().await?;
    let root = worker_root(app)?;
    let install_env = process_environment(&paths, &root, None);
    let mut model_env = process_environment(&paths, &root, request.hugging_face_token.as_deref());
    let current = status(&paths, selection);
    if current.is_ready() {
        emit_phase(
            events,
            job_id,
            "ready",
            100.0,
            "선택한 엔진과 모델이 이미 준비되어 있습니다.",
        )?;
        return Ok(current);
    }

    match selection.preset {
        EnginePreset::WhisperX => {
            prepare_whisperx(
                events,
                job_id,
                cancel,
                &paths,
                &root,
                &install_env,
                &mut model_env,
                request,
                selection.device,
            )
            .await
        }
        EnginePreset::Qwen3 => {
            prepare_qwen3(
                events,
                job_id,
                cancel,
                &paths,
                &root,
                &install_env,
                &model_env,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn prepare_whisperx(
    events: &dyn JobEvents,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    paths: &AppPaths,
    root: &Path,
    install_env: &HashMap<OsString, OsString>,
    model_env: &mut HashMap<OsString, OsString>,
    request: &SetupRequest,
    device: ComputeDevice,
) -> Result<EnvironmentStatus, AppError> {
    let selection = EngineSelection {
        preset: EnginePreset::WhisperX,
        device,
    };
    let current = status(paths, selection);
    if !current.engine_ready {
        install_whisperx_engine(events, job_id, cancel, paths, root, install_env, device).await?;
    }
    let imported = match import_standard_cache(paths).await {
        Ok(imported) if imported > 0 => emit_phase(
            events,
            job_id,
            "models",
            45.0,
            "기존 WhisperX 모델 캐시를 앱 전용 공간에서 재사용합니다.",
        )
        .map(|()| imported)?,
        Ok(_) => 0,
        Err(error) => {
            emit(
                events,
                job_id,
                WorkerEvent::Log {
                    stream: "stderr".to_owned(),
                    message: error.to_string(),
                },
            )?;
            0
        }
    };
    if can_use_offline_cache(imported, request.hugging_face_token.as_deref()) {
        model_env.insert("HF_HUB_OFFLINE".into(), "1".into());
        model_env.insert("TRANSFORMERS_OFFLINE".into(), "1".into());
    }
    let installed = status(paths, selection);
    if !installed.models_ready || !installed.ffmpeg_ready {
        run_worker_prepare(
            events,
            job_id,
            cancel,
            &paths.python.clone(),
            &paths.models_manifest.clone(),
            &paths.engine_bin.clone(),
            root,
            model_env,
            EnginePreset::WhisperX,
        )
        .await?;
    }

    let completed = status(paths, selection);
    if !completed.is_ready() {
        return Err(AppError::new(
            "SETUP_INCOMPLETE",
            "준비 프로세스가 종료됐지만 필수 파일이 확인되지 않았습니다.",
        ));
    }
    Ok(completed)
}

async fn prepare_qwen3(
    events: &dyn JobEvents,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    paths: &AppPaths,
    root: &Path,
    install_env: &HashMap<OsString, OsString>,
    model_env: &HashMap<OsString, OsString>,
) -> Result<EnvironmentStatus, AppError> {
    let current = status(
        paths,
        EngineSelection {
            preset: EnginePreset::Qwen3,
            device: ComputeDevice::Cpu,
        },
    );
    if !current.engine_ready {
        install_qwen3_engine(events, job_id, cancel, paths, root, install_env).await?;
    }
    let installed = status(
        paths,
        EngineSelection {
            preset: EnginePreset::Qwen3,
            device: ComputeDevice::Cpu,
        },
    );
    if !installed.models_ready || !installed.ffmpeg_ready {
        run_worker_prepare(
            events,
            job_id,
            cancel,
            &paths.qwen3_python.clone(),
            &paths.qwen3_models_manifest.clone(),
            &paths.qwen3_engine_bin.clone(),
            root,
            model_env,
            EnginePreset::Qwen3,
        )
        .await?;
    }
    let completed = status(
        paths,
        EngineSelection {
            preset: EnginePreset::Qwen3,
            device: ComputeDevice::Cpu,
        },
    );
    if !completed.is_ready() {
        return Err(AppError::new(
            "SETUP_INCOMPLETE",
            "준비 프로세스가 종료됐지만 필수 파일이 확인되지 않았습니다.",
        ));
    }
    Ok(completed)
}

async fn install_whisperx_engine(
    events: &dyn JobEvents,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    paths: &AppPaths,
    root: &Path,
    env: &HashMap<OsString, OsString>,
    device: ComputeDevice,
) -> Result<(), AppError> {
    let os = Os::current();
    let uv = uv_binary()?;
    let cwd = os.neutral_working_directory(paths);
    emit_phase(
        events,
        job_id,
        "engine",
        5.0,
        "Python 3.12 런타임을 준비합니다.",
    )?;
    run_raw(
        events,
        job_id,
        cancel,
        &cwd,
        &uv,
        os_args(["python", "install", "3.12"]),
        env,
    )
    .await?;

    emit_phase(
        events,
        job_id,
        "engine",
        22.0,
        "앱 전용 가상환경을 만듭니다.",
    )?;
    run_raw(
        events,
        job_id,
        cancel,
        &cwd,
        &uv,
        vec![
            "venv".into(),
            // A failed first attempt leaves a partial venv behind; replacing
            // it keeps retries idempotent instead of tripping uv's refusal.
            "--clear".into(),
            "--python".into(),
            "3.12".into(),
            paths.engine.join(".venv").into_os_string(),
        ],
        env,
    )
    .await?;

    emit_phase(
        events,
        job_id,
        "engine",
        35.0,
        "WhisperX와 내장 ffmpeg를 설치합니다.",
    )?;
    run_raw(
        events,
        job_id,
        cancel,
        &cwd,
        &uv,
        whisperx_install_args(os, device, &paths.python, root),
        env,
    )
    .await?;
    tokio::fs::write(&paths.engine_manifest, whisperx_marker(os, device))
        .await
        .map_err(|error| AppError::io("엔진 준비 마커를 쓰지 못했습니다", &error))
}

/// `uv pip install` arguments for the selected `WhisperX` lock. Every lock is
/// hash-pinned, so `--require-hashes` is unconditional.
fn whisperx_install_args(
    os: Os,
    device: ComputeDevice,
    python: &Path,
    root: &Path,
) -> Vec<OsString> {
    let lock = os.whisperx_lock(device);
    let mut args: Vec<OsString> = vec![
        "pip".into(),
        "install".into(),
        "--python".into(),
        python.as_os_str().to_owned(),
        "-r".into(),
        root.join(lock.file_name).into_os_string(),
        "--require-hashes".into(),
    ];
    args.extend(lock.extra_args.iter().map(OsString::from));
    args
}

async fn install_qwen3_engine(
    events: &dyn JobEvents,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    paths: &AppPaths,
    root: &Path,
    env: &HashMap<OsString, OsString>,
) -> Result<(), AppError> {
    let os = Os::current();
    let uv = uv_binary()?;
    let cwd = os.neutral_working_directory(paths);
    emit_phase(
        events,
        job_id,
        "engine",
        5.0,
        "Python 3.12 런타임을 준비합니다.",
    )?;
    run_raw(
        events,
        job_id,
        cancel,
        &cwd,
        &uv,
        os_args(["python", "install", "3.12"]),
        env,
    )
    .await?;

    emit_phase(
        events,
        job_id,
        "engine",
        22.0,
        "Qwen3 전용 가상환경을 만듭니다.",
    )?;
    run_raw(
        events,
        job_id,
        cancel,
        &cwd,
        &uv,
        vec![
            "venv".into(),
            "--clear".into(),
            "--python".into(),
            "3.12".into(),
            paths.qwen3_root.join(".venv").into_os_string(),
        ],
        env,
    )
    .await?;

    emit_phase(
        events,
        job_id,
        "engine",
        35.0,
        "Qwen3 ASR 런타임을 설치합니다.",
    )?;
    run_raw(
        events,
        job_id,
        cancel,
        &cwd,
        &uv,
        vec![
            "pip".into(),
            "install".into(),
            "--python".into(),
            paths.qwen3_python.clone().into_os_string(),
            "-r".into(),
            root.join("requirements-qwen3.lock").into_os_string(),
        ],
        env,
    )
    .await?;
    tokio::fs::create_dir_all(&paths.qwen3_root)
        .await
        .map_err(|error| AppError::io("Qwen3 엔진 폴더를 만들지 못했습니다", &error))?;
    tokio::fs::write(&paths.qwen3_engine_manifest, qwen3_marker())
        .await
        .map_err(|error| AppError::io("엔진 준비 마커를 쓰지 못했습니다", &error))
}

#[allow(clippy::too_many_arguments)]
async fn run_worker_prepare(
    events: &dyn JobEvents,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    python: &Path,
    manifest: &Path,
    engine_bin: &Path,
    root: &Path,
    env: &HashMap<OsString, OsString>,
    engine: EnginePreset,
) -> Result<(), AppError> {
    emit_phase(
        events,
        job_id,
        "models",
        50.0,
        "전사·정렬·화자분리 모델을 확인합니다.",
    )?;
    run_process(
        events,
        job_id,
        ProcessSpec {
            program: python.to_owned(),
            current_dir: root.to_owned(),
            args: vec![
                "-m".into(),
                "galpi_worker".into(),
                "prepare".into(),
                "--manifest".into(),
                manifest.to_owned().into_os_string(),
                "--engine-bin".into(),
                engine_bin.to_owned().into_os_string(),
                "--engine".into(),
                engine.as_str().into(),
            ],
            env: env.clone(),
            worker_protocol: true,
        },
        cancel,
    )
    .await?;
    Ok(())
}

async fn run_raw(
    events: &dyn JobEvents,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    working_directory: &Path,
    program: &Path,
    args: Vec<OsString>,
    env: &HashMap<OsString, OsString>,
) -> Result<(), AppError> {
    run_process(
        events,
        job_id,
        ProcessSpec {
            program: program.to_owned(),
            current_dir: working_directory.to_owned(),
            args,
            env: env.clone(),
            worker_protocol: false,
        },
        cancel,
    )
    .await?;
    Ok(())
}

fn emit_phase(
    events: &dyn JobEvents,
    job_id: Uuid,
    phase: &str,
    percent: f32,
    message: &str,
) -> Result<(), AppError> {
    emit(
        events,
        job_id,
        WorkerEvent::Phase {
            phase: phase.to_owned(),
            percent,
            message: message.to_owned(),
        },
    )
}

fn os_args<const N: usize>(values: [&str; N]) -> Vec<OsString> {
    values.into_iter().map(OsString::from).collect()
}

#[cfg(test)]
mod tests {
    use super::{ComputeDevice, Os, whisperx_install_args};
    use crate::adapters::outbound::environment::QWEN3_ALIGNER_ID;
    use std::ffi::OsString;
    use std::path::Path;

    fn args(os: Os, device: ComputeDevice) -> Vec<String> {
        whisperx_install_args(os, device, Path::new("py"), Path::new("root"))
            .into_iter()
            .map(|argument: OsString| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn every_whisperx_lock_installs_with_hashes_and_only_cuda_changes_the_index_strategy() {
        // Given / When
        let mac = args(Os::MacOs, ComputeDevice::Cpu);
        let cpu = args(Os::Windows, ComputeDevice::Cpu);
        let cuda = args(Os::Windows, ComputeDevice::Cuda);

        // Then
        for installed in [&mac, &cpu, &cuda] {
            assert!(installed.contains(&"--require-hashes".to_owned()));
        }
        assert!(
            mac.iter()
                .any(|argument| argument.ends_with("requirements.lock"))
        );
        assert!(
            cpu.iter()
                .any(|argument| argument.ends_with("requirements-windows-cpu.lock"))
        );
        assert!(
            cuda.iter()
                .any(|argument| argument.ends_with("requirements-windows-cuda.lock"))
        );
        assert!(!mac.contains(&"--index-strategy".to_owned()));
        assert!(!cpu.contains(&"--index-strategy".to_owned()));
        assert!(
            cuda.windows(2)
                .any(|pair| pair == ["--index-strategy", "unsafe-best-match"])
        );
    }

    #[test]
    fn qwen3_model_ids_match_huggingface_repo_names() {
        // The readiness check maps repo ids directly into cache directory
        // names (models--Org--Name), so the ids must keep that shape.
        assert!(QWEN3_ALIGNER_ID.contains('/'));
        assert!(QWEN3_ALIGNER_ID.starts_with("Qwen/"));
    }
}
