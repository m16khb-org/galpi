<p align="center">
  <img src="assets/app-icon.svg" width="96" alt="Galpi app icon" />
</p>

<h1 align="center">Galpi · 갈피</h1>

<p align="center">
  Mac(Apple Silicon)과 Windows x64에서 회의를 녹음하고, 화자를 구분해 전사하고,<br />
  필요한 경우 AI로 회의록까지 정리하는 로컬 우선 데스크톱 앱
</p>

<p align="center">
  <a href="README.md"><strong>한국어</strong></a>
  ·
  <a href="README.en.md">English</a>
</p>

> [!IMPORTANT]
> Galpi 0.1.0은 **macOS 14 이상 Apple Silicon(M1 이상)** 과 **Windows 10/11 x64** 를 지원하는 개발 빌드입니다.
> macOS DMG는 서명·공증되지 않았고 Windows 설치 프로그램은 서명되지 않았습니다. Intel Mac과 Linux는 지원하지 않습니다.

## 한눈에 보기

Galpi는 회의 음성을 앱 안에서 녹음하거나 기존 파일로 가져와, 한국어 전사와 화자분리를 로컬에서 수행합니다. 전사 엔진은 설정의 `전사 엔진`에서 선택하며 기본값은 `Qwen3`(Qwen3-ASR-1.7B + Qwen3-ForcedAligner-0.6B)이고 이전 엔진 `WhisperX`(faster-whisper large-v3-turbo)를 그대로 고를 수 있습니다. 화자분리는 두 프리셋이 pyannote community-1을 공용합니다. 전사 결과는 사용자가 고른 폴더에 저장되며, 선택적으로 OpenAI 호환 API를 사용해 한국어 회의록 Markdown을 만들 수 있습니다.

| 기능 | 내용 |
|---|---|
| 바로 녹음 | 마이크 입력(CPAL: CoreAudio/WASAPI)을 16-bit PCM WAV로 저장 |
| 파일 가져오기 | `m4a`, `mp3`, `wav`, `mp4`, `mov`, `aac`, `flac`, `ogg` |
| 로컬 전사 | `Qwen3`(기본) 또는 `WhisperX` 프리셋 한국어 ASR |
| 화자분리 | pyannote 기반 분리, 자동·정확히·최소/최대 화자 수 힌트 |
| 문장 정렬 | 한국어 문장 정렬과 장기 무음 환각 필터링 |
| 참석자 명부 | 이름·팀·역할·별칭·설명을 회의마다 재사용 |
| 단어집 | 고유명사와 전문 용어를 회의록 가공에 반영 |
| AI 회의록 | OpenAI 호환 API로 결정·담당·기한 중심 Markdown 생성 |
| 작업 제어 | 준비·전사 취소와 상세 로그, AI 증강 진행률·오류 표시 |

## 빠른 시작

### 1. 개발 도구 준비

필수 환경:

- macOS 14 이상 Apple Silicon 또는 Windows 10/11 x64
- Windows: Microsoft Visual C++ 2015–2022 x64 재배포 패키지(앱에 포함되지 않음)
- Rust 1.88 이상
- Bun 1.3 이상
- Tauri CLI 2.11.4

```bash
cargo install tauri-cli --version 2.11.4 --locked
bun install
bun run dev
```

`bun run dev`는 해당 플랫폼의 검증된 `uv`, Python worker, 프론트엔드, Tauri 앱을 준비해 실행합니다. Python, ffmpeg, WhisperX를 전역으로 미리 설치할 필요는 없습니다. Windows에서도 PowerShell에서 같은 명령을 사용합니다.

### Windows 빠른 시작

1. [Microsoft Visual C++ 2015–2022 x64 재배포 패키지](https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist)를 설치합니다. 앱에 포함되어 있지 않습니다.
2. 서명되지 않은 NSIS 설치 프로그램(`.exe`)을 실행합니다. SmartScreen 경고가 나오면 **추가 정보 → 실행**을 누릅니다.
3. 아래 '최초 엔진 준비'를 진행합니다.

