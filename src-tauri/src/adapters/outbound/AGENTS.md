# OUTBOUND ADAPTER KNOWLEDGE

## OVERVIEW

OS egress for the Rust host: worker supervision, CPAL capture, WAV persistence,
engine/model staging, path policy, and desktop port implementations. Limits,
cleanup, and stable error codes are the contract here.

## STRUCTURE

```text
outbound/
├── chatgpt/          # ChatGptAuthPort: OAuth/PKCE, loopback callback, ID-token, /v1/models, system browser; SystemClock
├── settings/chatgpt.rs # ChatGptStore on LocalSettingsStore (tokens in 0600 settings.json)
├── desktop.rs        # EnginePort, TranscriptionPort, TranscriptImportPort, ArtifactPort facade
├── environment.rs    # Readiness probe and worker environment
├── import.rs         # Transcript import into a per-meeting folder
├── model_cache.rs    # Safe model import from the user Hugging Face cache
├── paths.rs          # Debug/release resources, job dirs, checkpoint seeding
├── process.rs        # Child supervisor and bounded line reader
├── setup.rs          # uv/Python install and model preparation
├── transcription.rs  # Worker invocation and artifact containment
├── process/          # Process-group guard and tests
└── recording/        # NativeRecorder, capture, writer, cleanup, failure
```

## WHERE TO LOOK

| Task | Location |
|------|----------|
| Add a supervised step | `ProcessSpec` in `process.rs`; callers in `setup.rs`, `transcription.rs` |
| Change output bounds | `MAX_LINE_BYTES`, `error_tail` in `process.rs` |
| Change kill escalation | Cancel arms in `process.rs`, Drop in `process/guard.rs` |
| Change capture details | `recording/capture.rs` |
| Change backpressure/RIFF caps | `recording/writer.rs` |
| Change error mapping | `recording/cleanup.rs`, `recording/failure.rs` |
| Change worker variables | `platform::worker_environment` in `platform.rs` (used by `environment.rs`) |
| Change refinement transport variables | `assistant_environment` in `environment.rs`; `GALPI_ASSISTANT_TRANSPORT=responses` is set only for the ChatGPT Responses transport |
| Change meeting folder or artifact naming | `paths.rs` (`meeting_stem`, `create_meeting_directory`, `recording_folder_name`, `sanitize_name`); default root in `environment.rs` (`~/Documents/Galpi`) |
| Change debug/release paths | `uv_binary`, `worker_root` in `paths.rs` |

## CONVENTIONS

- Spawn with `env_clear`, the environment built only by
  `platform::worker_environment`, null stdin, `kill_on_drop`, and the
  platform process guard (`process/guard.rs`).
- Cancellation on Unix signals the dedicated process group: SIGTERM, a three
  second wait, then SIGKILL. On Windows the child joins a Job Object
  (`KILL_ON_JOB_CLOSE`); there is no group SIGTERM, so the graceful step is an
  immediate `TerminateJobObject`. The armed guard also kills on Drop; normal
  exits disarm it.
- Protocol stdout is one bounded JSON envelope per line; duplicate completion
  is rejected. Stderr remains logs and its 20-line tail explains failures.
- The CPAL callback only converts samples and `try_send`s bounded chunks.
  Queue saturation is `AUDIO_OVERRUN`; channel loss is `WAV_WRITER_FAILED`.
- The writer thread owns Hound, periodic flushing, and the RIFF 4 GiB guard.
- Record to `.wav.part`; finalize, drain failure state, validate duration, then
  atomically rename. Every failure path removes the partial.
- First failure wins: store and emit one `RecordingFailure`, ignore later ones.
- Offline mode requires all expected model directories and no token.
- Canonicalize worker paths, output roots, artifacts, and cache sources before
  prefix/containment checks.
- Use `spawn_blocking` for synchronous CPAL and filesystem work.

## ANTI-PATTERNS

- Do not signal a bare PID or call `child.kill`; use the process-group guard.
- Do not inherit the parent environment or add ad hoc variables at a spawn.
- Do not parse stderr as protocol or permit lines above `MAX_LINE_BYTES`.
- Do not allocate, lock, flush, or block in the audio callback.
- Do not replace `try_send` with a blocking send.
- Do not publish `.wav` before final rename or omit partial-file cleanup.
- Do not trust paths from the worker, model cache, or picker without containment.
- Keep process, recording, path, and model-cache regressions in their existing
  sibling/inline test modules.
