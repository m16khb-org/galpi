<p align="center">
  <img src="assets/app-icon.svg" width="96" alt="Galpi app icon" />
</p>

<h1 align="center">Galpi · 갈피</h1>

<p align="center">
  A local-first desktop app that records meetings on Apple Silicon Macs and Windows x64,<br />
  separates speakers, transcribes Korean audio, and optionally produces AI meeting minutes
</p>

<p align="center">
  <a href="README.md">한국어</a>
  ·
  <a href="README.en.md"><strong>English</strong></a>
</p>

> [!IMPORTANT]
> Galpi 0.1.0 is a development build for **Apple Silicon Macs running macOS 14 or later** and **Windows 10/11 x64**.
> The macOS DMG is unsigned and not notarized, and the Windows installer is unsigned. Intel Macs and Linux are not supported.

## At a glance

Galpi records audio inside the app or imports an existing meeting file, then runs Korean transcription and speaker diarization locally. The transcription engine is chosen in settings: `Qwen3` (Qwen3-ASR-1.7B + Qwen3-ForcedAligner-0.6B) is the default, and the previous `WhisperX` stack (faster-whisper large-v3-turbo) remains selectable. Both presets share pyannote community-1 for diarization. Results are stored in a folder you choose. Optionally, Galpi can send the transcript to an OpenAI-compatible API and generate structured Korean meeting minutes in Markdown.

| Capability | What it does |
|---|---|
| Direct recording | Records microphone input (CPAL: CoreAudio/WASAPI) to a 16-bit PCM WAV |
| File import | `m4a`, `mp3`, `wav`, `mp4`, `mov`, `aac`, `flac`, `ogg` |
| Local transcription | Korean ASR on the `Qwen3` (default) or `WhisperX` preset |
| Speaker diarization | pyannote diarization with automatic, exact, or min/max speaker-count hints |
| Alignment | Korean sentence alignment and long-silence hallucination filtering |
| Participant roster | Reusable names, teams, roles, aliases, and descriptions |
| Glossary | Reusable proper nouns and domain terminology |
| AI minutes | Decision-, owner-, and deadline-oriented Markdown via an OpenAI-compatible API |
| Job control | Cancel setup or transcription with detailed logs; monitor AI refinement progress and errors |

## Quick start

### 1. Install development tools

Requirements:

- macOS 14 or later on Apple Silicon, or Windows 10/11 x64
- Windows: Microsoft Visual C++ 2015–2022 x64 redistributable (not bundled)
- Rust 1.88 or later
- Bun 1.3 or later
- Tauri CLI 2.11.4

```bash
cargo install tauri-cli --version 2.11.4 --locked
bun install
bun run dev
```

`bun run dev` stages the verified `uv` binary for the current platform, the Python worker, the frontend, and the Tauri app. You do not need to preinstall Python, ffmpeg, or WhisperX globally. Use the same command in Windows PowerShell.

### Windows quick start

1. Install the [Microsoft Visual C++ 2015–2022 x64 redistributable](https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist). The app does not bundle it.
2. Run the unsigned NSIS installer (`.exe`). If SmartScreen warns, choose **More info → Run anyway**.
3. Follow "2. Prepare the local engine" below.

On Windows the only transcription engine is `WhisperX` (Qwen3/MLX requires Apple Silicon). It runs on CPU by default; CUDA is optional in settings. CUDA downloads an additional PyTorch cu128 build of about 3.5 GB and requires an NVIDIA driver. ASR itself stays on CTranslate2 CPU int8; CUDA accelerates only alignment and diarization.

### 2. Prepare the local engine

1. Open `설정` (Settings) from the top-right corner.
2. On a new Mac, save a Hugging Face token if the diarization model requires access.
3. Select `로컬 엔진 준비` (Prepare local engine).
4. Wait until WhisperX, the transcription model, and ffmpeg all show `Ready`.

The first setup installs an app-specific Python 3.12 environment and may download several gigabytes of models. Later runs reuse the same app data directory and model cache.

### 3. Hugging Face token

The token is only required when the diarization model is downloaded for the first time.

