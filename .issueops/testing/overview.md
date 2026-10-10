---
name: overview
description: Family module overview: test strategy and verification gates.
---

# Testing — Overview

Canonical index: [TESTING.md](../TESTING.md)

## Verification gates (actual)

Quick gate (TS only — does NOT cover Rust/Python):

```bash
bun run check   # architecture fence + Biome + tsc --noEmit
bun test        # Bun test runner, NOT npm test
```

Full gate (Rust + Python; `uv`/`uvx` must be on PATH, basedpyright needs
`--pythonpath <absolute WhisperX Python executable>`):

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
uvx ruff check worker
uvx ruff format --check worker
PYTHONPATH=. python3 -m unittest discover -s worker/tests -t . -v
```

CI (`.github/workflows/ci.yml`) runs the `rust`, `frontend`, and `worker` jobs
as a matrix on `macos-15` and `windows-latest`. Windows-only jobs:
`windows-engine` (installs `requirements-windows-cpu.lock`, imports
whisperx/torch/pyannote, runs the ignored worker-environment test, and
dry-run resolves the CUDA lock), `bundle-windows` (`bun run build`, asserts and
uploads the NSIS `.exe`), and `windows-install-smoke` (installs that artifact
and verifies the engine via `scripts/ci/windows-install-smoke.ps1`).

Manual Windows gates (not automated):

- Run the NSIS installer, prepare the engine, and transcribe a sample file.
- Record from the microphone, then cancel; confirm the recording is discarded.
- Cancel a running job and confirm in Task Manager that no worker/Python
  process remains.
- Save the Hugging Face token and assistant API key, restart the app, and
  confirm they persist (Windows Credential Manager entries
  `com.m16khb.galpi:hugging-face-token` / `:assistant-api-key`).

Benchmarks (no network, no ML stack; not part of CI or `check:all`):

```bash
python3 scripts/bench/worker_bench.py          # post-processing timing on a seeded 3-hour fixture
python3 scripts/bench/minutes_prompt_eval.py   # static rubric for the minutes prompts (0-100)
python3 scripts/bench/correction_eval.py       # correction evidence in refinement requests (0-100)
python3 scripts/bench/diarization_eval.py -v   # speaker attribution on cached real meetings (WDER %)
bash autoresearch.sh                           # worker unittest + ruff, WDER, correction score, prompt rubric
```

- `worker_bench.py` prints `postprocess_ms` plus output-shape metrics and exits
  non-zero when the pipeline output stops being well formed.
- `minutes_prompt_eval.py` scores only the prompt text (format/exemplar
  agreement, fluent-korean lint, style and fidelity rules, size penalty above
  4,400 characters). A full score does not prove minutes quality: confirm prompt
  changes by generating minutes from a real transcript and comparing them with
  the previous prompt's output.
- `correction_eval.py` builds the refinement requests (`plan_refinement`, single
  and map paths) for a seeded synthetic corpus (`correction_corpus.py`: 7 meeting
  domains × 3 noise seeds, half the misspellings generated from jamo confusions)
  and scores the `<보정후보>` block against known gold: term/name/grouping F1,
  speaker mapping (guests must stay unmapped), boundary F1, line compaction with
  the text preserved, and the usage instruction. It scores 0 when worker source
  gains corpus-specific words (`correction_guard_baseline.json`), so improve the
  detectors in `transcript_hints.py`, never the vocabulary. The corpus is
  synthetic; never commit real meeting transcripts to it.
- `diarization_eval.py` replays the step after diarization (line labels from
  pyannote turns, hallucination filter, voice relabelling in `speakers.py`) on
  cached engine output and scores word diarization error rate against gold
  speakers (labels mapped one-to-one). Inputs live outside the repo under
  `GALPI_DIARIZATION_DATA`: stitched AI Hub `회의 음성` (dataSetSn=132)
  validation meetings with gold JSON, the `.aligned.v2.json` checkpoint, cached
  turns and centroids, and per-line voices from `voice.line_voices`. The data is
  licensed; never commit it. Keep a separate held-out directory and check every
  change there too. Rebuild caches with the same model after changing
  `voice.py`, since cached voices freeze its output.

## Test structure in this repo

- Tests are colocated beside implementations: `src/**/*.test.ts` and
  `*.dom.test.ts` (happy-dom) next to sources; Rust tests in-crate
  (`application/tests.rs` FakePort); Python `worker/tests/` unittest.
- State machines (`job-machine.ts`, `recording-machine.ts`) are pure immutable
  reducers: test `(state, event) → state` without DOM or IPC.
- Fakes (`FakePort`, test backends) must preserve the production contract
  (error codes, event ordering); if a fake diverges, fix the fake (LSP rule,
  docs/ARCHITECTURE.md §4).
- Worker pure modules (`core`, `artifacts`, `minutes_*`) are tested without
  the ML stack (every module under `worker/tests`).
- Behavioral tests use Given/When/Then comments where setup is nontrivial.
- Worker stdout is machine-readable JSONL only; never emit diagnostics there.
- Style contract: `src/styles.test.ts` reads `src/styles.css` as text and fails on
  hex/`rgb()`/`hsl()` literals, non-`--seed-` custom properties, px padding/margin/
  gap/font-size/radius, and any drift between the `--seed-*` names in the
  stylesheet and root `DESIGN.md`. `src/ui/seed.test.ts` checks that every
  action-button class the helper can emit exists in the installed
  `@seed-design/css` recipe.

## Well-structured tests

- Verify observable behavior through public contracts (port methods, view
  selectors, reducer outputs), not implementation details.
- Deterministic: no wall-clock, sleeps, real network, or ordering dependence.
- One behavior per test; regression tests encode the recurring input and
  expected result.

## Poorly-structured tests

- Locking internal structure so harmless refactors fail.
- Assertions not tied to a real bug or requirement.
- Weakening production behavior (e.g., loosening Zod schemas) to pass.

## Known cautions

- DOM text checks are not pixel visibility: the error banner once passed
  textContent/hidden assertions while a grid-row collapse hid it — see
  [cautions/](../cautions/overview.md) before trusting view-level assertions.
- happy-dom ignores `@import`, so DOM tests never see SEED token values. Check
  computed values (AA overrides, button sizes, reduced motion) in a real browser
  via `bun run vite:dev` and record them; IPC-dependent flows stay Not Run there.
- `bun run build` fails on macOS 27 with deployment target 14.0 proc-macro
  dylibs — see [cautions/](../cautions/overview.md) before treating it as a
  regression.

## Rule

- `npm test` from the static draft is wrong; this repo uses `bun test`
  (package.json `packageManager: bun@1.3.14`).
- Verification commands here are confirmed from package.json scripts,
  README 개발 명령, and docs/ARCHITECTURE.md §8.
