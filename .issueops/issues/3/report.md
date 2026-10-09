# 구현 보고서 — io-21fd3b1e1f06 (이슈 #3 Windows x64 지원)

상태: 완료 기록 시점의 최종본. Windows 실기 2차 확인은 머지 뒤 Windows에서 이어서 한다(아래 `## 완료 기록 결정` 참고).

## 기준선 (변경 전, main@456b500, macOS arm64, 2026-10-08)

- `bun run check:all` 통과: bun test 106 pass / 0 fail, cargo test 66 passed / 1 ignored, worker unittest 91 OK.
- `dist/` 총 321,135 B(변경 후) vs 318,882 B(변경 전), +2,253 B. `chrome120` 하한 추가와 장치 선택 UI·CSS가 원인.

## 변경 요약

| 영역 | 변경 |
|---|---|
| Rust 플랫폼 규칙 | `adapters/outbound/platform.rs`의 `Os {MacOs, Windows}`와 순수 규칙 함수. `cfg!(windows)`는 `Os::current()` 한 곳에만 있음 |
| 도메인·애플리케이션·IPC | `ComputeDevice`, `EngineSelection`, `EnvironmentStatus`에 필드 4개 추가, `save_compute_device` 커맨드(총 18개) |
| 설정 | 플랫폼별 기본 프리셋·허용 목록, `Option` 필드 + `skip_serializing_if`(롤백 호환), 비밀 저장소 주입 |
| 비밀 | `secrets/{keychain,credential,credential_manager}.rs`, Windows는 Credential Manager, macOS는 기존 설정 파일 유지 |
| 프로세스 | `process/guard/{unix,windows}.rs`: unix 프로세스 그룹, Windows Job Object(`KILL_ON_JOB_CLOSE`) |
| 전원 | `recording/power/{macos,windows}.rs`: Windows Power Request |
| 경로·환경 | `canonical()`(dunce), `AppPaths::from_roots`, `worker_environment(os, …)`, Windows 락 선택·마커 |
| 워커 | `cuda` 장치, `detect_torch_device`, `needs_cpu_fallback`, `ffmpeg_link_name`; Windows CPU/CUDA 락 2개 |
| 스크립트 | `sidecar-targets.ts`, `build.ts`, `stage-sidecars.ts`(fetch·zip), 아키텍처 펜스에 `cfg` 금지·경로 구분자 처리, 검증 헬퍼 3개 |
| 프런트엔드 | 장치 선택 UI, 사용할 수 없는 프리셋 숨김, Windows 경로 회의명, 폰트 스택, `chrome120` |
| CI | ci.yml 매트릭스(mac·Windows)와 `windows-engine`·`bundle-windows`·`windows-install-smoke`, release.yml `secrets` 문법 수정과 `nsis` job |

## 검증 증거 (macOS arm64)

- 게이트 원장 `.issueops/issues/3/gates.md`: 15개 중 14개 충족(G2, G3a, G3b, G4, G7a, G9a, G9b, G10a, G10b, G11, G12b, G14a, G15, G16). G13은 문서 단계에서 처리할 몫이라 미충족.
- G13 RED: `python3 scripts/verify-docs-platform.py` → `PLATFORM_DOCS_FAIL`(13건).
- Windows 타깃 정적 검사: `RC_x86_64_pc_windows_msvc=<가짜 llvm-rc> cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --target x86_64-pc-windows-msvc -- -D warnings`. 첫 실행은 `SettingsFile` dead_code 1건으로 실패했고, `cfg_attr(all(windows, not(test)), expect(dead_code))`로 고친 뒤 통과했다. 가짜 RC는 리소스 컴파일만 건너뛰고(링크하지 않음) 타입·lint 검사는 실제로 수행한다. 실행·링크는 Windows CI에서만 판정한다.
- RED 증거 범위:
  - 원장 게이트 G13, G15(펜스 probe), G3a·G3b(헬퍼 부재), 워커 테스트(import 실패), 프런트 테스트(10건 실패), 스크립트 테스트는 구현 전에 실패하는 것을 확인했다.
  - Rust는 단계별 RED를 따로 남기지 않았다. 테스트와 구현을 함께 쓰고 마지막에 전체를 실행했다. 대신 mac 동작 불변은 특성화 테스트(`worker_environment(MacOs)` 키·값, mac 준비 마커 바이트 동일)로 고정했다.

