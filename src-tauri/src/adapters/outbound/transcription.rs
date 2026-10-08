use crate::adapters::outbound::environment::process_environment;
use crate::adapters::outbound::paths::{AppPaths, canonical, worker_root};
use crate::adapters::outbound::process::{ProcessSpec, run_process};
use crate::adapters::outbound::refinement::write_private_file;
use crate::application::error::AppError;
use crate::application::model::CompletedTranscription;
use crate::application::ports::JobEvents;
use crate::domain::artifact::Artifacts;
use crate::domain::engine::EnginePreset;
use crate::domain::job::SpeakerHint;
use crate::domain::worker::WorkerEvent;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tokio::sync::oneshot;
use uuid::Uuid;

pub struct Runtime<'a> {
    pub app: &'a AppHandle,
    pub events: &'a dyn JobEvents,
    pub paths: &'a AppPaths,
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    runtime: Runtime<'_>,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    input: &Path,
    output: &Path,
    hint: &SpeakerHint,
    engine: EnginePreset,
    asr_context: Option<&str>,
) -> Result<CompletedTranscription, AppError> {
    let context = match asr_context {
        Some(context) => Some(write_private_file(job_id, "asr-context", context).await?),
        None => None,
    };
    let result = run_worker(
        &runtime,
        job_id,
        cancel,
        input,
        output,
        hint,
        engine,
        context.as_deref(),
    )
    .await;
    if let Some(temporary) = context {
        let _removed = tokio::fs::remove_file(&temporary).await;
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_worker(
    runtime: &Runtime<'_>,
    job_id: Uuid,
    cancel: &mut oneshot::Receiver<()>,
    input: &Path,
    output: &Path,
    hint: &SpeakerHint,
    engine: EnginePreset,
    asr_context: Option<&Path>,
) -> Result<CompletedTranscription, AppError> {
    let root = worker_root(runtime.app)?;
    // The Qwen3 candidate stack runs from its own venv so the pinned
    // WhisperX environment keeps its dependency versions untouched.
    let python = match engine {
        EnginePreset::Qwen3 => runtime.paths.qwen3_python.clone(),
        EnginePreset::WhisperX => runtime.paths.python.clone(),
    };
    let mut args = vec![
        "-m".into(),
        "galpi_worker".into(),
        "transcribe".into(),
        "--input".into(),
        input.as_os_str().to_owned(),
        "--output".into(),
        output.as_os_str().to_owned(),
        "--engine".into(),
        engine.as_str().into(),
    ];
    if let Some(context) = asr_context {
        args.push("--asr-context".into());
        args.push(context.as_os_str().to_owned());
    }
    match hint {
        SpeakerHint::Auto => {}
        SpeakerHint::Exact { count } => {
            args.push("--num-speakers".into());
            args.push(count.to_string().into());
        }
        SpeakerHint::Range { min, max } => {
            args.push("--speaker-range".into());
            args.push(min.to_string().into());
            args.push(max.to_string().into());
        }
    }

    let process = run_process(
        runtime.events,
        job_id,
        ProcessSpec {
            program: python,
            current_dir: root.clone(),
            args,
            env: {
                // Transcription only runs after the readiness gate, so the
                // Qwen3 stack can load exclusively from the prepared cache.
                let mut env = process_environment(runtime.paths, &root, None);
                if matches!(engine, EnginePreset::Qwen3) {
                    env.insert("HF_HUB_OFFLINE".into(), "1".into());
                    env.insert("TRANSFORMERS_OFFLINE".into(), "1".into());
                }
                env
            },
            worker_protocol: true,
        },
        cancel,
    )
    .await?;
    let (srt, txt, checkpoint, segments, filtered) = match process.completed {
        Some(WorkerEvent::Completed {
            srt,
            txt,
            checkpoint,
            segments,
            filtered,
        }) => (
            PathBuf::from(srt),
            PathBuf::from(txt),
            // The Qwen3 pipeline publishes srt/txt only, so its completion
            // carries an empty checkpoint string meaning "no checkpoint".
            if checkpoint.is_empty() {
                None
            } else {
                Some(PathBuf::from(checkpoint))
            },
            segments,
            filtered,
        ),
        _ => {
            return Err(AppError::new(
                "WORKER_PROTOCOL_ERROR",
                "워커가 완료 결과를 전달하지 않았습니다.",
            ));
        }
    };
    let artifacts = validate_artifacts(output, &srt, &txt, checkpoint.as_deref(), input).await?;
    Ok(CompletedTranscription {
        artifacts,
        segments,
        filtered,
    })
}

async fn validate_artifacts(
    job_directory: &Path,
    srt: &Path,
    txt: &Path,
    checkpoint: Option<&Path>,
    source_audio: &Path,
) -> Result<Artifacts, AppError> {
    let root = canonical(job_directory)
        .await
        .map_err(|error| AppError::io("작업 디렉터리를 확인하지 못했습니다", &error))?;
    let srt = canonical_artifact(&root, srt).await?;
    let txt = canonical_artifact(&root, txt).await?;
    let checkpoint = match checkpoint {
        Some(path) => Some(canonical_artifact(&root, path).await?),
        None => None,
    };
    Ok(Artifacts {
        srt: Some(srt),
        txt,
        checkpoint,
        minutes: None,
        output_directory: root,
        source_audio: Some(source_audio.to_path_buf()),
    })
}

async fn canonical_artifact(root: &Path, path: &Path) -> Result<PathBuf, AppError> {
    let path = canonical(path)
        .await
        .map_err(|error| AppError::io("결과 파일을 확인하지 못했습니다", &error))?;
    if !path.starts_with(root) {
        return Err(AppError::new(
            "WORKER_PROTOCOL_ERROR",
            "워커가 작업 디렉터리 밖의 결과를 반환했습니다.",
        ));
    }
    Ok(path)
}
