---
name: TECH_STACK.md
description: Chosen languages, runtimes, tools, and rationale.
---

# Tech Stack

Three runtimes in one desktop app, connected by Tauri IPC and a versioned JSONL
worker protocol. All facts below are confirmed from config files; confidence is
high unless noted.

## TypeScript frontend (WebView)

- Bun 1.3+ (`packageManager: bun@1.3.14`, `engines.bun >= 1.3.0`) — scripts run
  through `bun run`, not npm. `npm test` does not apply.
- TypeScript 7.0.2 (`tsc --noEmit`), Vite 8.2.1, Biome 2.5.9 (lint), Zod 4.4.3
  (IPC boundary parsing), happy-dom (DOM tests), `@tauri-apps/api` 2.11.1.
- Strict flags in `tsconfig.json`: `strict`, `exactOptionalPropertyTypes`,
  `noUncheckedIndexedAccess`, `noPropertyAccessFromIndexSignature`,
  `verbatimModuleSyntax`.

## Rust host (Tauri)

- Rust edition 2024, `rust-version = "1.88"` (`src-tauri/Cargo.toml`), pinned components
  via `rust-toolchain.toml`.
- `tauri = "2"` / `tauri-build = "2"`; Tauri CLI 2.11.4 pinned in README quick
  start (`cargo install tauri-cli --version 2.11.4 --locked`).
- cargo is the package manager; fmt/clippy/test gates run with
  `--manifest-path src-tauri/Cargo.toml`.

## Python worker (transcription sidecar)

- Two isolated presets, each with its own virtualenv and lock file:
  - Qwen3 (default): mlx-qwen3-asr 0.3.5 on mlx 0.32.1, plus torch/torchaudio
    2.8.0 and pyannote.audio 4.0.7 for diarization
    (`worker/requirements-qwen3.txt` → `requirements-qwen3.lock`).
  - WhisperX: WhisperX 3.8.6, torch/torchaudio 2.8.0, pyannote.audio 4.0.7,
    imageio-ffmpeg 0.6.0 (`worker/requirements.txt` → `requirements.lock`).
- The app installs from the lock files; the WhisperX lock is hash-pinned, while
  the Qwen3 lock pins versions only because mlx ships a wheel per macOS release.
- strict Pyright (`pyrightconfig.json`: `typeCheckingMode: strict`,
  `stubPath: worker/stubs`); local WhisperX stubs, lazy heavy imports.
- Tooling via `uv`/`uvx`: ruff (lint + format), basedpyright, unittest
  (every module under `worker/tests`). App-managed Python 3.12 environment; no global
  Python/ffmpeg/WhisperX install required (README quick start).

## Platform target

- macOS 14+ on Apple Silicon and Windows 10/11 x64; Intel macOS and Linux are
  unsupported. macOS ships as `.app` + DMG via `hdiutil`; Windows x64 ships as
  an unsigned NSIS installer (`bun run build`, `scripts/build.ts`). Signing and
  notarization are separate manual steps.
- Sidecar `uv` targets: `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`
  (`scripts/sidecar-targets.ts`).
- Worker locks per platform/device: macOS `requirements.lock` (WhisperX) and
  `requirements-qwen3.lock` (Qwen3); Windows `requirements-windows-cpu.lock`
  (default) and `requirements-windows-cuda.lock` (PyTorch cu128, optional).
  Windows has no Qwen3 preset (MLX requires Apple Silicon).
- Rust platform deps: `dunce` (all targets), `windows-sys` (`cfg(windows)`),
  `nix` (`cfg(unix)`), `security-framework` (`cfg(target_os = "macos")`).

## Confidence

- All versions above: high (read from `package.json`, `src-tauri/Cargo.toml`,
  `worker/requirements.txt`, `pyrightconfig.json`, `tsconfig.json`, README).
- Static bootstrap had detected "npm-compatible" — corrected to Bun from
  `package.json` `packageManager` field.