## side effect

- `settings.json`: `enginePreset`·`computeDevice`는 명시 저장 때만 기록한다. 롤백 테스트로 고정했다.
- Windows Credential Manager 항목 `com.m16khb.galpi:hugging-face-token`, `com.m16khb.galpi:assistant-api-key`.
- `rust-toolchain.toml` 타깃이 추가돼 rustup이 Windows 표준 라이브러리를 내려받는다.
- macOS에서 `HOME`이 없을 때의 폴백이 `/tmp`에서 `std::env::temp_dir()`로 바뀐다(계획 §3.4).
- `release.yml`이 유효해져서, 지금까지 0초 만에 실패하던 실행이 더는 생기지 않는다.

## UI 판단

- **디자인 시스템:** 장치 선택은 기존 `segmented-control`/`engine-segmented`를 재사용했다. 새 색 토큰이나 컴포넌트는 없다.
- **상태 표시:** CUDA 비활성은 색만이 아니라 텍스트 `NVIDIA 드라이버 필요`로도 알린다.
- **접근성:**
  - 장치 그룹에 `role="radiogroup"`, `aria-label`, `aria-describedby`를 달았다.
  - 라디오는 네이티브 input이라 키보드로 조작할 수 있다.
- **모션:** 새 애니메이션이 없어 모션 감소 설정에 영향이 없다.
- **반응형:** 기존 2열 그리드를 그대로 쓴다.
- **숨김 처리:** `[hidden]` 동점 CSS는 계산 스타일 테스트로 고정했다.
- **실제 화면 확인:** Tauri IPC가 필요하므로 일반 브라우저 QA는 Not Run이다. 실기 확인은 Windows 수동 게이트 G6에서 한다.

## 남은 전제 (원장 밖)

- **CI 게이트**(draft PR의 Windows 러너에서 판정): G1, G5, G8, G12a, G17, G9c의 CI 부분.
- **Windows 실기 수동 게이트:** G6, G7b, G8·G9c의 수동 부분. 실기 결과가 없으면 execution complete를 하지 않고 blocked로 보고한다.

## ai-slop-clean

- **변경:** `scripts/ci/windows-install-smoke.ps1`의 환경 사본이 `worker_environment(Os::Windows)`와 키 하나하나까지 일치하도록 공통 키 10개를 추가했다(분류: duplication — 두 곳의 키 목록 불일치 해소).
- **확인:**
  - Windows 전용 Rust 3파일의 모든 `unsafe` 블록에 SAFETY 주석이 있다.
  - 새 `allow`/`expect`에는 모두 사유가 있다.
  - 디버그 출력·TODO·placeholder가 추가되지 않았다(추가 줄 grep 0건).
- **측정**(락 2개와 `.issueops` 제외 diff, base 456b500 대비):
  - 76개 파일, +3,582 / −474줄.
  - 추가된 줄 3,624줄 중 주석 457줄(12.6%).
  - 정리 전후 차이는 smoke 스크립트 환경 키 추가뿐이다.
- **정리 후 재검증**(SlopClean 실행):
  - Rust: `cargo fmt --check` 통과, mac과 `x86_64-pc-windows-msvc`(가짜 RC)의 `cargo clippy --all-targets -D warnings` 통과, `cargo test --all-targets` 105 passed / 1 ignored.
  - 프런트엔드: `bun run check` 종료 코드 0, `bun test` 128 pass.
  - 워커: `uvx ruff check worker`·`uvx ruff format --check worker` 통과, worker unittest 95 OK.
  - 공통: `git diff --check` 통과.

## 구현 리뷰 1차(revise) 반영

독립 리뷰가 Windows 러너의 `cargo test --target x86_64-pc-windows-msvc`에서 실패할 테스트 세 묶음과 문서 불일치 하나를 지적했다. 모두 사실로 확인하고 다음과 같이 고쳤다.