Windows에서는 전사 엔진으로 `WhisperX`만 선택할 수 있습니다(Qwen3/MLX는 Apple Silicon 전용). 기본은 CPU이며, 설정에서 CUDA를 고를 수 있습니다. CUDA는 PyTorch cu128 빌드를 추가로 약 3.5 GB 내려받고 NVIDIA 드라이버가 필요합니다. ASR 자체는 CTranslate2 CPU int8로 동작하며 CUDA는 정렬·화자분리만 가속합니다.

### 2. 최초 엔진 준비

1. 앱 우측 상단의 **설정**을 엽니다.
2. 새 Mac에서 화자분리 모델을 처음 받는다면 Hugging Face 토큰을 저장합니다.
3. **로컬 엔진 준비**를 누릅니다.
4. 선택한 엔진의 엔진, 전사 모델, ffmpeg가 모두 `준비됨`이 될 때까지 기다립니다. `전사 엔진` 선택지 아래에는 각 프리셋의 준비 상태가 함께 표시됩니다.

최초 준비에서는 앱 전용 Python 3.12 환경과 수 GB의 모델을 내려받을 수 있습니다. 이후에는 같은 앱 데이터 폴더와 모델 캐시를 재사용합니다. 두 프리셋은 서로 다른 가상환경(`engine/`, `engine/qwen3/`)에 설치되므로 한쪽을 준비해도 다른쪽에 영향을 주지 않습니다.

### 3. Hugging Face 토큰

화자분리 모델을 처음 내려받을 때만 필요합니다.

