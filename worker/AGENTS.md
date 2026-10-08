# WORKER KNOWLEDGE BASE

## OVERVIEW

Python sidecar launched and supervised by Rust. `prepare` warms dependencies and
models; `transcribe` runs Korean ASR, alignment, diarization, filtering, and
artifact publication. Stdout is a machine-readable protocol, not a log stream.
The worker's boundary is purity: `core`, `artifacts`, `minutes_*` modules stay
importable and testable without the ML stack. Repo-wide architecture rules
(DDD/hexagonal/SOLID mapping) live in `../docs/ARCHITECTURE.md`.

## STRUCTURE

```text
worker/
├── galpi_worker/
│   ├── __main__.py        # CLI dispatch, exit mapping, stdout isolation
│   ├── core.py            # Speaker validation and hallucination regex
│   ├── protocol.py        # Versioned, sequenced JSONL emitter
│   ├── runtime.py         # Device selection and warning suppression
│   ├── preparation.py     # Per-engine model warmup, ffmpeg link, manifest
│   ├── engine.py          # Transcription dispatch and WhisperX pipeline
│   ├── qwen3.py           # Qwen3 candidate pipeline (ASR + aligner + diarize)
│   ├── artifacts.py       # Typed segments, filtering, atomic writers
│   ├── minutes_template.py # Normative minutes format and compact exemplar
│   ├── minutes_prompt.py  # Typed context parsing and single-pass prompt
│   ├── minutes_pipeline.py # Long-meeting routing, chunking, map/reduce prompts
│   ├── assistant_stream.py # OpenAI-compatible SSE transport and progress
│   └── refine.py          # Short/long refinement orchestration and publication
├── stubs/whisperx/        # Local types for the untyped dependency
└── tests/                 # Pure contract tests; no ML stack required
```

## WHERE TO LOOK

| Task | Location |
|------|----------|
| Add event type/field | `protocol.py`, Rust `domain/worker.rs`, frontend event schema |
| Change device policy | `runtime.py`, retry blocks in `engine.py` |
| Change model pins/options | `engine.py` and `preparation.py` together |
| Change output names/formats | `artifacts.py` |
| Change hallucination filtering | `core.py`, `artifacts.py::filter_segments` |
| Change minutes format/exemplar | `minutes_template.py` |
| Change context/date rendering | `minutes_prompt.py` |
| Change long-meeting threshold/chunking | `minutes_pipeline.py`, `refine.py` |
| Change assistant streaming/progress | `assistant_stream.py` |
| Resolve WhisperX typing | Root `pyrightconfig.json`, `stubs/whisperx/*.pyi` |
| Change CLI flags | `__main__.py`; Rust callers in `setup.rs`, `transcription.rs` |

## CONVENTIONS

- Stdout contains only flushed `EventWriter` JSONL with protocol `v: 1`,
  monotonically increasing `seq`, and an event `type`.
- Run the pipeline under `redirect_stdout(sys.stderr)` so dependency prints
  cannot corrupt the protocol stream.
- Import Torch, WhisperX, pyannote, and imageio-ffmpeg inside runtime functions.
  Package import and pure tests must not require the ML environment.
- ASR deliberately uses CTranslate2 CPU int8 on every platform. Device policy:
  torch device is `cuda` > `mps` > `cpu` (`runtime.py::select_torch_device`);
  alignment and diarization use it, retry once on CPU if an accelerator step
  fails, and emit the fallback reason as a log event.
- Windows locks: `requirements-windows-cpu.lock` (default) and
  `requirements-windows-cuda.lock` (PyTorch cu128). Regenerate with the
  `uv pip compile ... --python-platform x86_64-pc-windows-msvc` command in each
  lock header, then run `python3 scripts/verify-windows-lock.py cpu|cuda`.
- Keep BasedPyright strict. Convert untyped library payloads with explicit
  casts to local `TypedDict` contracts; maintain realistic `.pyi` stubs.
- Publish checkpoint, SRT, speaker text, and manifests through sibling
  temporary files followed by `Path.replace` or `os.replace`.
- Exit 0 on success, 2 for `INVALID_INPUT`, and 1 for `ENGINE_ERROR`.
- Reusing `.aligned.v2.json` skips ASR/alignment only; diarization and output
  publication still run.
- Refinement stays single-pass through 48,000 characters. Longer transcripts split
  only at whole speaker-turn boundaries, map facts per chunk, then reduce once into
  the normative minutes format; all progress remains monotonic on `phase`.
- Tests use stdlib `unittest`; run the whole directory with `unittest discover`.

## ANTI-PATTERNS

- No `print`, progress bar, or logging handler on stdout outside `EventWriter`.
- Do not rename `.aligned.v2.json`, `.srt`, or `_화자별.txt` artifacts.
- Do not change a protocol version, event, or field in Python alone.
- Do not import heavy ML libraries at module scope or in pure tests.
- Do not move ASR to an accelerator; CTranslate2 CPU int8 is deliberate.
- Do not add retries beyond the single accelerator-to-CPU fallback.
- Do not write final artifacts in place.
- Do not weaken a stub merely to hide an installed-library mismatch.