- **`paths/tests.rs`:**
  - 문제: 기대값을 `std::fs::canonicalize`로 만들었다. Windows에서는 이 값이 `\\?\` 접두를 달고 나와, `dunce` 결과와 `Path` 비교에서 같지 않다고 판정된다.
  - 수정: 기대값을 `canonical_blocking`으로 만들었다. 정경화 테스트도 "같은 위치를 가리키고 `\\?\` 접두가 없음"을 단언하도록 바꿨다.
- **`platform/tests.rs`의 mac 특성화 테스트:**
  - 문제: POSIX 문자열 기대값을 쓰는데, Windows 호스트에서는 `join`이 `\`로 경로를 이어 붙여 기대값과 어긋난다.
  - 수정: `#[cfg(unix)]`로 한정하고, 배열을 채우던 빈 `("", "")` 항목을 지웠다.
- **`application/tests.rs`:** 회의록 경로를 `String` 대신 `Path`로 비교하도록 바꿨다.
- **문서:** `docs/ARCHITECTURE.md` 도식의 `14 커맨드`와 `AGENTS.md` 코드맵의 `16 command paths`를 18로 고쳤다.
- **재검증**(macOS arm64):
  - `cargo fmt --check` 통과.
  - `cargo clippy --all-targets -D warnings`가 mac과 Windows 타깃(가짜 RC) 모두에서 통과.
  - `cargo test --all-targets` 105 passed / 1 ignored.
- **보고서 위치:** `artifact/`는 git이 무시하는 디렉터리라, 보고서를 커밋되는 `.issueops/issues/3/report.md`로 옮겼다.

## draft PR CI 1차 (run 37829566909, head 0636d22)

- **통과:** `bundle-windows`, `windows-engine`, `windows-install-smoke`, `frontend`(macos-15·windows-latest), `worker`(macos-15·windows-latest), `rust (macos-15)`.
- **실패:** `rust (windows-latest)` 하나. 테스트는 105 passed, 1 failed, 2 ignored였다.
  - Job Object 트리 종료, Credential Manager 왕복, Power Request 테스트는 통과했다.
  - 실패한 테스트는 `a_disarmed_guard_still_clears_the_job_when_it_closes`이다. 기대와 달리 `!status.success()` 단언이 실패했다.
- **원인:** `KILL_ON_JOB_CLOSE`로 종료된 프로세스는 종료 코드 0을 남긴다. 10초 timeout 안에 끝났으므로 종료 자체는 일어났고(30회 ping은 약 29초가 걸린다), 틀린 것은 테스트의 종료 코드 단언이었다.
- **수정:** 종료 코드 단언을 지우고, 10초 timeout 안에 끝나는지로 판정한다.
- **재검증:**
  - `cargo fmt --check` 통과.
  - `cargo clippy --all-targets -D warnings`가 mac과 Windows 타깃(가짜 RC) 모두에서 통과.

## Windows 실기 1차 (2026-10-09, run 37832580843 설치본, 30af8f4)

- **환경:**
  - OS: Windows 11 Pro 10.0.26300 x64.
  - 설치 전부터 있던 것: VC++ 재배포 패키지 14.51, WebView2 154.0.
  - GPU: RTX 3070(`nvcuda.dll` 있음).
  - 마이크: MATA STUDIO C10.
- **PASS:**
  - 설치: 설치 위치는 `%LOCALAPPDATA%\Galpi`이고, `uv.exe`와 `resources\worker`가 있다.
  - 엔진 UI: WhisperX만 보이고, CPU로 시작한다. CUDA는 선택할 수 있고, CPU↔CUDA 저장도 확인했다.
  - G7b: 녹음을 정지하면 WAV가 생긴다(48 kHz, 2채널, 39초). 버리면 `.wav.part`가 0개 남고 세션 폴더도 삭제된다.
  - G9c:
    - Credential Manager에 `com.m16khb.galpi:hugging-face-token` 항목이 보인다.
    - 재시작 뒤에도 토큰이 "저장됨"으로 표시된다.
    - `settings.json`에 토큰 원문이 없다(키 이름은 값 `null`로 남아 있다).
