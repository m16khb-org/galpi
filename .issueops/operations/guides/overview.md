---
name: overview
description: Family module overview: installation and runtime operation.
---

# Operations — Overview

Canonical index: [OPERATIONS.md](../../OPERATIONS.md)

## Prerequisites (README 빠른 시작)

- macOS 14+ (Apple Silicon) or Windows 10/11 x64; Rust 1.88+; Bun 1.3+.
- Windows: Microsoft Visual C++ 2015–2022 x64 redistributable (not bundled).
- Tauri CLI: `cargo install tauri-cli --version 2.11.4 --locked`.
- `uv`/`uvx` on PATH for Python verification gates (`brew install uv` on macOS).

## Local development

```bash
bun install
bun run dev        # cargo tauri dev — stages verified uv for the host, Python
                   # worker, frontend, and Tauri app, then runs
bun run vite:dev   # frontend only: vite --port 1420 --strictPort
bun run sidecar:stage  # stage sidecars without running the app
```

- No global Python, ffmpeg, or WhisperX install is required; `bun run dev`
  stages an app-managed Python 3.12 environment. First engine setup may
  download GB-scale models into the app data folder; later runs reuse it.
- Dev/build staging may download the pinned `uv` archive for the host target first.

## Environment and secrets

- No required env vars for the app itself. User-level settings live in the
  app settings UI, never in docs/logs: Hugging Face fine-grained read-only
  token (only for first `pyannote/speaker-diarization-community-1` download),
  OpenAI-compatible endpoint + key for meeting-minutes refinement.
- Secret storage differs per OS: Windows keeps the Hugging Face token and
  assistant API key in Credential Manager (`com.m16khb.galpi:hugging-face-token`,
  `com.m16khb.galpi:assistant-api-key`); macOS keeps them in the `0600`
  settings file.
- Do not put raw tokens (`hf_...`, API keys) in docs, test fixtures, or logs.

## Build and release

```bash
bun run build      # cargo tauri build --bundles app --ci + DMG script
```

On macOS, outputs `src-tauri/target/release/bundle/macos/Galpi.app` and
`bundle/dmg/Galpi_0.1.0_aarch64.dmg` via `hdiutil`; signing and notarization
require an Apple Developer certificate and are done separately. On Windows,
outputs `src-tauri/target/release/bundle/nsis/*.exe` (unsigned NSIS installer;
SmartScreen shows a warning). `scripts/build.ts` picks the bundle commands per
platform. Set `GALPI_SIDECAR_TARGET` to a triple to stage a specific sidecar.

| Target triple | `uv` archive | Staged name |
|---|---|---|
| `aarch64-apple-darwin` | `uv-aarch64-apple-darwin.tar.gz` | `uv-aarch64-apple-darwin` |
| `x86_64-pc-windows-msvc` | `uv-x86_64-pc-windows-msvc.zip` | `uv-x86_64-pc-windows-msvc.exe` |

The table mirrors `scripts/sidecar-targets.ts` (SHA-256 pinned). Other
platforms/architectures are rejected. Artifact version follows `tauri.conf.json`.

## Generated trees (not source; never edit)

`node_modules`, `dist`, `src-tauri/target`, `src-tauri/resources/worker`,
`src-tauri/binaries`.

## Smoke checks

- Quick: `bun run check && bun test`.
- Packaging smoke: `bun run build` then open the produced `.app`.