1. [`pyannote/speaker-diarization-community-1`](https://huggingface.co/pyannote/speaker-diarization-community-1)의 이용 조건에 동의합니다.
2. [Hugging Face Access Tokens](https://huggingface.co/settings/tokens)에서 **Fine-grained** 토큰을 만듭니다.
3. 해당 저장소에 **Read** 권한만 허용합니다.
4. `hf_`로 시작하는 값을 Galpi 설정에 저장합니다.

쓰기 권한과 Inference Providers 권한은 필요하지 않습니다.

## 사용 흐름

### 회의 녹음 또는 파일 선택

**앱에서 녹음**

1. 출력 폴더를 확인합니다.
2. **마이크로 바로 녹음**을 누르고 마이크 권한을 허용합니다. Windows에서는 설정 → 개인 정보 → 마이크에서 마이크 접근과 '데스크톱 앱이 마이크에 액세스하도록 허용'이 켜져 있어야 합니다.
3. 회의가 끝나면 **정지**를 누릅니다.
4. 완성된 WAV가 자동으로 전사 입력에 선택됩니다.

녹음은 bounded queue와 전용 WAV writer를 사용해 점진적으로 저장합니다. **버리기**를 선택하면 부분 파일을 제거합니다.

> [!NOTE]
> 현재는 선택한 Mac 마이크 입력만 녹음합니다. Zoom, Meet 등의 시스템 오디오는 직접 캡처하지 않습니다.

**기존 녹음 가져오기**

1. **오디오 파일 선택**에서 회의 파일을 고릅니다.
2. 결과를 저장할 출력 폴더를 확인합니다.
3. 화자 수를 모르면 `자동`, 정확히 알면 `정확히`, 범위만 알면 `범위`를 고릅니다.
4. **전사 시작**을 누릅니다.

### 참석자와 단어집

설정에서 다음 정보를 저장해 여러 회의에 재사용할 수 있습니다.

- 참석자: 이름, 팀, 역할, 별칭, 설명
- 단어집: 용어와 선택적 설명
- 회의 배경: 목적, 맥락, 원하는 정리 방식

이 정보는 화자 이름과 용어 표기를 안정적으로 정리하는 데 사용됩니다.

### AI 회의록 만들기

전사가 끝난 뒤 `AI 증강 실행`을 누르면 OpenAI 호환 API로 Markdown 회의록을 생성합니다. 새 녹음·전사 없이 이미 가지고 있는 전사문도 **03 전사 결과 AI 증강** 패널의 `전사문 파일 가져오기`로 바로 증강할 수 있습니다(txt·md).

- 기본 모델: `glm-5.3-flash`
- 기본 API: `https://api.z.ai/api/coding/paas/v4`
- 모델, Base URL, 추론 강도는 설정에서 변경 가능

준비 순서:

1. `설정`의 **AI 증강** 섹션에 사용할 서비스의 API Key를 입력합니다. 저장한 키는 마스킹되어 다시 표시되지 않습니다. 키를 바꾸려면 `AI 증강 API 키 지우기`로 지운 뒤 새 키를 입력하세요.
2. z.ai가 아니라면 제공자의 모델 이름과 OpenAI 호환 Base URL을 입력합니다.
3. 필요한 참석자를 선택하고 단어집·사전 정보를 확인합니다.
4. 전사를 완료한 뒤 `AI 증강 실행`을 누릅니다.

### ChatGPT로 로그인해 사용하기

API Key 대신 ChatGPT 계정으로 로그인해 회의록 정제 모델을 사용할 수 있습니다.

1. `설정` → **AI 증강**에서 **ChatGPT**를 선택하고 `ChatGPT로 계속하기`를 누릅니다.
2. 시스템 브라우저가 열리면 로그인 후 접근을 승인합니다. 승인이 끝나면 앱으로 돌아옵니다.
3. 모델은 로그인한 계정의 `/v1/models` 목록에서 고릅니다.

- 사용량 한도는 ChatGPT 요금제와 공유되며, 한도에 걸려도 앱은 요청을 자동으로 다시 보내지 않습니다.
- ChatGPT 대화 내용과 계정 정보에는 접근하지 않습니다.
- 로그아웃하면 서버에 토큰 폐기를 시도하고 로컬 토큰을 삭제합니다. ChatGPT 설정의 연결된 앱에서도 해제할 수 있습니다.
- 토큰은 현재 `0600` 권한의 `settings.json`에 저장됩니다(아래 "로컬 데이터와 개인정보" 참고). refresh token은 30일 유효하며 갱신 때마다 회전됩니다.

> [!WARNING]
> 음성 녹음과 전사(Qwen3·WhisperX 모두)는 로컬에서 처리됩니다. `AI 증강 실행`을 누르면 전사문, 이번 회의에서 선택한 참석자, 단어집, 사전 정보가 설정한 외부 API(ChatGPT 로그인 사용 시 OpenAI API)로 전송됩니다. 민감한 회의에서는 사용 중인 API 제공자의 보안·보존 정책을 먼저 확인하세요.

## 산출물

기본 저장 위치는 `~/Documents/Galpi`입니다(출력 폴더에서 변경할 수 있습니다). 회의 하나에 폴더 하나가 대응됩니다. 마이크 녹음은 시작 시각으로 폴더를 만들고(`YYYY-MM-DD HHMMSS 녹음`), 전사 결과는 오디오 파일 이름과 같은 폴더에 저장됩니다. 폴더 안의 모든 파일은 폴더와 같은 이름을 공유합니다.

```text
~/Documents/Galpi/
├── 2026-08-24 143052 녹음/     # 마이크 녹음 (시작 시각으로 자동 이름)
│   ├── 2026-08-24 143052 녹음.wav
│   ├── 2026-08-24 143052 녹음.srt
│   ├── 2026-08-24 143052 녹음_화자별.txt
│   ├── 2026-08-24 143052 녹음.aligned.v2.json
│   └── 2026-08-24 143052 녹음_회의록.md   # AI 증강 실행 시
└── 팀미팅/                      # 가져온 오디오·전사문 (원본 이름 그대로)
    ├── 팀미팅.srt
    ├── 팀미팅_화자별.txt
    ├── 팀미팅.aligned.v2.json
    └── 팀미팅_회의록.md
```

같은 이름의 회의가 이미 있으면 `팀미팅 2`처럼 번호를 붙입니다. 정렬 체크포인트(`.aligned.v2.json`)는 **WhisperX 프리셋에서만** 만들어지며, 있으면 같은 오디오를 다시 전사할 때 전사·정렬 단계를 건너뜁니다. Qwen3 프리셋은 srt·txt만 만들고 체크포인트를 남기지 않습니다.

| 파일 | 용도 |
|---|---|
| `.srt` | 자막과 타임코드 |
| `_화자별.txt` | 화자 단위 읽기 쉬운 전사문 |
| `.aligned.v2.json` | 문장 정렬 체크포인트와 재처리 기반(WhisperX 전용) |
| `_회의록.md` | 결정, 담당, 기한, 논의 내용을 정리한 문서 |

완료 화면에서 파일을 열거나 Finder에서 출력 폴더를 확인할 수 있습니다.

## 로컬 데이터와 개인정보

- 음성과 전사 산출물은 사용자가 선택한 로컬 폴더에 저장됩니다.
- 전사·정렬·화자분리 모델(Qwen3, WhisperX, pyannote)은 Galpi의 앱 전용 Hugging Face 캐시에 저장됩니다.
- Hugging Face 토큰, AI 증강 API 키, ChatGPT 로그인 토큰은 플랫폼에 따라 다르게 저장됩니다. Windows에서는 Windows Credential Manager의 `com.m16khb.galpi:hugging-face-token`, `com.m16khb.galpi:assistant-api-key`, `com.m16khb.galpi:chatgpt-tokens` 항목에 저장되며, 긴 값은 여러 항목에 나누어 저장됩니다. macOS에서는 Application Support 아래 설정 파일(`settings.json`)에 `0600` 권한으로 저장됩니다. ChatGPT 토큰(access·refresh)도 이 파일에 평문으로 들어가므로, 파일이 유출되면 계정의 API 접근이 노출될 수 있습니다.
- macOS에서는 자격 증명을 아직 Keychain으로 암호화하지 않습니다. Keychain은 항목 접근 권한을 앱의 코드 서명에 묶는데, 현재 빌드는 ad-hoc 서명이라 빌드마다 서명이 달라져 사용자가 업데이트할 때마다 접근 허용 창을 다시 보게 됩니다. macOS의 Keychain 전환은 Developer ID 서명과 함께 적용할 예정입니다.
- AI 회의록을 실행하지 않으면 전사문은 외부 LLM API로 전송되지 않습니다.
- worker는 고정된 프로그램과 argv로 실행되며 셸 문자열을 실행하지 않습니다.

## 아키텍처

```text
TypeScript UI
    │ Tauri IPC + validated events
    ▼
Rust application
    │ ports
    ├── 마이크 녹음기 (CPAL: CoreAudio/WASAPI)
    ├── filesystem / opener
    └── supervised Python worker
            │ versioned JSONL
            ▼
       Qwen3·WhisperX / pyannote / assistant
```

| 경로 | 역할 |
|---|---|
| `src/` | 도메인 계약(작업·화자·백엔드 포트)과 검증, 상태 머신, Zod 경계, DOM UI |
| `src-tauri/` | Tauri 명령, Rust use case·도메인 값 객체, 녹음·프로세스 어댑터 |
| `worker/` | Qwen3·WhisperX 프리셋 전사, 정렬, 화자분리, 회의록 가공 |
| `scripts/` | 아키텍처 검사, sidecar staging, DMG 패키징 |
| `DESIGN.md` | UI, 접근성, 컴포넌트 상태의 규범 |
| `docs/ARCHITECTURE.md` | 레이어 구조와 포트 소유권의 규범 |
| `docs/ROADMAP.md` | 현재 상태와 다음 제품 단계 |

## 개발 명령

### 빠른 검증

```bash
bun run check
bun test
```

### 전체 검증

Python 검증에는 `uv`/`uvx`가 PATH에 있어야 합니다. Homebrew를 사용한다면 `brew install uv`로 설치할 수 있습니다. `<WhisperX Python 경로>`에는 WhisperX가 설치된 Python 실행 파일의 절대 경로를 넣습니다.

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
uvx ruff check worker
uvx ruff format --check worker
uvx basedpyright --pythonpath <WhisperX Python 경로>
PYTHONPATH=. python3 -m unittest discover -s worker/tests -t . -v   # macOS
# Windows PowerShell
$env:PYTHONPATH='.'; python3 -m unittest discover -s worker/tests -t . -v
```

### 프로덕션 빌드

```bash
bun run build
```

결과:

```text
src-tauri/target/release/bundle/macos/Galpi.app
src-tauri/target/release/bundle/dmg/Galpi_0.1.0_aarch64.dmg
```

macOS에서는 위 두 파일을, Windows x64에서는 NSIS 설치 프로그램을 만듭니다.

```text
src-tauri/target/release/bundle/nsis/*.exe
```

macOS 빌드는 `.app`을 만든 뒤 `hdiutil`로 DMG를 생성합니다. Windows 빌드는 서명되지 않은 NSIS `.exe`를 만듭니다. CI는 두 플랫폼을 모두 검증합니다.

#### 다른 사람에게 배포할 때

서명·공증되지 않은 DMG를 받은 Mac은 Gatekeeper가 실행을 막습니다. 배포 전에 Apple Developer 인증서로 서명하고 공증하세요.

서명이 붙으면 자격 증명도 Keychain으로 옮길 수 있습니다. macOS는 Keychain 항목의 접근 권한을 앱의 코드 서명에 묶어 두므로, 서명이 고정되기 전에는 사용자가 업데이트할 때마다 접근 허용 창을 보게 됩니다.

`.github/workflows/release.yml`이 `v*` 태그에서 DMG를 만들고, 아래 저장소 시크릿이 설정되어 있으면 서명·공증까지 수행한 뒤 `codesign`/`spctl`로 검증합니다.

| 시크릿 | 내용 |
|---|---|
| `APPLE_CERTIFICATE` | Developer ID Application 인증서(`.p12`)의 base64 |
| `APPLE_CERTIFICATE_PASSWORD` | 해당 `.p12`의 비밀번호 |
| `APPLE_SIGNING_IDENTITY` | 예: `Developer ID Application: 이름 (TEAMID)` |
| `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` | 공증용 Apple ID와 앱 암호, 팀 ID |

시크릿이 없으면 워크플로는 기존처럼 ad-hoc 서명 DMG를 만들며, 이는 내부 테스트용입니다.

Hardened Runtime에 필요한 entitlement는 `src-tauri/Entitlements.plist`에 이미 선언되어 있습니다. Galpi는 첫 실행에서 자체 Python 환경을 설치하고 그 안의 PyTorch·MLX를 불러오므로 라이브러리 검증 비활성화와 실행 가능 메모리 허용이 필요합니다. 아직 서명된 빌드를 만들어 검증한 적은 없으므로, 첫 공증 시 이 부분을 반드시 확인하세요.

## 문제 해결

| 증상 | 확인할 내용 |
|---|---|
| `cargo tauri` 명령을 찾을 수 없음 | `cargo install tauri-cli --version 2.11.4 --locked` |
| 로컬 엔진 준비가 중간에 실패함 | 같은 버튼을 다시 누르면 이어서 재시도합니다(실패한 부분 설치는 자동으로 정리됩니다). 네트워크 상태도 확인하세요 |
| Qwen3 준비 시 내려받을 파일이 큼 | Qwen3 프리셋은 전사·정렬 모델 합계 약 6.6GB를 내려받습니다 |
| 모델 다운로드가 401/403으로 실패 | 모델 이용 조건 동의와 Fine-grained Read 토큰 확인 |
| 마이크 녹음이 시작되지 않음 | 시스템 설정에서 Galpi 마이크 권한 확인 |
| 상대방 음성이 녹음되지 않음 | 현재 시스템 오디오 캡처는 지원하지 않음 |
| AI 회의록이 실패함 | API Key, Base URL, 모델 이름, 제공자 사용량 한도 확인 |
| 다른 Mac에서 앱을 열 수 없음 | 미서명·미공증 빌드는 Gatekeeper가 막습니다. 서명·공증해 배포하세요(위 "다른 사람에게 배포할 때") |
| 앱 업데이트 후 엔진이 다시 `대기`로 표시됨 | 엔진 준비 마커가 의존성 잠금 파일의 해시를 따릅니다. 잠금이 바뀌면 **로컬 엔진 준비**를 한 번 더 눌러 환경을 맞춥니다(모델은 다시 받지 않습니다) |

## 현재 상태

Galpi는 활발히 개발 중인 `0.1.0` 프로젝트입니다. 배포 자동 업데이트, 서명·공증, 시스템 오디오 캡처, 회의 라이브러리는 이후 단계로 계획되어 있습니다. 자세한 계획은 [`docs/ROADMAP.md`](docs/ROADMAP.md)를 참고하세요.