- **FAIL — G6 모델 준비:**
  - 현상: 준비 단계에서 pyannote가 다음 오류로 실패했다. "Access to model pyannote/speaker-diarization-community-1 is restricted"
  - 원인 확인:
    - 저장된 토큰을 보내면 whoami와 gated config 요청이 모두 200을 받는다.
    - 토큰 없이 보내면 앱과 같은 401 GatedRepo가 나온다.
  - 근본 원인:
    - WhisperX 준비 경로(`prepare_whisperx_models`)는 `DiarizationPipeline`에 토큰을 넘기지 않았다. 워커는 `HF_HUB_DISABLE_IMPLICIT_TOKEN=1`로 실행되므로, 환경 변수 `HF_TOKEN`도 자동으로 쓰이지 않는다.
    - macOS에서는 Qwen3 준비 단계가 토큰을 넘겨 pyannote를 먼저 캐시하므로 이 결함이 드러나지 않았다. 이 결함은 기준 커밋 456b500부터 있었다.
  - 수정:
    - WhisperX 준비 경로가 `HF_TOKEN`을 `token=`으로 넘긴다. 첫 시도와 CPU 재시도 모두 해당한다.
    - whisperx 3.8.6의 `DiarizationPipeline(model_name, token, device, cache_dir)` 시그니처에 맞춰 stub도 고쳤다.
  - 전사 단계는 토큰 없이 캐시만 쓴다. huggingface_hub 0.36.2는 gated 401 응답을 받으면 캐시로 대체하고, 캐시에 없을 때만 오류를 낸다(`file_download.py`의 `_get_metadata_or_catch_error`).
  - 재현 → 확인:
    - 가짜 `whisperx` 모듈을 끼운 임시 스크립트로 확인했다. 수정 전에는 `TOKEN_MISSING [None]`, 수정 후에는 `TOKEN_PASSED`가 나왔다.
    - `uvx ruff check worker`와 `uvx ruff format --check worker`가 통과했다.
    - worker unittest 95개가 OK였다.
- **환경 관측 — G9c ACL:**
  - 관측: `%LOCALAPPDATA%\com.m16khb.galpi`의 ACL에 현재 사용자, SYSTEM, Administrators 외에 `CodexSandboxUsers`(M)와 이름으로 바뀌지 않는 SID 하나(M)가 있다.
  - 원인: 둘 다 `%LOCALAPPDATA%`에서 상속된 항목이다. 이 PC의 Codex 샌드박스가 부여한 것이다.
  - 앱 동작: 앱은 ACE를 추가하지 않는다. 계획 위험 10에 따라 명시적 DACL도 설정하지 않는다.
  - 노출 범위: 비밀은 Credential Manager에 있으므로, 노출 범위는 설정 파일의 참석자·용어집 같은 비밀이 아닌 정보다.
- **NOT RUN:**
  - G6 전사 3파일과 G8 취소 후 프로세스 정리: 모델 준비 실패로 전사를 시작할 수 없었다.
  - assistant API 키: 키가 없어 확인하지 않았다.
  - SmartScreen 경로: gh로 받은 파일에는 Mark of the Web가 없어 확인할 수 없었다.
- **검증 프롬프트의 결함:** G9c의 `Select-String` 명령은 값이 `null`인 키 이름에도 반응하도록 잘못 작성됐다. 판정은 값이 있는지를 기준으로 했다.

## origin/main 병합 (da46360: #5 SEED 디자인, #8 ChatGPT 로그인)

- **병합한 이유:** PR #7이 main과 24개 파일에서 충돌해 GitHub가 CI를 실행하지 않았고, 토큰 수정이 반영된 새 Windows 설치본이 만들어지지 않았다. 사용자 선택(2026-10-09)에 따라 main을 merge commit으로 합쳤다. 백업 ref는 `backup/3-windows-x64-support-pre-merge-20261009`(1cee5f7)다.
- **충돌 해결 원칙:** 양쪽 기능을 모두 유지했다.
  - IPC 커맨드는 24개다(main 23개 + `save_compute_device`).
  - `LocalSettingsStore::new(app, secrets)` 하나를 `SettingsPort`와 `ChatGptStore`가 함께 쓴다.
  - main의 assistant 환경 변수는 `platform::worker_environment`로 만든다. main의 프로세스 오류 처리는 guard 구조 위에 합쳤다.
  - UI는 main의 SEED 스타일을 기준으로 삼고, 그 위에 장치 선택·숨김 규칙·Windows 글꼴을 SEED 토큰으로 다시 적용했다.
