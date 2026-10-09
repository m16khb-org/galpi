---
name: overview
description: Family module overview: installation and runtime operation.
---

# Operations — Overview

Canonical index: [OPERATIONS.md](../../OPERATIONS.md)

## Prerequisites (README 빠른 시작)

- macOS 14+ on Apple Silicon; Rust 1.85+; Bun 1.3+.
- Tauri CLI: `cargo install tauri-cli --version 2.11.4 --locked`.
- `uv`/`uvx` on PATH for Python verification gates (`brew install uv`).

## Local development

```bash
bun install
bun run dev        # cargo tauri dev — stages verified arm64 uv, Python
                   # worker, frontend, and Tauri app, then runs
bun run vite:dev   # frontend only: vite --port 1420 --strictPort
bun run sidecar:stage  # stage sidecars without running the app
```

- No global Python, ffmpeg, or WhisperX install is required; `bun run dev`
  stages an app-managed Python 3.12 environment. First engine setup may
  download GB-scale models into the app data folder; later runs reuse it.
- Dev/build staging may download the pinned ARM64 `uv` archive first.

## Environment and secrets

- No required env vars for the app itself (the worker receives
  `GALPI_ASSISTANT_*` variables from the host). User-level settings live in the
  app settings UI, never in docs/logs: Hugging Face fine-grained read-only
  token (only for first `pyannote/speaker-diarization-community-1` download),
  OpenAI-compatible endpoint + key for meeting-minutes refinement.
- Do not put raw tokens (`hf_...`, API keys) in docs, test fixtures, or logs.

## ChatGPT sign-in (issue #4)

- Sign in: 설정 → AI 증강 → "ChatGPT" → "ChatGPT로 계속하기". The host opens the
  system browser at `https://auth.openai.com/api/accounts/authorize` and waits up to
  5 minutes on `http://127.0.0.1:1455/auth/callback` (an OS-assigned port when 1455
  is taken). Cancel from the sheet; consent denial shows a Korean message and the
  app does not retry.
- Storage: the issued client id, account email/subject, and the non-secret
  `chatgpt*` flags live in `~/Library/Application Support/com.m16khb.galpi/settings.json`;
  the token record (`chatgpt-tokens`) goes through the current `SecretStore`, which
  is the same 0600 settings file until Developer ID signing re-enables Keychain.
  `chatgptHostId` survives sign-out.
- Sign out: "ChatGPT 로그아웃" tries server-side revocation, then always deletes the
  tokens, client id, email, subject, and model, and returns to API-key mode. Check
  without printing values:
  `python3 -c 'import json,os; d=json.load(open(os.path.expanduser("~/Library/Application Support/com.m16khb.galpi/settings.json"))); print(sum(1 for k in ("chatgptClientId","chatgptTokens","chatgptEmail","chatgptSubject") if d.get(k)), bool(d.get("chatgptHostId")))'`
  → `0 True`.
- Worker contract: the host sets `GALPI_ASSISTANT_TRANSPORT=responses` and passes the
  short-lived access token as `GALPI_ASSISTANT_API_KEY` (in `assistant_environment`
  only). The refresh token never leaves the host. Unset transport keeps the Chat
  Completions path.
- Manual refresh check: sign in, quit, move the system clock forward more than one
  hour (automatic time off), relaunch and refine, then restore the clock. Compare
  `shasum` of `settings.json` before and after instead of reading token values.
- Usage limit: ChatGPT plan limits are shared with other apps; the worker reports
  `CHATGPT_USAGE_LIMIT_EXCEEDED` and the sheet links to
  `https://chatgpt.com/settings/usage`.

## Build and release

```bash
bun run build      # cargo tauri build --bundles app --ci + DMG script
```

Outputs `src-tauri/target/release/bundle/macos/Galpi.app` and
`bundle/dmg/Galpi_0.1.0_aarch64.dmg` via macOS `hdiutil`. Signing and
notarization require an Apple Developer certificate and are done separately.
Build scripts currently hardcode ARM64 and artifact version 0.1.0.

## Generated trees (not source; never edit)

`node_modules`, `dist`, `src-tauri/target`, `src-tauri/resources/worker`,
`src-tauri/binaries`.

## Smoke checks

- Quick: `bun run check && bun test`.
- Packaging smoke: `bun run build` then open the produced `.app`.