1. Accept the terms for [`pyannote/speaker-diarization-community-1`](https://huggingface.co/pyannote/speaker-diarization-community-1).
2. Create a **Fine-grained** token in [Hugging Face Access Tokens](https://huggingface.co/settings/tokens).
3. Grant **Read** access only to that repository.
4. Save the `hf_...` value in Galpi Settings.

Write access and Inference Providers access are not required.

## Workflow

### Record a meeting or choose a file

**Record inside Galpi**

1. Confirm the output folder.
2. Select `마이크로 바로 녹음` (Record with microphone) and grant microphone access. On Windows, Settings → Privacy → Microphone must allow microphone access and "Let desktop apps access your microphone".
3. Select `정지` (Stop) when the meeting ends.
4. The completed WAV is selected automatically as the transcription input.

Recording uses a bounded queue and a dedicated WAV writer, so the entire meeting is not kept in memory. `버리기` (Discard) cancels the recording and removes its partial file.

> [!NOTE]
> Galpi currently captures the selected Mac microphone input only. It does not capture system audio from Zoom, Meet, or other apps.

**Import an existing recording**

1. Select `오디오 파일 선택` (Choose audio file).
2. Confirm the output folder.
3. Choose `자동` (Automatic), `정확히` (Exact), or `범위` (Min/max range) for the speaker-count hint.
4. Select `전사 시작` (Start transcription).

### Participants and glossary

Settings can store reusable context:

- Participants: name, team, role, aliases, and description
- Glossary: term and optional explanation
- Meeting background: purpose, context, and preferred output style

This context helps Galpi produce stable names and terminology in refined meeting minutes.

### Generate AI meeting minutes

After transcription, select `AI 증강 실행` (Run AI augmentation) to create Markdown through an OpenAI-compatible API. You can also augment an existing transcript without a new recording: use `전사문 파일 가져오기` (Import transcript file) in the augmentation panel (txt/md).

- Default model: `glm-5.3-flash`
- Default API: `https://api.z.ai/api/coding/paas/v4`
- Model, Base URL, and reasoning effort are configurable

Setup:

1. Open `설정` (Settings) and enter the provider API Key in the **AI 증강** (AI augmentation) section. A saved key is masked and never shown again; to replace one, clear it with `AI 증강 API 키 지우기` (Clear AI augmentation API key) and enter the new one.
2. If you are not using z.ai, enter the provider's model name and OpenAI-compatible Base URL.
3. Select the participants for this meeting and review the glossary and background context.
4. Finish transcription, then select `AI 증강 실행`.

> [!WARNING]
> Audio recording and transcription (both the Qwen3 and WhisperX presets) stay on the Mac. When you select `AI 증강 실행`, the transcript, participants selected for this meeting, glossary, and background context are sent to the configured external API. Review the provider's security and retention policy before using this feature with sensitive meetings.

## Outputs

The default location is `~/Documents/Galpi` (changeable from the output folder picker). One folder corresponds to one meeting: a microphone recording creates `YYYY-MM-DD HHMMSS 녹음` named after its start time, while imported audio and transcripts keep their original file name. Every artifact inside a meeting folder shares the folder's name.

```text
~/Documents/Galpi/
├── 2026-08-24 143052 녹음/     # microphone recording (auto-named by start time)
│   ├── 2026-08-24 143052 녹음.wav
│   ├── 2026-08-24 143052 녹음.srt
│   ├── 2026-08-24 143052 녹음_화자별.txt
│   ├── 2026-08-24 143052 녹음.aligned.v2.json
│   └── 2026-08-24 143052 녹음_회의록.md      # produced by AI augmentation
└── 팀미팅/                      # imported audio/transcript (original name)
    ├── 팀미팅.srt
    ├── 팀미팅_화자별.txt
    ├── 팀미팅.aligned.v2.json
    └── 팀미팅_회의록.md
```

Name collisions get a numeric suffix such as `팀미팅 2`. The alignment checkpoint (`.aligned.v2.json`) is produced by the **WhisperX preset only**; when it exists, re-transcribing the same audio skips the transcription and alignment stages. The Qwen3 preset publishes srt/txt and leaves no checkpoint.

| File | Purpose |
|---|---|
| `.srt` | Subtitles with timestamps |
| `_화자별.txt` | Readable speaker-oriented transcript |
| `.aligned.v2.json` | Alignment checkpoint and reprocessing input (WhisperX only) |
| `_회의록.md` | Decisions, owners, deadlines, and discussion notes (`회의록` means meeting minutes) |

The completion screen can open each artifact or reveal its output folder in Finder.

## Local data and privacy

- Audio and transcription artifacts are stored in the local folder you choose.
- Transcription, alignment, and diarization models (Qwen3, WhisperX, pyannote) are stored in Galpi's app-specific Hugging Face cache.
- The Hugging Face token and the AI augmentation API key are stored differently per platform. On Windows they live in Windows Credential Manager under `com.m16khb.galpi:hugging-face-token` and `com.m16khb.galpi:assistant-api-key`. On macOS they are stored in the Application Support settings file with `0600` permissions and are not encrypted with the Keychain: the Keychain ties access to an item to the app's code signature, and ad-hoc signing changes that signature on every build, so each update would ask users to re-authorize a token they never touched. The macOS move to the Keychain lands together with Developer ID signing.
- Transcripts are not sent to an external LLM API unless AI minutes are run.
- The worker launches fixed programs with explicit argv and does not execute shell strings.

## Architecture

```text
TypeScript UI
    │ Tauri IPC + validated events
    ▼
Rust application
    │ ports
    ├── microphone recorder (CPAL: CoreAudio/WASAPI)
    ├── filesystem / opener
    └── supervised Python worker
            │ versioned JSONL
            ▼
       Qwen3·WhisperX / pyannote / assistant
```

| Path | Responsibility |
|---|---|
| `src/` | Domain contracts (job, speaker, backend port) with validation, state machines, Zod boundaries, DOM UI |
| `src-tauri/` | Tauri commands, Rust use cases and domain value objects, recording and process adapters |
| `worker/` | Qwen3/WhisperX preset transcription, alignment, diarization, minutes refinement |
| `scripts/` | Architecture checks, sidecar staging, and DMG packaging |
| `DESIGN.md` | Normative UI, accessibility, and component-state contract |
| `docs/ARCHITECTURE.md` | Normative layering and port ownership |
| `docs/ROADMAP.md` | Current state and planned product milestones |

## Development commands

### Fast checks

```bash
bun run check
bun test
```

### Full validation

Python validation requires `uv`/`uvx` on PATH. With Homebrew, install it using `brew install uv`. Replace `<WhisperX Python path>` with the absolute path to a Python executable where WhisperX is installed.

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
uvx ruff check worker
uvx ruff format --check worker
uvx basedpyright --pythonpath <WhisperX Python path>
PYTHONPATH=. python3 -m unittest discover -s worker/tests -t . -v   # macOS
# Windows PowerShell
$env:PYTHONPATH='.'; python3 -m unittest discover -s worker/tests -t . -v
```

### Production build

```bash
bun run build
```

Outputs:

```text
src-tauri/target/release/bundle/macos/Galpi.app
src-tauri/target/release/bundle/dmg/Galpi_0.1.0_aarch64.dmg
```

On macOS this produces the two files above; on Windows x64 it produces an NSIS installer:

```text
src-tauri/target/release/bundle/nsis/*.exe
```

The macOS build creates the `.app` first, then packages the DMG with `hdiutil`. The Windows build produces an unsigned NSIS `.exe`. CI verifies both platforms.

#### Distributing to other people

Gatekeeper blocks an unsigned, un-notarized DMG on every Mac that receives it. Sign and notarize with an Apple Developer certificate before handing the build to anyone.

A signature also unlocks Keychain storage for credentials. macOS ties access to a Keychain item to the app's code signature, so until that signature is stable every update asks users to re-authorize a token they never touched.

`.github/workflows/release.yml` builds the DMG on a `v*` tag and, when the repository secrets below are set, signs and notarizes it, then verifies the result with `codesign` and `spctl`.

| Secret | Contents |
|---|---|
| `APPLE_CERTIFICATE` | base64 of the Developer ID Application certificate (`.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | Password for that `.p12` |
| `APPLE_SIGNING_IDENTITY` | For example `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` | Apple ID, app-specific password, and team id for notarization |

Without the secrets the workflow still produces an ad-hoc signed DMG, which is for internal testing only.

The entitlements Hardened Runtime needs are declared in `src-tauri/Entitlements.plist`. Galpi installs its own Python environment on first run and loads PyTorch and MLX from it, so library validation must be disabled and executable memory allowed. No signed build has been produced yet, so verify this during the first notarization.

## Troubleshooting

| Symptom | What to check |
|---|---|
| `cargo tauri` is not found | Run `cargo install tauri-cli --version 2.11.4 --locked` |
| Local engine preparation fails midway | Press the same button again to retry (a partial install is cleared automatically); also check the network |
| The Qwen3 preset download is large | The Qwen3 preset downloads about 6.6 GB of ASR and aligner models in total |
| Model download returns 401/403 | Accept the model terms and verify the Fine-grained Read token |
| Microphone recording does not start | Check Galpi microphone access in System Settings |
| Other participants are not recorded | System-audio capture is not supported yet |
| AI minutes fail | Check API Key, Base URL, model name, and provider quota |
| The app will not open on another Mac | Gatekeeper blocks unsigned, un-notarized builds. Sign and notarize before distributing (see "Distributing to other people") |
| The engine shows `Pending` again after an update | The readiness marker tracks the hash of the dependency lock file. When the lock changes, press **Prepare local engine** once to match the environment (models are not downloaded again) |

## Project status

Galpi is under active development at version `0.1.0`. Automatic updates, signing and notarization, system-audio capture, and a meeting library are planned for later milestones. See [`docs/ROADMAP.md`](docs/ROADMAP.md) for details.