- **main 코드의 Windows 보정:**
  - `settings/chatgpt.rs` 테스트의 `PermissionsExt` import와 0600 단언을 `#[cfg(unix)]`로 한정했다.
  - main의 `/bin/sh` 기반 프로세스 오류 코드 테스트도 `#[cfg(unix)]`로 한정했다.
- **긴 비밀의 Credential Manager 저장(ADR 2026-10-09-windows-credential-manager):**
  - 문제: ChatGPT 토큰 레코드가 한 항목의 한도인 2,560바이트를 넘는다.
  - 해결: 한도를 넘는 비밀은 새 세대의 조각(`<target>#<generation>.<i>`)을 모두 쓴 뒤, 원래 target의 manifest를 커밋한다. manifest에는 조각 수·세대·길이·SHA-256이 들어간다. 커밋 뒤와 삭제 때는 `CredEnumerateW`로 쓰이지 않는 조각을 모두 지운다. 2,560바이트 이하는 기존 단일 항목 형식을 그대로 쓴다.
  - 이 결정은 계획 §3.2의 "1,280 코드 유닛 초과 시 `CREDENTIAL_WRITE_FAILED`" 규칙을 대체한다.
  - 구현 리뷰 5차에서 두 가지를 지적받아 고쳤다.
    - Windows 테스트의 조각 수 기대값이 틀렸다(UTF-16 바이트가 아니라 문자 수로 셌다).
    - 조각을 제자리에 덮어써서, 쓰기가 끊기면 두 세대가 섞이고 고아 조각이 남을 수 있었다. 이를 세대별 조각과 열거 정리로 바꿨다.
  - mac에서 순수 로직 테스트 16개가 통과했다. Windows 왕복 테스트 두 개(한도 초과, 기존 단일 항목에서 전환)는 타입 검사만 했고, 실행은 Windows CI에서 한다.
- **병합 후 재검증**(macOS arm64):
  - `bun run check:all`이 종료 코드 0으로 끝났다.
    - cargo test: 200 passed / 1 ignored.
    - bun test: 187 pass.
    - worker unittest: 109 OK.
  - 원장 필터 결과:
    - G7a: 1 passed.
    - `credential`: 16 passed.
    - `settings::`: 26 passed.
    - `os_windows_`: 4 passed.
    - G2·G10a 대상 bun test: 74 pass.
  - 검사 스크립트: `PLATFORM_DOCS_OK`, `PLATFORM_FENCE_OK`.
  - Windows 타깃 `cargo clippy --all-targets -D warnings`(가짜 RC)가 통과했다.
  - `git diff --check`가 통과했다.
  - 병합으로 워크플로와 락 파일은 바뀌지 않았다.

## 완료 기록 결정 (2026-10-09)

- **사용자 지시**: 2026-10-09 "일단 머지할 수 있게 만들어줘. 구체적인 작업은 윈도우에서 할거야", 이어서 "done까지 처리해야 머지하지".
- **계획과 다른 점**: 계획(이슈 본문과 다른 사실 11, 구현 순서 16(c))은 Windows 실기 결과가 있어야 execution complete를 하도록 정했다. 최신 사용자 지시가 이보다 우선하므로, 실기 2차 확인을 머지 뒤 Windows 작업으로 넘기고 완료를 기록한다.
- **완료 시점에 실행된 증거**
  - mac: 게이트 원장 15/15, `bun run check:all` 통과.
  - CI: 9개 job이 모두 통과했다. NSIS 빌드, 무인 설치와 엔진 구성 스모크, Windows Rust 테스트 200개(Job Object, Credential Manager 조각 저장, Power Request 포함)가 여기에 들어간다.
  - Windows 실기 1차 PASS 항목: 설치, 엔진 UI, 녹음과 버리기, HF 토큰 Credential Manager 저장과 재시작 뒤 유지.
- **실행되지 않은 확인**(머지 뒤 Windows에서 진행)
  - G6: 모델 준비와 WhisperX 전사로 결과 3파일 생성. 1차 실패 원인(토큰 미전달)은 1cee5f7에서 고쳤다.
  - G8 수동: 전사를 취소했을 때 워커 프로세스가 0개가 되는지 확인.
  - G9c 일부: assistant API 키와 ChatGPT 토큰을 Credential Manager에 저장하는지 확인.
  - SmartScreen 경로.
