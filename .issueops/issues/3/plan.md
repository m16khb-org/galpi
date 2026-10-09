# Windows 10/11 x64 데스크톱 지원 (플랫폼 어댑터 분리 + NSIS 설치본 + Windows CI)

- Lifecycle ID: `io-21fd3b1e1f06`
- 이슈: https://github.com/m16khb-org/galpi/issues/3
- 브랜치: `3-windows-x64-support`
- 기준(base): `main@456b500cd843577e381d56842465656e3b596e4a`
- 사용자 요청 범위: "이슈 생성 후 세 사이클을 병렬로 전체 진행: 구현 → 정리 → 문서 → 검증 → 커밋·푸시 → draft PR 발행 → execution complete"
- 워크트리 준비 후 Herdr 새 세션으로 인계되며, 인계는 승인 범위를 넓히지 않는다

이 문서는 계획이다. 현재 동작에 대한 모든 진술은 기준 커밋의 코드를 읽고 `경로:줄`로 인용했고, 읽지 못한 것은 `미확인 가정:`으로 표시했다. 이슈 본문이 계약이며 범위를 넓히거나 좁히지 않는다. 다만 이슈 본문이 다루지 않은 필수 사실(기본 프리셋이 Qwen3라는 점, `release.yml`이 이미 무효 워크플로라는 점 등)은 `## 이슈 본문과 다른 사실`에 따로 적었다.

---

## 목표와 비목표

### 목표 (이슈 완료 기준 8개를 그대로 옮기고 A1~A8로 부른다)

| ID | 완료 기준 | 이 계획에서 증명하는 방식 |
|---|---|---|
| A1 | Windows 러너에서 `cargo build --target x86_64-pc-windows-msvc`와 `bun run build`가 NSIS 설치본을 만든다 | CI `bundle-windows` job (G1), 스테이징·빌드 스크립트 로컬 테스트 (G2) |
| A2 | 설치본 실행 시 환경 진단 통과, WhisperX 프리셋으로 샘플 오디오 전사해 `.srt`, `_화자별.txt`, `.aligned.v2.json` 생성 | CI `windows-engine`(락 설치·import 스모크, G5) + CI `windows-install-smoke`(NSIS 무인 설치 → 설치된 uv로 CPU 락 설치·import 스모크, G17) + Windows 실기 수동 게이트(설치본 실행·GUI 환경 진단·전사 3파일, G6) |
| A3 | 녹음 시작·중지·취소 동작, 취소 시 `.wav.part` 미잔존 | 취소 회귀 테스트(mac·Windows 공통, G7a) + Windows 실기 마이크 수동 게이트(G7b) |
| A4 | 전사 취소 시 워커와 그 자식 프로세스 전부 종료 | Job Object 트리 종료 테스트(Windows CI, G8) + 작업 관리자 수동 확인 |
| A5 | HF 토큰·assistant API 키가 Windows Credential Manager에 저장되고 재시작 후에도 유지 | 순수 로직 테스트(mac, G9a·G9b) + Windows CI 실저장소 왕복 테스트 + 수동 확인 (G9c) |
| A6 | 엔진 선택 UI가 Windows에서 Qwen3 숨김, WhisperX의 CPU/CUDA 선택만 표시 | Bun DOM 테스트 + Rust 설정 어댑터 테스트(`Os::Windows` 주입) (G10) |
| A7 | 기존 macOS CI 게이트 불변 통과, CI 매트릭스에 Windows job 추가 | `bun run check:all`(mac, G11) + Windows CI job들 (G12a·G12b·G14a) |
| A8 | `README.md`, `.issueops/TECH_STACK.md`, `.issueops/OPERATIONS.md`, 루트 `DESIGN.md`의 지원 플랫폼 서술에 Windows 포함 | 양성·음성 문구 검사 (G13) |

### 비목표 (이슈 "하지 않는 것"을 그대로 따른다)

- Windows ARM64, Intel macOS, Linux. 이 세 가지를 위한 `cfg` 스텁이나 가짜 폴백도 만들지 않는다(컴파일 불가 상태 유지).
- Qwen3 프리셋의 Windows 백엔드(MLX가 Apple 전용). Windows에서는 선택지에서 숨기고 저장된 값은 기본값으로 되돌린다.
- Authenticode 서명, Microsoft Store. 서명 없는 NSIS 설치본까지만 만든다.
- assistant 인증 방식 변경.
- macOS 비밀 저장 방식 변경. macOS는 지금처럼 설정 파일(`SettingsFile`)을 쓴다. Keychain 전환은 Developer ID 서명과 함께 하기로 이미 문서화돼 있다(`src-tauri/src/adapters/outbound/secrets.rs:1-8`, `README.md:163`).
- ASR을 GPU로 옮기는 일. `worker/AGENTS.md`의 "Do not move ASR to MPS; CTranslate2 CPU int8 is deliberate" 원칙을 CUDA에도 그대로 적용한다. CUDA는 정렬·화자분리(torch)만 가속한다.

### 열린 결정의 확정 (이슈 "열린 결정": CUDA 휠 vs CPU 기본값)

실측(2026-10-08, PyPI JSON·`download.pytorch.org` HEAD `content-length`):

| 휠 | 크기 | 비고 |
|---|---|---|
| `torch-2.8.0-cp312-cp312-win_amd64.whl` (PyPI) | 241 MB | 휠 내용을 zip 중앙 디렉터리로 열어 확인: `torch/lib/`에 CUDA DLL 없음 → CPU 빌드 |
| `torch-2.8.0+cpu` (pytorch cpu 인덱스) | 619 MB | PyPI보다 큼 → 쓰지 않음 |
| `torch-2.8.0+cu126` | 2,915,418,348 B (≈2.92 GB) | |
| `torch-2.8.0+cu128` | 3,461,384,651 B (≈3.46 GB) | |
| `ctranslate2-4.8.2` win_amd64 | 19 MB | CPU/CUDA 공용 휠 |

**결정: Windows 기본은 PyPI의 CPU 빌드(241 MB), CUDA는 사용자가 설정에서 고를 때만 `cu128`(≈3.46 GB, CPU 휠의 약 14배)을 설치한다.** 근거: (1) 다수 팀원의 첫 설치 부담을 CPU 기준으로 최소화, (2) `cu128`을 고른 이유는 RTX 50 시리즈(Blackwell, sm_120) 지원이며 `cu126`은 0.55 GB 작지만 그 GPU를 못 쓴다는 점은 `미확인 가정:`(PyTorch 릴리스 노트로 확인하지 않음)이라, 실패 시 워커의 단일 CPU 폴백이 안전망이 된다, (3) CUDA 선택 UI에 "약 3.5 GB 추가 내려받기"를 명시해 놀람을 없앤다. 어느 쪽이든 완료 기준은 바뀌지 않는다.

`uv pip compile`로 두 락이 실제로 풀리는 것을 임시 디렉터리에서 확인했다(저장소 밖, 결과물은 버림): CPU 락 2,802줄에 `torch==2.8.0`, `torchaudio==2.8.0`, `torchvision==0.23.0`, `torchcodec==0.7.0`, `ctranslate2==4.8.2`, `whisperx==3.8.6`, `pyannote-audio==4.0.7`; CUDA 락은 `torch==2.8.0+cu128`, `torchaudio==2.8.0+cu128`, `torchvision==0.23.0+cu128`(torchvision도 CUDA 빌드여야 CUDA 텐서 연산이 맞는다)와 해시를 포함한다.

---

## 이슈 본문과 다른 사실

1. **macOS는 Keychain을 쓰지 않는다.** 이슈 표는 "비밀 저장 | `security-framework` Keychain"을 macOS의 현재 동작처럼 적었지만, `Keychain`은 `#[expect(dead_code)]`로 컴파일만 되는 죽은 코드이고(`secrets.rs:62-64`), 실제 저장소는 `SettingsFile`이며 `LocalSettingsStore::new`가 그것을 고정 연결한다(`settings.rs:39-41`). 비밀은 `settings.json`에 0600으로 평문 저장된다(`secrets.rs:151-172`, `settings.rs:362`, `README.md:163`). 따라서 Windows Credential Manager는 앱 최초의 OS 보안 저장소이고, macOS와 저장 방식이 비대칭이 된다. 이 비대칭은 의도된 것으로 문서에 적는다.
2. **"`cfg(unix)` 하나가 유일한 타깃 분기"는 틀리다.** `process.rs`에 두 곳(`process.rs:7`의 import, `process.rs:103-105`의 `process_group(0)`)이 있고, `cfg` 없이 unix 전용인 코드가 더 있다: `guard.rs:2-3`(`nix` 무조건 사용), `process.rs:4,46,49`(`nix::Signal`), `settings.rs:11,362`(`PermissionsExt`/`from_mode`), `refinement.rs:181`(`.mode(0o600)`), `model_cache.rs:76`(`std::os::unix::fs::symlink`)와 테스트 `model_cache.rs:92`, `environment.rs:94,129,134-137,215`(`HOME`, 유닉스 PATH, `TMPDIR`, `/tmp`), `setup.rs:396`(`PathBuf::from("/")`), 테스트 `process/tests.rs:53,83`(`/bin/sleep`, `/bin/sh`), `recording/tests.rs:21-49`(`pmset`), `settings.rs` 테스트 `:388,438,575`(`PermissionsExt`). `Cargo.toml:21-22`의 `nix`/`security-framework`는 타깃 구분 없는 일반 의존성이다. 그리고 `main.rs:1-3`에는 `windows_subsystem`이 없어 Windows 릴리스 빌드가 콘솔 창을 띄운다. Rust `domain/`·`application/`에 플랫폼 코드가 없다는 주장은 `cfg(` 전수 검색(테스트용 `cfg(test)`뿐)으로 사실임을 확인했다.
3. **기본 프리셋이 Qwen3다.** `domain/engine.rs:5-10`의 `#[default] Qwen3`가 신규 설치 기본값이고 TS도 `qwen3` 기본 UI다(`app-template.ts:207`). Windows에서는 Qwen3를 쓸 수 없으므로 "플랫폼별 기본 프리셋"이 필요한데 이슈는 이를 언급하지 않았다. 도메인에 `cfg`를 넣을 수 없으므로(이슈의 "플랫폼 분기는 `composition.rs`와 아웃바운드 어댑터에만") 설정 어댑터가 기본값을 결정한다.
4. **"CPU/CUDA 선택"은 현재 존재하지 않는 설정이다.** `TorchDevice`는 `Literal["cpu", "mps"]`이고 자동 선택(`runtime.py:7,27-28`)이라 사용자 설정이 없다. 이 이슈는 새 설정(`ComputeDevice`)과 새 IPC 커맨드 `save_compute_device`를 추가해야 한다.
5. **이슈 검증 명령 중 두 개는 그대로 쓸 수 없다.** `bun run build`는 `cargo tauri build --bundles app --ci && bun run dmg:build`(`package.json:17`)라 Windows에서 `--bundles app`이 무효이고 `hdiutil`이 없다 → 플랫폼 분기 빌드 스크립트 필요. `PYTHONPATH=. python -m unittest ...`는 POSIX 환경변수 문법이라 Windows 러너에서는 `shell: bash`로 실행해야 한다.
6. **`cargo ... --target x86_64-pc-windows-msvc`는 mac에서 곧바로 돌지 않는다.** `tauri-build`가 Windows 타깃이면 `tauri-winres` → `embed-resource`로 리소스를 컴파일하는데(`tauri-build-2.6.3/src/lib.rs:604-640`, `tauri-winres-0.3.6/src/lib.rs:530-555`), 비-Windows 호스트의 msvc 타깃은 `llvm-rc`를 찾고 없으면 `manifest_required().unwrap()`로 패닉한다(`embed-resource-3.0.11/src/non_windows.rs:46-55`). 이 mac에는 `llvm-rc`가 없고(`which llvm-rc` 결과 없음) 설치된 타깃은 `aarch64-apple-darwin` 하나뿐이다(`rustup target list --installed`). 또 `tauri-build`는 타깃 트리플용 사이드카 파일(`binaries/uv-x86_64-pc-windows-msvc.exe`)이 없으면 실패한다(`tauri-build-2.6.3/src/lib.rs:545-551`). 그래서 게이트를 로컬 실행 가능분과 CI 전용으로 나눈다.
7. **`release.yml`은 이미 무효 워크플로다.** `gh run view 37789731837`가 "workflow file issue"로 0초 만에 실패하고, 원인은 `if: ${{ secrets.APPLE_CERTIFICATE != '' }}`(`release.yml:27`), `if: ${{ secrets.APPLE_SIGNING_IDENTITY != '' }}`(`release.yml:56`)처럼 `if`에서 `secrets` 컨텍스트를 쓴 것으로 추정한다(`미확인 가정:` 원인 줄은 actionlint로 확정 — 구현 단계 G14). Windows job을 이 파일에 추가하려면 먼저 파일이 유효해야 하므로 최소 수정(시크릿 존재 여부를 `env`로 받아 `env.HAS_…`로 분기)을 한다. 이는 macOS 릴리스 동작을 바꾸지 않는다.
8. **`.issueops/OPERATIONS.md`는 색인일 뿐이다.** 플랫폼 서술은 `.issueops/operations/guides/overview.md`("macOS 14+ on Apple Silicon", "hardcode ARM64")에 있다. A8은 색인 파일에 Windows 한 줄을 넣고 실제 서술은 가이드 문서에서 고친다.
9. **IPC 커맨드 수 문서가 서로 어긋난다.** 실제는 17개(`composition.rs:40-58`)인데 `AGENTS.md`는 "Sixteen", `docs/ARCHITECTURE.md:53`은 14, `.issueops/OPEN_API_SPEC.md`·`.issueops/architecture/overview.md`도 14다. 이 이슈가 커맨드 1개를 더하므로 18로 맞춘다.
10. **README 영문판이 있다.** 이슈는 `README.md`만 적었지만 `README.en.md`가 같은 문장을 미러링한다(예: `README.en.md:19,44,163`). 한쪽만 고치면 지원 플랫폼 서술이 서로 모순되므로 둘 다 고친다(범위 확장이 아니라 일관성 유지).
11. **수동 실기 결과 없이는 execution complete를 하지 않는다.** A2(설치본 실행·GUI 환경 진단·WhisperX 전사), A3(Windows 녹음 시작·중지·취소), A5(자격 증명 관리자 표시·재시작 유지)는 Windows 실기 관측이 있어야 판정된다. 실기 결과가 없으면 draft PR까지만 진행하고 execution complete 없이 **blocked**로 보고한다(PR 본문 `Not-tested`에 적고 complete하는 우회는 두지 않는다).
12. **CI에서 끌어올 수 있는 부분은 끌어온다(시크릿 불필요한 범위만).** 오디오 입력 장치가 필요한 녹음, GUI 진단, 그리고 전사 3파일 증명은 수동(G6·G7b)으로 남긴다. 전사는 gated 모델 `pyannote/speaker-diarization-community-1`(화자 수를 1로 줘도 pyannote 파이프라인이 항상 실행됨)이 Hugging Face 토큰을 요구하므로 CI 시크릿 없이는 자동화할 수 없다. 대신 NSIS 설치본 무인 설치(`/S`), 설치된 사이드카 존재 확인(`uv.exe`, `resources\worker`), 설치된 `uv.exe`로 CPU 락 설치, `import whisperx, torch, pyannote.audio`와 `python -m galpi_worker --help` 스모크까지를 `windows-install-smoke` job(G17)으로 자동화한다. 이 job은 시크릿·샘플 오디오 픽스처를 쓰지 않는다.

---

## 적용되는 결정과 주의사항

확인한 문서: `AGENTS.md`, `src-tauri/AGENTS.md`, `src-tauri/src/adapters/outbound/AGENTS.md`, `worker/AGENTS.md`, `docs/ARCHITECTURE.md`, `.issueops/CONSTITUTION.md`, `.issueops/ARCHITECTURE.md`(+`architecture/overview.md`), `.issueops/CONVENTIONS.md`(+`conventions/overview.md`), `.issueops/CAUTIONS.md`(+`cautions/overview.md`, 아래 두 기록), `.issueops/ADR.md`(+`adr/overview.md`, 헥사고날 ADR), `.issueops/TESTING.md`(+`testing/overview.md`), `.issueops/TECH_STACK.md`, `.issueops/OPERATIONS.md`(+`operations/guides/overview.md`), `.issueops/DESIGN.md`, 루트 `DESIGN.md`, `.issueops/COMMIT_POLICY.md`, `.issueops/AGENT_WORKFLOW.md`, `.issueops/OPEN_API_SPEC.md`(HTTP API 없음 확인). `cautions/2026-09-05-codegraph-...`은 이 작업과 무관하다.

| 문서 | 항목 | 이 계획에 대한 제약 |
|---|---|---|
| `docs/ARCHITECTURE.md` §1·§2 | 하나의 의존성 규칙, 포트 소유 규칙(DIP) | 플랫폼 분기는 `composition.rs`와 `adapters/outbound/`에만 둔다. `domain/`·`application/`에는 `cfg`를 넣지 않으며 `scripts/check-architecture.ts`에 그 금지를 추가해 게이트로 만든다. |
| `docs/ARCHITECTURE.md` §7 | 변경 시 필수 동반 세트 | IPC 커맨드 추가(`tauri.rs` + `composition.rs` + `BackendPort`/Zod + §2 표)와 새 외부 능력(포트 → 어댑터 → 조합 루트 → `FakePort`)을 각각 한 변경 세트로 처리한다. 워커 프로토콜(JSONL v1)은 건드리지 않는다. |
| `docs/ARCHITECTURE.md` §6 "의도적으로 남겨둔 것" | `DesktopAdapter`의 4포트, `JobRegistry` 비포트화 유지 | 이를 쪼개지 않는다. 플랫폼 차이는 `Os` 값과 순수 함수로 흡수한다. |
| `.issueops/ADR.md` / `adr/2026-08-23-adopt-hexagonal-...` | 기각된 대안(어댑터 쪼개기, 레지스트리 포트화) 재시도 금지 | 위와 동일. 새 결정(플랫폼 분기 규칙, CPU 기본/CUDA 선택)은 문서 단계에서 ADR 레코드로 추가한다. |
| `src-tauri/AGENTS.md` | `AppError` 안정 ASCII 코드·한국어 메시지; Cargo lint가 `unwrap`/`expect`/`panic`/`todo` 거부; 포트는 `Arc<dyn Trait + Send + Sync>` | Windows FFI 코드도 `unwrap`/`expect` 없이 `AppError`로 올린다. 새 에러 코드는 ASCII(`CREDENTIAL_READ_FAILED`, `CREDENTIAL_WRITE_FAILED`, `COMPUTE_DEVICE_UNAVAILABLE`, `ENGINE_PRESET_UNAVAILABLE`, `PROCESS_ERROR` 재사용). |
| `outbound/AGENTS.md` CONVENTIONS | `env_clear`+명시 환경, null stdin, `kill_on_drop`, 전용 프로세스 그룹, 3초 SIGTERM→SIGKILL, Drop 시 SIGKILL | Windows도 같은 계약을 지킨다: 환경은 `process_environment` 한 곳에서만 만들고(임의 변수 추가 금지), 그룹 종료는 `ProcessGroupGuard`의 Job Object 구현으로, Drop 시 강제 종료. Windows에는 그룹 단위 SIGTERM이 없으므로 "정중한 종료"가 즉시 `TerminateJobObject`와 같다는 점을 코드 주석과 문서에 명시한다. |
| `outbound/AGENTS.md` ANTI-PATTERNS | "Do not signal a bare PID or call `child.kill`"; "Keep `tokio::process` and `nix` primitives inside the process adapter"; "Do not publish `.wav` before final rename or omit partial-file cleanup"; "Do not trust paths from the worker… without containment" | `nix`는 `process/guard/unix.rs`에만, Win32 Job Object는 `process/guard/windows.rs`에만 둔다. 정경화(canonicalize) 후 접두 검사는 모든 경로에서 그대로 유지하되 Windows `\\?\` 접두를 `dunce`로 일관되게 제거해 비교 기준을 통일한다. |
| `worker/AGENTS.md` | stdout은 JSONL만; 무거운 import는 함수 안에서; 단일 MPS→CPU 폴백만; `.aligned.v2.json`/`.srt`/`_화자별.txt` 이름 고정 | 폴백은 "가속기(mps/cuda) → CPU 1회"로 일반화할 뿐 재시도 횟수를 늘리지 않는다. 출력 이름·프로토콜 불변. |
| `.issueops/CONSTITUTION.md` | 비밀(`hf_…`, API 키)을 문서·로그·테스트 픽스처·MCP 응답에 쓰지 않는다; 장시간 단계의 소요시간을 지어내지 않는다 | Credential Manager 테스트는 `galpi-test-…` 형태의 더미 값과 테스트 전용 서비스 접두를 쓰고 항목을 반드시 삭제한다. CUDA 3.46 GB는 실측 크기로만 안내하고 "n분 소요" 같은 예상 시간은 쓰지 않는다. |
| `.issueops/CONVENTIONS.md` / `AGENTS.md` | TS strict·named export·무세미콜론; 네이티브 응답은 Zod로 파싱; 이벤트는 invoke 전에 구독 | `EnvironmentStatus` 확장은 Zod 스키마에 반영하고 `save_compute_device` 호출 흐름은 기존 `saveEnginePreset`과 같은 모양을 따른다. |
| `.issueops/TESTING.md` / `testing/overview.md` | 공개 계약으로 검증, Given/When/Then, 결정적 테스트, 페이크는 프로덕션 계약을 지킨다(LSP) | 새 포트 메서드는 `FakePort`에 같은 계약으로 추가한다. 실제 Credential Manager·`powercfg`·마이크를 쓰는 테스트는 `#[cfg(windows)]`로 격리하고 시스템 상태에 의존하는 것은 기존 `pmset` 테스트처럼 `#[ignore]`로 둔다. |
| `.issueops/cautions/2026-08-23-dom-visibility-...` | `textContent`/`hidden` 단언만으로 가시성을 믿지 말 것 | 새 `hidden` 요소는 CSS 동점 문제를 겪는다: 전역 `html [hidden]`(`styles.css:44`)과 `.engine-segmented` 레이아웃 규칙이 같은 명시도라 `.step-list li`처럼 별도 `[hidden]` 규칙이 필요하다(`styles.css:156` 주석 선례). 그 규칙을 `src/styles.test.ts` 계열 계산 스타일 테스트로 고정한다. |
| `.issueops/cautions/2026-08-23-tauri-frontend-renders-...` | 평범한 브라우저에서는 IPC가 죽어 있다; subscribe-before-invoke | UI 변경은 DOM 테스트로 검증하고 IPC가 걸린 동작은 Windows 실기 수동 게이트로만 판정한다. 평범한 브라우저 QA 결과는 "Not Run"으로 분류한다. |
| `.issueops/DESIGN.md` + 루트 `DESIGN.md` | `app-template.ts` + `styles.css` + `DESIGN.md` 세 파일 정합; 상태는 색+텍스트; 40 px 터치 타깃; 폰트 스택 표는 스타일시트를 문서화한다 | 폰트 스택 변경은 세 파일을 한 변경 세트로 고친다. CUDA 라디오 비활성 사유는 색만이 아니라 텍스트(`NVIDIA 드라이버 필요`)로 표시한다. 루트 `DESIGN.md:67-68`의 스택은 이미 CSS와 어긋난다(`styles.css:5-7,572-576`에 `JetBrains Mono`가 없음) → 이번에 CSS 기준으로 바로잡는다. |
| `.issueops/COMMIT_POLICY.md` | 컨벤셔널 커밋 + Why/Tested/Not-tested 본문 | 커밋 본문 `Not-tested`에 Windows 실기 수동 게이트 미실행분을 정직하게 적는다. |
| `.issueops/OPEN_API_SPEC.md` | HTTP/OpenAPI 없음; IPC·JSONL·`AppError` 코드가 계약 | IPC 응답 필드 추가는 가산적이다. JSONL v1은 바꾸지 않는다. |

---

## 재사용하는 기존 구현

| 대상 | 재사용 방식 |
|---|---|
| `SecretStore` 트레이트·`Secret`·`SettingsFile`·`InMemorySecrets` (`secrets.rs:45-172`) | 트레이트는 그대로 두고 `CredentialManager`가 구현한다. `LocalSettingsStore`의 캐시·레거시 이전·`*_stored` 플래그 로직(`settings.rs:65-191`)과 재시작 유지 테스트 `the_assistant_key_survives_a_relaunch`(`settings.rs:696`)가 A5의 "재시작 후 '저장됨' 표시"를 이미 덮는다 — 새 저장소를 끼우기만 하면 된다(`keeps_plaintext_in_settings()`가 `false`면 파일에서 평문이 지워지는 기존 경로, `settings.rs:122-143`). |
| `SleepBlocker::acquire(name) -> Option<Self>` + Drop 계약 (`power.rs:50-89`, 사용처 `recording/mod.rs:151-159`) | 호출부 불변. 파일을 `power.rs`(공용 문서·공개 경로) + `power/macos.rs`(기존 코드 이동) + `power/windows.rs`(신규)로 나눈다. |
| `ProcessGroupGuard::{new,terminate,disarm}` + Drop (`guard.rs:5-38`) 과 `terminate_on_cancel`의 정중→강제 2단계(`process.rs:42-54`) | 호출 구조는 유지하고 `nix::Signal` 인자만 우리 `Escalation` 열거형으로 바꾼다. 취소·종료 후 `child.wait()` 수거 로직은 그대로. |
| `process_environment`·`assistant_environment`·`status`·`whisperx_marker` (`environment.rs:14-211`) | 같은 함수가 `Os`를 받는 순수 변형(`*_for(os, …)`)으로 감싸 mac 호스트에서 Windows 분기를 테스트한다. 마커 문자열 형식은 mac에서 바이트 단위로 동일 유지. |
| `build.rs`의 FNV-1a 지문 (`build.rs:12-47`) | Windows 락 두 개를 `REQUIREMENTS` 배열에 추가해 같은 방식으로 컴파일 타임 지문을 만든다. |
| `scripts/stage-sidecars.ts`의 `stageUv`/`stageWorker` 흐름과 체크섬 검증(`stage-sidecars.ts:30-75`) | 타깃 테이블만 일반화하고 검증 로직은 유지. `curl` 외부 호출은 Bun `fetch`로 바꿔 Windows에서 도구 의존을 줄인다. |
| `scripts/check-architecture.ts`의 `Fence.forbidden` 문자열 검사(`check-architecture.ts:13-42`) | 금지 토큰(`cfg(windows)`, `cfg(unix)`, `target_os`, `cfg!(`)만 `domain`·`application`·`inbound` 펜스에 추가. |
| `AppError::new/io`, `emit`, `run_process`, `JobEvents` | 변경 없이 재사용. |
| `application/tests.rs`의 `FakePort` | `EnginePort`·`SettingsPort` 시그니처 변경을 같은 계약으로 따라간다. |
| `writer_tests.rs`의 `spawn`/`NoopEvents`/`failure::new` 헬퍼 | A3 취소 회귀 테스트가 그대로 재사용(`cleanup.rs`의 `cancel_and_remove`). |
| `AppView.setEngineBadge`, `segmented-control`/`engine-segmented` 마크업·CSS(`app-view.ts:136-146`, `styles.css:1167-1222`) | 장치 선택도 같은 컨트롤을 쓴다. 새 컴포넌트·새 색 토큰 없음. |
| `tauri-backend.ts`의 `saveEnginePreset` 패턴(`tauri-backend.ts:184-186`), `controller.ts:94,116-118`의 `switchEngine` | `saveComputeDevice`/`switchComputeDevice`가 동일 모양. |
| `dunce 1.0.5` | 이미 `Cargo.lock`에 있고(Tauri 전이 의존) 직접 의존으로 승격만 한다. 직접 짠 `\\?\` 제거 함수보다 260자 초과 경로를 올바르게 보존한다. |
| `windows-sys 0.61.2` | 이미 `Cargo.lock`에 있다(`Cargo.lock` `windows-sys` 항목). 직접 의존으로 선언해 Job Object·Power Request·Credential API의 구조체 레이아웃을 손으로 선언하지 않는다. |

**새로 만드는 것과 정당화**

- `adapters/outbound/platform.rs` (`Os` 열거형 + 순수 규칙 함수): 같은 분기(`python 경로`, `ffmpeg 파일명`, `uv 이름`, `락 선택`, `PATH 구성`, `홈/임시 디렉터리`, 허용 프리셋/장치)가 7개 파일에 흩어지는 것을 한 곳으로 모으고, `cfg` 대신 값으로 분기해 **mac 호스트에서 Windows 규칙을 단위 테스트**하기 위한 최소 장치다. `cfg!(windows)`는 `Os::current()` 한 곳에만 둔다. 컴파일에서 제외돼야 하는 것(Win32 FFI, `nix`)만 `#[cfg]`를 쓴다.
- `ComputeDevice`·`EngineSelection` (domain): 장치 선택은 비즈니스 값이고(저장·진단·준비가 공유) 프레임워크 의존이 없다. `EnginePort`가 프리셋만 받던 시그니처를 `EngineSelection`으로 바꾼다(준비 마커가 장치에 따라 달라지기 때문).
- `scripts/sidecar-targets.ts`: 스테이징 스크립트가 import 시 실행되는 구조라 테이블을 테스트하려면 순수 모듈로 분리해야 한다.
- `scripts/build.ts`: 플랫폼별 번들 분기 1곳.
- Windows 락 2개(`requirements-windows-cpu.lock`, `requirements-windows-cuda.lock`): 기존 락이 `--python-platform aarch64-apple-darwin`으로 생성돼 있어(`requirements.lock:1-2`) 재사용 불가.
- `.gitattributes`(`* text=auto eol=lf`): Windows 체크아웃이 CRLF로 바뀌면 `build.rs`의 바이트 지문과 해시 고정 락 파일이 플랫폼마다 달라진다. mac 파일은 이미 LF라 변화 없음.

---

## 설계

### 0. 원칙: 분기를 값으로, 컴파일 제외는 최소로

`adapters/outbound/platform.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os { MacOs, Windows }
impl Os {
    pub const fn current() -> Self { if cfg!(windows) { Self::Windows } else { Self::MacOs } }
    pub fn python_relative(self) -> &'static [&'static str];   // [".venv","bin","python"] | [".venv","Scripts","python.exe"]
    pub fn ffmpeg_file_name(self) -> &'static str;             // "ffmpeg" | "ffmpeg.exe"
    pub fn uv_staged_name(self) -> &'static str;               // "uv-aarch64-apple-darwin" | "uv-x86_64-pc-windows-msvc.exe"
    pub fn uv_installed_name(self) -> &'static str;            // "uv" | "uv.exe"
    pub fn presets(self) -> &'static [EnginePreset];           // [Qwen3, WhisperX] | [WhisperX]; 첫 원소가 플랫폼 기본값
    pub fn devices(self) -> &'static [ComputeDevice];          // [] | [Cpu, Cuda]
    pub fn whisperx_lock(self, device: ComputeDevice) -> WhisperxLock; // 락 파일명 + 추가 uv 인자
}
pub fn worker_environment(os: Os, lookup: &dyn Fn(&str) -> Option<OsString>, paths: &AppPaths, worker_root: &Path) -> HashMap<OsString, OsString>;
pub fn to_wide_nul(value: &str) -> Vec<u16>;                   // Win32 문자열 변환(순수)
pub fn cuda_driver_present(system_root: Option<&OsStr>) -> bool; // <SystemRoot>\System32\nvcuda.dll 존재
```

`Os::current()`는 `cfg!(windows)`를 쓰는 유일한 상수 분기다. 이 방식이면 `if cfg!(windows)`의 양쪽 가지가 모든 플랫폼에서 타입 검사되므로 Windows 분기 로직의 컴파일 오류와 데드코드 린트 문제가 mac에서도 드러난다. 반대로 `windows-sys`, `nix`, 확장 트레이트(`CommandExt`, `PermissionsExt`)처럼 해당 OS에서만 존재하는 것만 `#[cfg(windows)]`/`#[cfg(unix)]`로 격리한다.

### 1. Domain (`src-tauri/src/domain/engine.rs`)

- `EnginePreset`은 와이어 이름(`qwen3`/`whisperx`)을 바꾸지 않는다. `#[default] Qwen3`도 유지한다(`engine.rs:5-10`) — 기본값 변경은 설정 어댑터의 플랫폼 규칙으로 처리하므로 mac 동작이 불변이다.
- 신규 `ComputeDevice { Cpu, Cuda }` (`serde rename_all = "lowercase"`, `Default = Cpu`, `as_str()`): "WhisperX 환경에 어떤 PyTorch 빌드를 설치하느냐"를 뜻한다. mac은 선택지가 없고 항상 `Cpu`(표준 락)다. 런타임 장치(`mps`/`cuda`/`cpu`)는 워커가 설치된 torch에서 자동 탐지한다.
- 신규 `EngineSelection { preset, device }` (`Copy`). 테스트: 직렬화 라운드트립, 미지 이름 거부.

### 2. Application

- `ports.rs`: `EnginePort::diagnose(&self, selection: EngineSelection)`, `prepare(..., selection: EngineSelection)`. `TranscriptionPort::transcribe(..., engine: EnginePreset, ...)`는 그대로(전사는 준비된 venv만 쓴다). `SettingsPort`에 `load_compute_device()`, `save_compute_device(device)` 추가(`ports.rs:98-99` 옆).
- `model.rs` `EnvironmentStatus`(`model.rs:11-21`)에 가산 필드 4개: `compute_device: ComputeDevice`, `available_presets: Vec<EnginePreset>`(첫 원소 = 플랫폼 기본), `available_devices: Vec<ComputeDevice>`(빈 배열 = 선택 없음, mac), `cuda_driver_detected: bool`.
- `use_cases.rs`: 비공개 `engine_selection()`이 설정에서 프리셋·장치를 읽어 묶는다. `diagnose`(`use_cases.rs:57-60`), `prepare`(`:70-77`), `run_transcription`의 준비 확인(`:218-219`)이 이를 쓴다. 공개 메서드 `save_compute_device(device)` 추가.
- `adapters/inbound/tauri.rs`: `save_compute_device` 커맨드(얇게, `save_engine_preset`(`tauri.rs:75-80`)과 동일 모양). `composition.rs:40-58`의 `generate_handler!`에 등록 → 커맨드 18개.

### 3. Outbound 어댑터

**3.1 설정 (`settings.rs`)**
- `LocalSettings.engine_preset`를 `Option<EnginePreset>`로, `compute_device: Option<ComputeDevice>` 추가(두 필드 모두 `#[serde(default, skip_serializing_if = "Option::is_none")]`, camelCase `computeDevice`). 미설정이면 키 자체를 쓰지 않아야 한다: `null`이 쓰이면 구 빌드의 비-Option `EnginePreset`(`settings.rs:221`) 역직렬화가 실패해 `read_settings`가 `SETTINGS_INVALID`를 내므로(`settings.rs:325-332`) 롤백 호환이 깨진다. `is_empty`는 둘 다 `None`일 때만 비어 있음으로 본다(`settings.rs:235-249`; 기존 "기본 프리셋만 저장하면 파일 제거" 동작이 "명시 저장은 유지"로 미세 변경 — 아래 호환성 절에 기록).
- `load_engine_preset`: 저장값이 `Os::presets()`에 있으면 그것, 없거나 비어 있으면 `presets()[0]`. `save_engine_preset`: 목록에 없으면 `AppError("ENGINE_PRESET_UNAVAILABLE", "이 컴퓨터에서는 사용할 수 없는 전사 엔진입니다.")`.
- `load_compute_device`: `devices()`가 비면 항상 `Cpu`. `save_compute_device`: 목록에 없으면 `AppError("COMPUTE_DEVICE_UNAVAILABLE", …)`.
- `LocalSettingsStore::new(app, secrets)`로 비밀 저장소를 주입받는다(지금은 `SettingsFile` 고정, `settings.rs:41`). 테스트용 `for_os(path, os)` 생성자를 둬 `Os::Windows` 규칙을 mac에서 검증한다.
- `write_settings`의 `set_permissions(0o600)`(`settings.rs:362-364`)는 `#[cfg(unix)]`. Windows는 `%LOCALAPPDATA%\com.m16khb.galpi` 하위의 사용자 전용 ACL을 상속한다(`미확인 가정:` 기본 ACL 상속 여부는 Windows 실기에서 `icacls`로 확인 — 수동 게이트 G9에 포함).

**3.2 비밀 저장 (`secrets.rs` → 디렉터리화)**
- `secrets.rs`(트레이트·`Secret`·`SettingsFile`·`InMemorySecrets`) + `secrets/keychain.rs`(기존 `Keychain` 이동, `#[cfg(target_os = "macos")]`) + `secrets/credential.rs`(**플랫폼 무관 순수부**: `credential_target(secret) -> String`(`com.m16khb.galpi:hugging-face-token` 형태), `encode_blob(&str) -> Result<Vec<u8>, AppError>`(UTF-16LE, 2,560바이트=1,280 코드 유닛 초과 시 `CREDENTIAL_WRITE_FAILED`), `decode_blob(&[u8]) -> Result<String, AppError>`) + `secrets/credential_manager.rs`(`#[cfg(windows)]`, `CredWriteW`/`CredReadW`/`CredDeleteW`/`CredFree`, `CRED_TYPE_GENERIC`, `CRED_PERSIST_LOCAL_MACHINE`).
- 읽기: `ERROR_NOT_FOUND` → `Ok(None)`, 그 외 실패 → `AppError("CREDENTIAL_READ_FAILED", …)`(기존 Keychain 구현은 모든 실패를 `None`으로 삼켰다 `secrets.rs:70-76`; Windows는 조용히 비밀을 잃는 쪽보다 오류 노출을 택한다). 삭제: 없는 항목 삭제는 성공으로 취급(`secrets.rs:80-85`과 같은 의미).
- 린트 함정: `SERVICE`(`secrets.rs:17-18`)와 `Secret`의 `account` 메서드(`secrets.rs:28-32`)의 `expect(dead_code)`는 Windows 빌드에서 "충족되지 않은 expect"가 되어 `-D warnings`로 실패한다. 조건을 `#[cfg_attr(all(not(test), not(windows)), expect(dead_code, …))]`로 바꾸고 `Keychain`은 `target_os = "macos"`에서만 컴파일한다.
- 선택은 `composition.rs`: `#[cfg(windows)] let secrets: Arc<dyn SecretStore> = Arc::new(CredentialManager::default());` / `#[cfg(not(windows))] … Arc::new(SettingsFile)`.

**3.3 프로세스 (`process.rs`, `process/guard.rs` → 디렉터리화)**
- `process/guard.rs`는 공용 얼굴: `enum Escalation { Graceful, Force }`, `fn configure(command: &mut tokio::process::Command)`, `struct ProcessGroupGuard`(`attach(&Child) -> Result<Self, AppError>`, `terminate(&self, Escalation)`, `disarm`, Drop 시 armed면 `Force`).
- `process/guard/unix.rs`: 기존 구현 이동. `configure`는 `process_group(0)`(`process.rs:103-105`에서 이동), `terminate`는 `nix::kill(-pid, SIGTERM|SIGKILL)`.
- `process/guard/windows.rs`: `configure`는 `creation_flags(CREATE_NO_WINDOW)`(GUI 앱이 콘솔 자식을 띄울 때 검은 창 방지). `attach`는 `CreateJobObjectW` → `SetInformationJobObject(JobObjectExtendedLimitInformation, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)` → `AssignProcessToJobObject(job, child.raw_handle())`. `terminate`는 두 단계 모두 `TerminateJobObject`. Drop은 핸들을 닫아 `KILL_ON_JOB_CLOSE`가 잔여 프로세스를 정리(정상 종료 후 고아 정리까지 겸함). 워커가 산출물을 임시 파일+`os.replace`로 게시하므로(`worker/AGENTS.md`) 즉시 종료는 mac의 SIGKILL 단계와 동등하다. 스폰 직후 할당 전 창(윈도)에 손자가 생길 수 있는 경쟁은 인터프리터 기동 시간이 손자 생성보다 길다는 점에 기대며 `미확인 가정:` — Windows CI의 트리 종료 테스트(G8)와 수동 게이트에서 관찰한다.
- `process.rs`: `use nix::…`, `CommandExt` import, `#[cfg(unix)]` 두 곳 제거. `terminate_on_cancel`은 `guard.terminate(Escalation::Graceful)` → 3초 대기 → `Force`(`process.rs:42-54`)로 시그니처만 바뀐다. 스폰은 `guard::configure(&mut command)` 후 `ProcessGroupGuard::attach(&child)?`.
- `setup.rs:396` `run_raw`의 `current_dir("/")`: macOS는 `/`를 그대로 유지한다(uv의 `pyproject.toml`/`uv.toml` 탐색이 `$HOME`까지 올라가는 미선언 동작 변경을 피함). Windows는 `/`가 현재 드라이브 루트를 가리켜 의미가 불명확하므로 `Os::neutral_working_directory(paths)`(MacOs → `/`, Windows → `paths.root`)로 분기한다. 순수 함수로 두고 두 값을 테스트한다.

**3.4 경로·환경 (`paths.rs`, `environment.rs`, `model_cache.rs`, `setup.rs`, `transcription.rs`, `refinement.rs`)**
- `AppPaths`: `python`·`qwen3_python`을 `Os::python_relative()`로 조립(`paths.rs:36,42`), 순수 생성자 `AppPaths::from_roots(os, root, documents)`로 분리, `default_output: PathBuf`(`app.path().document_dir()` + `Galpi`) 추가 — Windows는 OneDrive로 리디렉션된 문서 폴더가 흔하고 mac에서는 기존 `~/Documents/Galpi`와 같은 값이다(`environment.rs:68-71`의 `HOME` + `Documents/Galpi` 대체).
- `uv_binary()`(`paths.rs:73-84`): 디버그는 `binaries/<Os::uv_staged_name()>`, 릴리스는 `current_exe().parent()/<Os::uv_installed_name()>`. 테스트: 두 `Os` 값.
- `worker_root`(`paths.rs:86-95`)와 모든 `canonicalize`(`paths.rs:101,126,177,219`, `desktop.rs`, `import.rs`, `transcription.rs:307,327`, `refinement.rs`, `recording/mod.rs:96,225`)는 헬퍼 `canonical(path)`(= `tokio::fs::canonicalize` 후 `dunce::simplified`)로 통일한다. Rust 표준 문서상 Windows의 `canonicalize`는 `\\?\` 확장 경로를 돌려주고, 이를 그대로 워커 argv·`PYTHONPATH`·UI 표시에 쓰면 ffmpeg·Python 일부 경로 처리와 UI 문자열이 깨질 수 있다(`미확인 가정:` 구체 실패 지점은 실기 확인). 포함 검사(`starts_with`)는 양쪽을 같은 헬퍼로 만들어 일관성을 지킨다.
- `environment.rs`:
  - `status(paths, selection)`: 준비 판정에서 ffmpeg 파일명을 `Os::ffmpeg_file_name()`으로(`environment.rs:46,52,63,66`), `default_output_directory`는 `paths.default_output`, 신규 필드 채움(`available_presets`, `available_devices`, `compute_device`, `cuda_driver_detected` = `Os::Windows`이고 `SystemRoot\System32\nvcuda.dll` 존재). 순수 변형 `status_for(os, …)`.
  - 마커: `whisperx_marker(os, device)` — **mac은 기존과 바이트 동일**(`format!("{ENGINE_VERSION}+{GALPI_WHISPERX_REQUIREMENTS_HASH}")`, `environment.rs:14-19`)이라 기존 설치가 재설치를 요구받지 않는다. Windows는 `…+win-<cpu|cuda>-<선택 락의 지문>`으로 장치와 락 변경 모두 준비 상태를 무효화한다. `qwen3_marker`는 불변.
  - `worker_environment`(`process_environment`의 몸통, `environment.rs:88-145`): 공통 키(`PYTHONUTF8`, `PYTHONSAFEPATH`, `HF_*`, `UV_*`, `PYTHONPATH`, …)는 그대로. OS별 부분만 갈린다 — mac: `HOME`, `LANG`/`LC_ALL=ko_KR.UTF-8`, `PATH=<engine_bin>:/usr/local/bin:…`, `TMPDIR`(현행 그대로). Windows: `USERPROFILE`, `SYSTEMROOT`, `WINDIR`, `TEMP`, `TMP`, `LOCALAPPDATA`, `APPDATA`, `COMSPEC`, `PATHEXT`, `HOMEDRIVE`, `HOMEPATH`(호스트 환경에서 복사), `PATH=<engine_bin>;<SystemRoot>\System32;<SystemRoot>`, `LANG`/`LC_ALL` 없음(`PYTHONUTF8=1`이 입출력 인코딩을 고정), `HF_HUB_DISABLE_SYMLINKS_WARNING=1`. `env_clear`로 시작하므로 `SYSTEMROOT` 누락은 Python 기동 실패(해시 난수 초기화 실패로 알려진 문제)를 부르기 쉽다 — `미확인 가정:` 정확한 필수 키 집합은 Windows 실기/CI 스모크로 확정하고, 키 목록은 한 곳(`platform.rs`)에만 둔다.
  - `home_directory()`(`environment.rs:214-216`)는 `platform::home_directory(os, lookup)`로: mac `HOME`, Windows `USERPROFILE`, 폴백은 `std::env::temp_dir()`(하드코딩 `/tmp` 제거).
- `model_cache.rs`: `HOME` 대신 위 홈 헬퍼(`model_cache.rs:13`). `copy_symlink`(`:64-77`)는 `#[cfg(unix)]`로 심볼릭 링크를 유지하고 `#[cfg(windows)]`에서는 심링크 권한 문제를 피해 해석된 대상 파일을 하드링크/복사한다(포함 검사는 그대로). 테스트(`:92`의 `MetadataExt::ino`, `symlink`)는 `#[cfg(unix)]`로 격리하고, Windows에는 복사·포함 검사 테스트를 둔다.
- `refinement.rs:177-183`: `.mode(0o600)`를 `#[cfg(unix)]`로(`OpenOptions`를 가변 변수로 받아 분기). Windows `%TEMP%`는 사용자 전용이다(`미확인 가정:` 기본 ACL).
- `setup.rs`의 WhisperX 설치(`setup.rs:178-256`): 락 선택과 uv 인자를 `Os::whisperx_lock(device)`에서 받는다 — mac `requirements.lock`(+`--require-hashes`, 불변), Windows CPU `requirements-windows-cpu.lock`, Windows CUDA `requirements-windows-cuda.lock` + `--index-strategy unsafe-best-match`(락에 `--index-url …/whl/cu128`, `--extra-index-url https://pypi.org/simple`가 들어 있고, uv 기본 `first-index`에서는 `torch==2.8.0+cu128`을 PyPI 쪽에서 찾지 못하기 때문. 모든 항목이 해시 고정이므로 `unsafe-best-match`가 종속성 혼동 위험을 늘리지 않는다). 세 경우 모두 `--require-hashes`. Qwen3 설치는 mac 전용이라 불변.

**3.5 녹음·전원 (`recording/`)**
- `power.rs`를 공용 얼굴로, `power/macos.rs`(기존 IOKit FFI 그대로 이동, `#[cfg(target_os = "macos")]`), `power/windows.rs`(`#[cfg(windows)]`).
- Windows 구현은 이슈 표의 `SetThreadExecutionState` 대신 **Power Request**(`PowerCreateRequest` + `PowerSetRequest(PowerRequestSystemRequired)`, Drop에서 `PowerClearRequest` + `CloseHandle`)를 쓴다. 이유: `SetThreadExecutionState`는 호출 스레드에 묶이는데 `SleepBlocker`는 `spawn_blocking` 풀의 한 스레드에서 생성되어(`recording/mod.rs:61,151`) 다른 풀 스레드나 `Drop`에서 해제되므로(`mod.rs:68,159`, `cleanup.rs:271-286`) 의도대로 해제되지 않는다. Power Request는 핸들 기반이라 스레드와 무관하고 `powercfg /requests`에 사유 문자열이 보여 관찰 가능하다. 의미는 macOS `PreventUserIdleSystemSleep`과 같다(디스플레이는 꺼질 수 있고 시스템 유휴 절전은 막음). 완료 기준에는 영향이 없다.
- CPAL은 Windows에서 WASAPI를 쓴다(`cpal = "0.18.2"`는 이미 플랫폼 중립 의존). `Stream`을 `spawn_blocking` 사이로 옮기는 현 구조(`recording/mod.rs:26,156`)의 `Send` 성립 여부는 Windows 컴파일에서만 확인된다(`미확인 가정:` → CI 게이트 G12).
- 취소 경로(`cancel_and_remove` → `writer.cancel()`로 스레드 join 후 파일 삭제, `cleanup.rs:313-317`, `writer.rs:132-141`)는 Windows에서 "열린 핸들이 있는 파일은 삭제 불가" 제약과 맞물리므로 취소 후 `.wav.part` 부재를 회귀 테스트로 고정한다(현재 취소 테스트가 없음: `writer_tests.rs`는 `finish`만 다룸).
- `recording/tests.rs:21-49`의 `pmset` 테스트는 `#[cfg(target_os = "macos")]`.

**3.6 데스크톱 연동**
- `main.rs`: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` 추가(릴리스 Windows 빌드의 콘솔 창 제거).
- `desktop.rs`의 `open_path`(opener 플러그인)는 Windows 셸 연결을 그대로 쓴다. 변경 없음.

### 4. 조합 루트 (`composition.rs`)
- `cfg`가 존재하는 유일한 비-어댑터 파일. 변경은 비밀 저장소 선택과 `save_compute_device` 핸들러 등록뿐이다. 나머지 구현체는 `Os::current()`를 어댑터 안에서 읽는다.

### 5. Cargo 의존성 (`src-tauri/Cargo.toml:21-22`)
- `nix` → `[target.'cfg(unix)'.dependencies]`, `security-framework` → `[target.'cfg(target_os = "macos")'.dependencies]`, `windows-sys` 0.61(필요 기능만) → `[target.'cfg(windows)'.dependencies]`(필요 기능은 `Win32_Foundation`, `Win32_Security`, `Win32_Security_Credentials`, `Win32_System_JobObjects`, `Win32_System_Power`, `Win32_System_SystemServices`, `Win32_System_Threading`로 시작해 `cargo clippy` 오류가 요구하는 최소로 줄인다), `dunce = "1"` 일반 의존성. `Cargo.lock`은 이미 해당 버전들을 담고 있으므로 오프라인으로 갱신된다.

### 6. Worker (Python)
- `runtime.py`: `TorchDevice = Literal["cpu", "mps", "cuda"]`. `select_torch_device(*, mps_available: bool, cuda_available: bool = False)`는 `cuda` > `mps` > `cpu` 우선순위(기존 호출·테스트 `test_core.py:271-273`은 키워드 기본값 덕에 그대로 통과). 신규 `detect_torch_device()`는 함수 안에서 `import torch`로 `torch.cuda.is_available()`/`torch.backends.mps.is_available()`를 읽어 `select_torch_device`에 위임한다 — 현재 4곳에 복제된 `mps_available=torch.backends.mps.is_available()`(`engine.py:90`, `preparation.py:139,229`, `qwen3.py:151`)를 `engine.py`·`preparation.py`는 이 헬퍼로 바꾸고 `qwen3.py`는 mac 전용이므로 손대지 않는다.
- `engine.py`·`preparation.py`의 폴백: `if device != "mps": raise` → `if device == "cpu": raise`, 로그 문구 `"MPS … 실패해 CPU로 다시 시도"`를 `f"{device.upper()} … 실패해 CPU로 다시 시도"`로(`engine.py:162-169,196-205`, `preparation.py:166-176,188-200`). 재시도는 여전히 1회다. ASR은 CPU int8 고정이며 안내 문구 `"CTranslate2 CPU · Apple Accelerate"`를 OS 중립인 `"CTranslate2 CPU int8"`로 바꾼다(Apple Accelerate는 Windows에 없다).
- `preparation.py:121-131` `link_ffmpeg`: 링크 이름을 순수 함수 `ffmpeg_link_name(platform: str = sys.platform)`(`win32` → `ffmpeg.exe`, 그 외 `ffmpeg`)로 정한다. Windows 심볼릭 링크 권한 오류는 이미 `except OSError`로 `shutil.copy2` 폴백한다. Rust 쪽 `Os::ffmpeg_file_name()`과 같은 계약이므로 두 곳에 "이 두 줄은 한 세트"라는 주석을 단다.
- 출력 파일명·JSONL 프로토콜·CLI 플래그는 불변.
- `requirements.txt`는 그대로(직접 핀). 신규 `requirements-windows-cpu.lock`, `requirements-windows-cuda.lock`은 아래 명령으로 생성하고 헤더에 명령이 남는다:
  - `uv pip compile --generate-hashes --exclude-newer <생성일> --python-platform x86_64-pc-windows-msvc --python-version 3.12 requirements.txt -o requirements-windows-cpu.lock`
  - `uv pip compile --generate-hashes --exclude-newer <생성일> --emit-index-url --python-platform x86_64-pc-windows-msvc --python-version 3.12 --index-url https://download.pytorch.org/whl/cu128 --extra-index-url https://pypi.org/simple --index-strategy unsafe-best-match requirements.txt -o requirements-windows-cuda.lock` (`<생성일>`은 생성 시점의 RFC 3339 날짜로 고정하고 헤더에 남는다)
- `build.rs`: `REQUIREMENTS`에 두 락을 추가(`GALPI_WHISPERX_WIN_CPU_LOCK_HASH`, `GALPI_WHISPERX_WIN_CUDA_LOCK_HASH`). 기존 두 항목과 mac 마커는 불변.

### 7. 프런트엔드
- `src/domain/job.ts`: `ComputeDevice = "cpu" | "cuda"`, `EnvironmentStatus`에 `computeDevice`, `availablePresets`, `availableDevices`, `cudaDriverDetected`. `src/domain/backend.ts`: `saveComputeDevice(device)`. `tauri-backend.ts`: Zod 스키마 확장(`:27-38`)과 `invoke("save_compute_device", { device })`.
- `app-template.ts`: 엔진 라디오에 `data-engine-option`를 달고(`:207-208`), 장치 그룹(`#engine-device`, `name="compute-device"` 라디오 `cpu`/`cuda`, 안내 `#engine-device-help`)을 추가한다. 안내 문구: CUDA는 PyTorch CUDA 빌드(약 3.5 GB)를 추가로 내려받고, 바꾸면 엔진 준비를 다시 실행해야 함. `CoreAudio · 16-bit PCM WAV`(`:78`)는 `시스템 마이크 · 16-bit PCM WAV`로, `Finder에서 보기`(`:152`)는 `출력 폴더 열기`로, `이 Mac`(`:16,221,234,292`)은 `이 컴퓨터`로 바꾼다.
- `app-view.ts`: `setEnvironment`(`:132-148`)에서 `availablePresets`에 없는 프리셋 라벨을 `hidden`, 장치 그룹은 `availableDevices.length > 0 && enginePreset === "whisperx"`일 때만 표시, CUDA 라디오는 `!cudaDriverDetected`이면 `disabled` + 상태 텍스트 `NVIDIA 드라이버 필요`. 배지 문구는 `availablePresets[0]`이 기본(`기본`), 나머지는 `이전 엔진`. `onComputeDeviceChange(handler)` 추가.
- `app-view.ts:567` `meetingName`은 `path.split("/")`라 Windows 경로(`C:\…\회의.m4a`)에서 상단바 제목에 전체 경로가 나온다 → `split(/[\\/]/)`로 바꾸고 Windows 경로 DOM 테스트(`app-view.dom.test.ts`)를 추가한다.
- `controller.ts`: `switchComputeDevice(device)`가 `switchEngine`(`:116-118`)처럼 저장 후 재진단한다.
- `styles.css`: 폰트 스택에 Windows 글꼴 추가 — 본문 `… "Apple SD Gothic Neo", "Segoe UI", "Malgun Gothic", sans-serif`(`:5-7`), 모노 5곳(`:572-576`, `:997-1001`, `:1113`, `:1580-1584` 및 인접 선언)에 `Consolas`. 새 `[hidden]` 동점 해소 규칙(`.engine-segmented label[hidden]`, `#engine-device[hidden]`)을 추가한다.
- `vite.config.ts`: `build.target: ["safari17", "chrome120"]`로 WebView2(Chromium) 하한을 명시한다. 주석의 "WKWebView only" 서술도 갱신. esbuild·lightningcss가 두 하한의 합집합으로 낮추므로 mac 번들 출력이 달라질 수 있어 빌드 산출물 크기 변화를 기록한다(성능 영향 참고).
- `tauri.conf.json`의 CSP는 이미 `connect-src ipc: http://ipc.localhost`(Windows IPC 오리진)를 포함한다(`tauri.conf.json:27`) — 변경 없음. `img-src asset:`은 현재 사용처가 없다(`convertFileSrc` grep 결과 없음).

### 8. 빌드·패키징
- `tauri.conf.json`: `bundle.windows.nsis`(`installMode: "currentUser"`, `languages: ["Korean", "English"]`)만 추가. `externalBin: ["binaries/uv"]`(`:42`)는 Tauri가 타깃 트리플·`.exe` 접미사를 붙여 찾으므로 그대로다. `bundle.targets`는 mac 기준 `["app"]` 유지(Windows는 CLI `--bundles nsis`로 덮는다). 기존 `icon.ico`는 이미 목록에 있다.
- `scripts/sidecar-targets.ts` + 테스트: `{ platform, arch } → { target, archiveName, archiveSha256, binarySha256, innerPath, stagedName }`. mac 값은 현행 상수(`stage-sidecars.ts:6-9`). Windows: `uv-x86_64-pc-windows-msvc.zip` SHA-256 `4c4d49d8738847d9b71ba319e49a5688c93eac0fe6204b1df24e98528dddf39a`(릴리스 `.sha256` 파일과 내려받은 아카이브의 `shasum`이 일치), zip 루트의 `uv.exe` SHA-256 `8da6cedef60c27ac997ebf400fbfc6d373c5b0a7ae6a299b9d52be7fe63723fb`(zip 목록: `uv.exe`, `uvw.exe`, `uvx.exe`가 루트에 있고 하위 디렉터리 없음), 스테이징 이름 `uv-x86_64-pc-windows-msvc.exe`. `GALPI_SIDECAR_TARGET`로 타깃을 강제할 수 있다(mac에서 Windows 타깃을 교차 검사할 때 사용). 테스트는 Rust `platform.rs`의 `uv_staged_name` 문자열이 이 테이블과 일치하는지 소스 텍스트로 대조해 두 언어 상수의 드리프트를 막는다.
- `scripts/stage-sidecars.ts`: 위 테이블 사용, `curl` → `fetch`, 압축 해제는 확장자로 분기(`.tar.gz` → `tar -xzf`(현행), `.zip` → Windows는 PowerShell `Expand-Archive`, 그 외 호스트는 `unzip -q -o`), `chmod`는 비-Windows에서만. `requirements`에 Windows 락 2개를 복사 목록에 추가(`stage-sidecars.ts:66-71`).
- `scripts/build.ts` + `package.json` `"build": "bun run scripts/build.ts"`: `darwin` → `cargo tauri build --bundles app --ci` 후 `bun run dmg:build`(현행과 동일), `win32` → `cargo tauri build --bundles nsis --ci`. 그 외는 명확한 에러. `dmg:build`와 `build-dmg.ts`는 불변.
- `rust-toolchain.toml`의 `targets`에 `x86_64-pc-windows-msvc` 추가.

### 9. CI·릴리스
- `ci.yml`: `frontend`/`rust`/`worker` 세 job을 `strategy.matrix.os: [macos-15, windows-latest]`(`fail-fast: false`), `runs-on: ${{ matrix.os }}`, `defaults.run.shell: bash`로 전환. `rust` job은 mac에서 현행 명령과 동일하게, Windows에서는 `matrix.cargo_target = "--target x86_64-pc-windows-msvc"`를 `clippy`/`test`에 붙인다(이슈 검증 명령과 일치). 추가 job 셋은 Windows 전용: `windows-engine`(`uv venv` → CPU 락을 `--require-hashes`로 설치 → `python -c "import whisperx, torch, pyannote.audio"` 스모크 → CUDA 락 `uv pip install --dry-run`으로 해석·인덱스 접근만 확인, 3.46 GB 다운로드 없이), `bundle-windows`(`cargo install tauri-cli --version 2.11.4 --locked` → `bun run build` → `src-tauri/target/release/bundle/nsis/*.exe` 존재 확인 → 아티팩트 업로드), `windows-install-smoke`(아래). 모든 job 이름은 매트릭스 값이 붙어도 branch protection이 없으므로(`gh api …/branches/main/protection` 404) 이름 변경의 부작용이 없다.
- `release.yml`: `if: ${{ secrets.… != '' }}`를 `env`에 시크릿 존재 여부를 받아 `if: env.HAS_APPLE_CERT == 'true'` 식으로 고치고, `nsis` job(Windows, `cargo install tauri-cli --version 2.11.4 --locked` → `bun run build` → NSIS 아티팩트 업로드; 저장소에 `@tauri-apps/cli` 의존이 없어 `bundle-windows`와 같은 설치 단계가 필요) 추가. mac `dmg` job의 명령은 불변(단, 이 job이 `tauri-cli`를 설치하지 않는 점은 `release.yml:12-45`로 확인된 기존 상태이며 이 이슈에서 건드리지 않는다 — 위험 절 참고).
- `windows-engine`은 락 설치 뒤 `GALPI_TEST_PYTHON=<venv python> cargo test --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc -- --ignored windows_worker_environment_starts_python`도 실행한다. 이 `#[cfg(windows)] #[ignore]` 테스트는 `run_process`와 `worker_environment(Os::Windows, …)`(`env_clear` 상태)로 `python -c "import whisperx, torch, pyannote.audio"`를 띄워, 필수 환경 키 집합(`SYSTEMROOT` 등)이 맞는지 수동 G6 이전에 CI에서 증명한다. 평범한 `cargo`는 스테이징된 `binaries/uv-*`·`resources/worker`와 `dist/`가 없으면 실패하므로(`ci.yml:33-41` 주석) 이 job의 `cargo test` 앞에 `rust` job과 같은 선행 단계를 복사한다: `actions/checkout` → `oven-sh/setup-bun`(`bun-version-file: package.json`) → `bun install --frozen-lockfile` → `bun run vite:build` → `bun run sidecar:stage` → Rust toolchain(`rust-toolchain.toml`) → `Swatinem/rust-cache@v2`(`workspaces: src-tauri`).
- `windows-install-smoke`(G17, `needs: bundle-windows`, `windows-latest`, 시크릿·fork 가드 불필요): `galpi-nsis` 아티팩트를 내려받아 `Start-Process <설치본> -ArgumentList /S -Wait`로 무인 설치 → 설치 폴더를 찾아(`미확인 가정:` `installMode: currentUser`의 기본 위치는 `%LOCALAPPDATA%\Galpi`이며 스크립트가 `uv.exe`를 탐색해 확정) `uv.exe`, `resources\worker\galpi_worker`, `resources\worker\requirements-windows-cpu.lock`의 존재를 단언 → 설치된 `uv.exe`로 `--version`, `python install 3.12`, `venv`, `pip install -r resources\worker\requirements-windows-cpu.lock --require-hashes` → `worker_environment(Os::Windows)`와 같은 키 집합의 환경에서 `python -c "import whisperx, torch, pyannote.audio"`와 `python -m galpi_worker --help`(`PYTHONPATH=resources\worker`)가 종료 코드 0임을 단언. 스크립트는 워크플로 인라인 대신 `scripts/ci/windows-install-smoke.ps1` 파일로 둔다. 모델 다운로드·전사·취소·녹음은 다루지 않는다(전사 3파일은 G6 수동, 취소 트리 종료는 G8 Rust 테스트, 녹음은 G7b 수동).
- `scripts/verify-windows-lock.py <cpu|cuda>`(G3a·G3b 전용): 워크스페이스 안에 임시 디렉터리를 만들어 `worker/requirements.txt`를 복사하고, 커밋된 락 파일 **헤더에 적힌 명령줄**(날짜 고정 `--exclude-newer` 포함)을 그대로 읽어 같은 상대 인자로 `uv pip compile`을 다시 실행해 커밋된 락과 바이트 비교하고, 같으면 `LOCK_REPRODUCIBLE <cpu|cuda>`를 출력한다(다르면 종료 코드 1). 임시 디렉터리는 항상 지운다.
- `scripts/verify-docs-platform.py`(G13 전용)와 `scripts/verify-platform-fence.py`(G15 전용): 정규식·경로 검사를 CHECK 인자 밖으로 옮긴 헬퍼다(원장 CHECK의 큰따옴표 안 백슬래시는 정책 토크나이저가 제거해 정규식이 깨지므로). 각각 `PLATFORM_DOCS_OK`, `PLATFORM_FENCE_OK` 한 줄을 출력하고 실패하면 사유를 출력하며 종료 코드 1이다.
- 줄 끝: `.gitattributes` 추가(`* text=auto eol=lf`).

### 10. 아키텍처 펜스
- `scripts/check-architecture.ts`: `domain`·`application`·`adapters/inbound` 펜스의 `forbidden`에 `cfg(windows)`, `cfg(unix)`, `target_os`, `cfg!(`를 추가한다(현재 위반 0건, `cfg(` 전수 검색으로 확인). `adapters/outbound`와 `composition.rs`만 허용되는 구조를 실행 가능한 형태로 만든다.

---

## 성능 영향

- **핫 패스**: 오디오 콜백(`capture.rs`)과 JSONL 읽기 루프(`process.rs` `read_bounded_line`)는 건드리지 않는다. `SleepBlocker` 획득은 녹음 시작 시 1회(콜백 밖), Job Object 생성·할당은 프로세스 스폰당 1회로 마이크로초~밀리초 단위다. `status()`에 `nvcuda.dll` 존재 확인 1회(`is_file`)가 추가되지만 진단·전사 시작 시점에만 호출된다.
- **복잡도**: 변경 없음(모두 O(1) 분기·상수 크기 환경 맵). 락 파일 선택은 상수 시간. `ProcessGroupGuard`의 상태는 Windows에서 핸들 1개.
- **설치 크기(실측)**: Windows 기본(CPU) torch 휠 241 MB, CUDA 선택 시 `cu128` 3,461,384,651 B. 이 외 의존성(ctranslate2 19 MB 등)의 합계는 이번에 측정하지 않았다 — 소요 시간 추정치는 쓰지 않는다(`CONSTITUTION.md`).
- **측정 계획**: (1) 프런트 번들: 변경 전 `bun run vite:build`의 `dist/` 총 바이트와 변경 후를 비교해 `chrome120` 하한 추가 영향을 PR 본문에 기록한다. (2) CI 시간: 새 Windows job의 실행 시간을 PR 본문에 기록한다(첫 `windows-engine`은 락 전체 설치가 포함됨). (3) mac 회귀: 기존 mac CI job 시간이 매트릭스 전환 전후로 유사한지만 확인한다. 별도 마이크로벤치마크는 만들지 않는다.

---

## 하위 호환성과 side effect

- **IPC 커맨드/이벤트**: `diagnose_environment`·`prepare_environment` 응답에 필드 4개가 **가산**된다(기존 필드 불변). Zod `z.object`는 미지 키를 버리므로 구 프런트는 계속 동작하지만 신 프런트는 신 필드를 요구하므로 둘은 함께 배포된다(단일 앱 번들). 신규 커맨드 `save_compute_device` 1개(총 18). `job-event`/`recording-event` 불변. Tauri capability(`capabilities/default.json`)는 새 플러그인을 쓰지 않아 불변.
- **워커 JSONL 프로토콜 v1**: 이벤트·필드·`v` 불변. CLI 플래그 불변. 변하는 것은 워커가 설치된 torch에서 `cuda`를 고를 수 있다는 점과 `log` 이벤트 문구뿐이다(`MPS …` → `<장치> …`).
- **저장된 설정**: `settings.json`에 선택 키 `computeDevice` 추가, `enginePreset`는 "없으면 플랫폼 기본". 두 필드는 `skip_serializing_if = "Option::is_none"`이라 미설정이면 키가 파일에 쓰이지 않는다(테스트로 고정). 구 mac 설정 파일은 그대로 읽힌다(`enginePreset: "qwen3"` 유지, 키 부재는 mac에서 Qwen3와 동일). 새 앱이 쓴 파일을 구 버전이 읽어도 `deny_unknown_fields`가 없어 `computeDevice`를 무시하고 `enginePreset` 부재는 구 기본(Qwen3)이 된다 → **롤백 안전**(`"enginePreset": null`이 쓰이면 구 빌드가 `SETTINGS_INVALID`로 실패하므로 skip이 필수). 미세 변화: "기본 프리셋만 저장된 파일"은 이전에는 `is_empty`로 삭제됐으나 이제 명시 저장이라 유지된다(기능 영향 없음, 테스트 갱신).
- **Keychain/자격 증명**: macOS는 불변(설정 파일 0600). Windows는 Credential Manager 항목 `com.m16khb.galpi:hugging-face-token`, `com.m16khb.galpi:assistant-api-key`에 저장하고 `settings.json`에는 `*_stored` 플래그만 남긴다. 롤백(커밋 되돌리기) 시 Windows 항목이 남지만 무해하며 앱이 읽지 않는다(Windows 빌드 자체가 사라짐).
- **엔진 준비 마커**: mac `ready-3.8.6` 내용은 바이트 동일(순수 테스트로 고정) → 업데이트 후 재설치 강제 없음. Windows는 신규.
- **출력 파일명**: `.srt`, `_화자별.txt`, `.aligned.v2.json`, `_회의록.md`, 녹음 폴더명 `YYYY-MM-DD HHMMSS 녹음` 불변. 폴더 이름 정화(`sanitize_name`)는 Windows 금지 문자(`<>:"/\|?*`)를 이미 `-`로 치환하지 못한다 — 현재 규칙은 영숫자·`-`·`_`·공백만 허용하고 나머지를 `-`로 바꾸므로(`paths.rs:238-250`) 금지 문자는 이미 제거된다(검증됨). 예약 이름(`CON`, `NUL` 등)은 `미확인 가정:` 별도 처리 없음 — 오디오 파일명이 그 이름일 확률이 낮아 이번 범위에서 다루지 않되 위험 절에 기록한다.
- **패키징**: macOS DMG 이름·위치 불변. 신규 산출물은 `src-tauri/target/release/bundle/nsis/*.exe`뿐이다.
- **의존성/빌드**: `nix`와 `security-framework`가 타깃 한정으로 이동해 mac 빌드 그래프는 그대로, Windows 그래프에서 제외된다. `windows-sys`·`dunce` 직접 의존 추가로 `Cargo.lock`의 `galpi` 항목이 바뀐다.
- **롤백 경로**: 모든 변경은 한 브랜치의 커밋 되돌리기로 복구된다. 데이터 마이그레이션이 없고(설정 키 가산, 마커 불변) 외부 상태 변경은 Windows 사용자의 Credential Manager 항목과 앱 데이터 폴더뿐이다.
- **side effect 주의**: (1) `release.yml` 수정으로 지금까지 매 푸시마다 0초 실패하던 워크플로 실행이 사라진다(의도된 부수 효과). (2) `.gitattributes`의 `eol=lf`는 mac 작업 트리에 변화를 주지 않는다(파일이 이미 LF). (3) `rust-toolchain.toml`에 타깃을 추가하면 처음 실행 시 rustup이 Windows 타깃 표준 라이브러리를 내려받는다.

---

## 구현 순서

각 단계는 **먼저 실패하는 테스트(또는 먼저 실패하는 검사)**를 쓰고 → 최소 구현으로 통과시킨다(RED → GREEN). 구현자는 macOS arm64에서 작업하므로 단계 안의 "로컬 검증"은 mac에서 돌아가는 것만 뜻한다. Windows 전용 코드의 컴파일·린트·실행은 단계 14·16의 draft PR CI 루프에서 확인한다. 단계마다 커밋하지 않고(`COMMIT_POLICY`의 검증 후 커밋), 논리 변경 세트 단위로 마지막에 원자적 커밋으로 나눈다.

0. **환경 준비 / 의존성 매니페스트.**
   - RED: `grep -q 'x86_64-pc-windows-msvc' rust-toolchain.toml`이 실패한다(현재 `aarch64-apple-darwin`만). `.gitattributes` 부재 확인.
   - GREEN: `rust-toolchain.toml`에 타깃 추가, `.gitattributes` 추가, `Cargo.toml`을 `[target.'cfg(unix)']`(nix), `[target.'cfg(target_os = "macos")']`(security-framework), `[target.'cfg(windows)']`(windows-sys), 일반 `dunce`로 재구성. 로컬: `cargo check --manifest-path src-tauri/Cargo.toml`이 mac에서 계속 통과.
1. **`platform.rs` 순수 규칙.**
   - RED: `adapters/outbound/platform/tests.rs` — `python_relative(Windows)`가 `.venv\Scripts\python.exe`, `ffmpeg_file_name`이 OS별로 `ffmpeg`/`ffmpeg.exe`, `uv_staged_name`/`uv_installed_name`, `presets(Windows) == [WhisperX]`와 `presets(MacOs)[0] == Qwen3`, `devices(MacOs)` 빈 배열·`devices(Windows) == [Cpu, Cuda]`, `whisperx_lock(MacOs, _)`는 `requirements.lock`과 인덱스 인자 없음, Windows CUDA는 `unsafe-best-match` 포함, `worker_environment(Windows, …)`가 `SYSTEMROOT`·`USERPROFILE`·`TEMP`를 포함하고 `HOME`/`LANG`을 포함하지 않으며 `PATH`가 `;`로 `engine_bin` 다음에 `System32`를 잇는지, `worker_environment(MacOs, …)`가 현행 키와 값을 그대로 만드는지(현행 `environment.rs:88-145`와 동일 단언), `to_wide_nul("가a")` 끝에 0, `cuda_driver_present`의 존재/부재.
   - GREEN: `platform.rs` 구현과 `outbound/mod.rs` 등록. `environment.rs`는 이 함수를 호출하게 한다(아직 시그니처 불변).
2. **도메인 + 애플리케이션 + IPC.**
   - RED: `domain/engine.rs` 테스트(`ComputeDevice` 라운드트립·미지 이름 거부, `EngineSelection` 구성), `application/tests.rs`에 "저장된 장치가 `diagnose`·`prepare`·전사 전 준비 확인에 `EngineSelection`으로 전달된다"와 "`save_compute_device`가 설정 포트로 위임된다"(둘 다 컴파일 실패로 시작).
   - GREEN: `ports.rs`·`model.rs`·`use_cases.rs`·`tauri.rs`·`composition.rs`·`FakePort` 갱신. `adapters/outbound/setup.rs`·`desktop.rs`의 시그니처를 따라간다.
3. **설정 어댑터.**
   - RED: `settings.rs` 테스트 — `for_os(path, Os::Windows)`에서 (a) 저장값이 없으면 `load_engine_preset() == WhisperX`, (b) `save_engine_preset(Qwen3)`가 `ENGINE_PRESET_UNAVAILABLE`, (c) 파일에 `"enginePreset":"qwen3"`가 있어도 `WhisperX`로 정정, (d) `save_compute_device(Cuda)` 저장·재로드, (e) `Os::MacOs`에서는 기본 `Qwen3`·`save_compute_device`가 `COMPUTE_DEVICE_UNAVAILABLE`·기존 파일 호환(키 부재). 기존 `PermissionsExt` 단언 테스트(`settings.rs:388,438,575`)는 `#[cfg(unix)]`로 격리.
   - RED(롤백 호환): 프리셋 미설정 상태에서 `save_assistant`로 다른 설정을 저장한 뒤 `settings.json` 원문에 `enginePreset`/`computeDevice` 키가 **존재하지 않음**을 단언; 프리셋 저장 후에는 키가 존재.
   - GREEN: `Option` 필드, `is_empty` 조정, `new(app, secrets)`/`for_os`, `#[cfg(unix)] set_permissions`.
4. **비밀 저장 순수부와 Windows 구현.**
   - RED: `secrets/credential.rs` 테스트(mac에서 실행) — `credential_target`이 `com.m16khb.galpi:hugging-face-token`/`…:assistant-api-key`, `encode_blob`/`decode_blob` 왕복(한글 포함), 1,280 코드 유닛 초과 시 `CREDENTIAL_WRITE_FAILED`, 홀수 길이/잘못된 UTF-16 디코드는 `CREDENTIAL_READ_FAILED`. `#[cfg(windows)]` 왕복 테스트(쓰기→읽기→삭제→읽기 `None`, 테스트 전용 서비스 접두, 더미 값) — mac에서는 컴파일되지 않아 RED 확인 불가이므로 CI(G9)에서 확정.
   - GREEN: 디렉터리화(`keychain.rs`, `credential.rs`, `credential_manager.rs`), `expect(dead_code)` 조건 수정, 조합 루트 선택.
5. **프로세스 가드 분리.**
   - RED(특성화 + 신규): 기존 `process/tests.rs`의 `/bin/sleep`·`/bin/sh` 테스트에 `#[cfg(unix)]` 부착(동작 불변 확인), 신규 unix 테스트 "취소하면 손자 프로세스까지 사라진다"(`sh -c 'sleep 30 & echo $!; wait'`의 stdout 로그 이벤트로 PID를 받아 취소 후 `nix::sys::signal::kill(pid, None)`이 `ESRCH`가 될 때까지 짧은 한도로 폴링), 신규 `#[cfg(windows)]` 테스트 "Job Object 취소가 `cmd /c ping` 자식·손자를 모두 종료한다"(가드에 `#[cfg(test)]`로 `active_processes()`를 두고 `QueryInformationJobObject`의 `ActiveProcesses`가 2 이상이 됐다가 취소 후 0이 되는지 폴링)와 "정상 종료 후 `disarm`하면 job이 남은 프로세스를 정리한다". 리팩터링이므로 mac에서는 먼저 기존 테스트가 통과함을 확인한 뒤 가드를 분리한다.
   - GREEN: `guard.rs`/`guard/unix.rs`/`guard/windows.rs`, `process.rs`에서 `nix`·`cfg(unix)` 제거, `setup.rs`의 `current_dir`.
6. **경로·환경·모델 캐시.**
   - RED: (a) `AppPaths::from_roots(Windows, …)`의 `python`이 `…\.venv\Scripts\python.exe`, `default_output`이 `documents/Galpi`; (b) `uv_binary_for(os, debug, …)` 4조합; (c) `whisperx_marker(MacOs, Cpu)`가 현행 형식(`ENGINE_VERSION+HASH`)과 바이트 동일, `(Windows, Cpu)`와 `(Windows, Cuda)`가 서로 다르고 둘 다 mac 마커와 다름; (d) `status_for(Windows, …)`가 `engine_bin/ffmpeg.exe` 존재로 `ffmpeg_ready`를 판정하고 `available_presets == [WhisperX]`·`available_devices == [Cpu, Cuda]`, mac은 `[Qwen3, WhisperX]`·`[]`; (e) `whisperx_install_args`가 세 조합에서 기대 인자(`--require-hashes` 공통, CUDA만 `--index-strategy unsafe-best-match`) — `platform.rs`에서 이미 실패하는 것은 단계 1과 합친다; (f) `model_cache` — Windows 분기 테스트는 `cfg(windows)`로, 기존 심링크 테스트는 `cfg(unix)`로 격리; (g) `canonical()`이 mac에서 `canonicalize`와 동일(항등)임을 임시 디렉터리로 확인하고 `\\?\C:\a\b`/`\\?\UNC\…` 문자열 변환은 `dunce` 동작에 위임하므로 Windows CI에서만 단언.
   - GREEN: `paths.rs`·`environment.rs`·`model_cache.rs`·`setup.rs`·`transcription.rs`·`refinement.rs`(`#[cfg(unix)] mode`)·`desktop.rs`·`import.rs`·`recording/mod.rs`의 `canonical()` 적용과 `build.rs` 지문 추가.
7. **녹음·전원.**
   - RED: (a) 신규 크로스플랫폼 테스트 `cancelling_a_recording_leaves_no_partial_file_or_empty_folder` — `writer::spawn`으로 `<temp>/<folder>/x.wav.part`를 만들고 샘플 일부를 보낸 뒤 `cancel_and_remove`를 호출하고, 파일과 빈 폴더가 모두 없음을 단언(mac에서도 통과해야 하는 특성화 테스트이자 Windows 핸들 해제 회귀 보호); (b) `#[cfg(windows)]` `sleep_blocker_is_acquired_and_released`(획득이 `Some`, Drop 후 패닉 없음); (c) `powercfg /requests`로 사유 문자열을 확인하는 테스트는 `#[ignore]`(관리자 권한 필요, 기존 `pmset` 선례).
   - GREEN: `power.rs` 분리, `power/windows.rs`, `recording/tests.rs`의 `pmset` 테스트를 `#[cfg(target_os = "macos")]`로.
8. **엔트리·설정 파일.**
   - RED: `grep -q windows_subsystem src-tauri/src/main.rs` 실패, `grep -q '"nsis"' src-tauri/tauri.conf.json` 실패.
   - GREEN: `main.rs`, `tauri.conf.json`의 `bundle.windows.nsis`.
9. **워커 파이썬.**
   - RED: `worker/tests/test_core.py`에 (a) `select_torch_device(mps_available=True, cuda_available=True) == "cuda"`, `(False, True) == "cuda"`, `(True, False) == "mps"`, `(False, False) == "cpu"`(기존 `:271-273` 단언 유지), (b) `ffmpeg_link_name("win32") == "ffmpeg.exe"`, `("darwin") == "ffmpeg"`, (c) 폴백 조건을 순수 함수 `needs_cpu_fallback(device) -> bool`(`device != "cpu"`)로 빼서 `cuda`/`mps`는 참, `cpu`는 거짓을 단언 — 모두 import 실패/NameError로 시작.
   - GREEN: `runtime.py`, `engine.py`, `preparation.py` 수정. 로컬: `uvx ruff check worker && uvx ruff format --check worker`, `PYTHONPATH=. python3 -m unittest discover -s worker/tests -t . -v`.
10. **락 파일 생성.**
    - RED: `test -f worker/requirements-windows-cpu.lock`가 실패.
    - GREEN: 위 두 `uv pip compile` 명령으로 생성하고 `scripts/verify-windows-lock.py`를 함께 만든다(이 스크립트가 없으면 G3a·G3b가 실패하는 것이 RED). `build.rs`가 지문을 만드는지 `cargo check`로 확인. 재현성 게이트(G3a, G3b, G4)로 마무리.
11. **스크립트.**
    - RED: `scripts/sidecar-targets.test.ts` — `resolveSidecarTarget("darwin","arm64")`가 현행 상수와 같고, `("win32","x64")`가 위의 두 SHA와 `uv-x86_64-pc-windows-msvc.exe`, `("linux","x64")`는 `SidecarStageError`, `GALPI_SIDECAR_TARGET` 강제 지정, 모든 해시가 64자리 소문자 16진, Rust `platform.rs` 소스에 각 `stagedName`이 문자열로 포함됨. `scripts/build.test.ts`(순수 함수 `bundleCommands(platform)`) — `darwin`은 `[cargo tauri build --bundles app --ci, bun run dmg:build]`, `win32`는 `[cargo tauri build --bundles nsis --ci]`.
    - GREEN: `sidecar-targets.ts`, `stage-sidecars.ts` 개편, `build.ts`, `package.json`.
12. **프런트엔드.**
    - RED(Bun, happy-dom): `app-view.dom.test.ts` — (a) `availablePresets: ["whisperx"]`이면 Qwen3 라벨이 `hidden`이고 `getComputedStyle().display === "none"`(동점 CSS 방지, 캐시된 DOM 가시성 주의 기록 참고), 장치 그룹이 보이며 WhisperX 배지가 `기본`; (b) mac 상태(`availableDevices: []`)면 장치 그룹이 숨겨지고 두 프리셋이 보임; (c) `cudaDriverDetected: false`면 CUDA 라디오가 `disabled`이고 텍스트 `NVIDIA 드라이버 필요`가 표시; (d) 장치 라디오 변경이 핸들러를 호출. `controller.test.ts` — 장치 전환이 `saveComputeDevice` 후 재진단. `tauri-backend.test.ts` — 신규 필드 Zod 파싱과 `save_compute_device` 호출 인자. `settings-autosave.dom.test.ts`·기존 테스트의 `EnvironmentStatus` 팩토리 갱신(필드 4개). `src/styles.test.ts` 계열 — 폰트 스택에 `"Malgun Gothic"`, `Consolas` 포함.
    - GREEN: `job.ts`, `backend.ts`, `tauri-backend.ts`, `app-template.ts`(문구 4종 + 장치 그룹), `app-view.ts`, `controller.ts`, `styles.css`, `vite.config.ts`. `bun run check`·`bun test` 통과.
13. **아키텍처 펜스.**
    - RED: 임시로 `domain/engine.rs`에 `#[cfg(windows)]` 한 줄을 넣어 새 펜스가 `bun run architecture:check`를 실패시키는지 본 뒤 되돌린다(커밋 대상 아님). 펜스 확장 전에는 같은 삽입이 통과함을 먼저 보인다.
    - GREEN: `check-architecture.ts`의 `forbidden` 확장, 그리고 `checkFrameworkLocality`의 `path.startsWith(`${processAdapterDirectory}/`)`(`check-architecture.ts:113`)를 `node:path`의 `sep` 기반(또는 경로 정규화) 비교로 바꾼다 — Windows에서 `join`이 역슬래시를 만들어 `process/guard/unix.rs`·`nix` 테스트가 위반으로 오탐되고 Windows `frontend` job의 `bun run check`가 실패하는 것을 막는다. 순수 헬퍼로 빼서 `\`/`/` 두 구분자 입력 테스트를 둔다.
    - GREEN(추가): `scripts/verify-platform-fence.py`를 만든다(RED: 없는 상태에서 G15가 실패). `bun run architecture:check`를 실행하고 `src-tauri/src/domain`, `application`, `adapters/inbound`의 `*.rs`에서 `cfg[(](windows|unix)|target_os|cfg![(]`을 검색해 하나도 없을 때만 `PLATFORM_FENCE_OK`를 출력한다.
14. **CI·릴리스 + 피드백 루프.**
    - RED: `uvx --from actionlint-py actionlint .github/workflows/release.yml`이 `secrets` 컨텍스트 오류로 실패(원인 줄 확정; `actionlint-py` 패키지의 실행 파일 이름은 `actionlint`다).
    - GREEN: `ci.yml` 매트릭스와 세 Windows job(`windows-engine`, `bundle-windows`, `windows-install-smoke`), `scripts/ci/windows-install-smoke.ps1`, `release.yml` 수정과 `nsis` job. `uvx --from actionlint-py actionlint .github/workflows/ci.yml .github/workflows/release.yml` 통과.
    - Windows 컴파일 오류는 mac에서 드러나지 않는다. `ci.yml`은 `push: branches: [main]`과 `pull_request`에만 반응하므로(`ci.yml:3-6`) 브랜치만 푸시하면 실행이 생기지 않는다. draft PR은 `issueops remote create-pr`로만 발행하며(`gh pr create` 직접 실행 금지) 이는 phase `pr`을 요구하고, `phase --to pr`는 upstream 존재·동기화·깨끗한 워크트리를 요구하며 게이트 원장(`.issueops/issues/3/gates.md`)에 미충족 게이트가 있으면 `gates_incomplete`로 거부한다. 순서: 로컬 원장 게이트 초록 + 15단계 문서·정리·구현 리뷰 완료 → **커밋 → 브랜치 푸시 → `phase --to pr` → `remote create-pr`**(승인 범위의 종료점이 draft PR + execution complete이므로 범위 안) → 그 PR의 `pull_request` 실행에서 CI Windows job 결과로 수정 커밋을 반복한다. G1·G5·G8·G12a·G17과 G9c의 CI 부분은 원장이 아니라 **execution complete의 전제 조건**이다. 루프 중 임시 `continue-on-error`를 쓸 수 있으나 **최종 커밋에는 남기지 않는다**(G12b가 확인).
15. **문서 (정리·문서 사이클).**
    - RED: `scripts/verify-docs-platform.py`(신규; G13의 새 문구 양성 검사 + 낡은 문구 음성 검사)를 먼저 만들고 실행하면 `PLATFORM_DOCS_FAIL`을 출력한다(기존 "Windows" 언급은 모두 "지원하지 않음"·Accepted Debt 문맥이라 개수 검사는 공회전).
    - GREEN: `README.md`/`README.en.md`(지원 플랫폼 인용문 `:19`, 필수 환경 `:44`, 마이크 권한 `:84`, 개인정보의 비밀 저장 서술 `:163`, 아키텍처 도식 `:175`의 `CoreAudio recorder`, Windows 빠른 시작·개발 명령(PowerShell용 `PYTHONPATH` 예시)·NSIS 빌드·SmartScreen 경고·VC++ 재배포 가능 패키지 선행 조건·CUDA 선택 안내·Windows는 Credential Manager, mac은 설정 파일이라는 비대칭), `.issueops/TECH_STACK.md`(Platform target, Python worker 락 목록, 장치), `.issueops/OPERATIONS.md` 색인 한 줄 + `operations/guides/overview.md`(Prerequisites, Build and release, ARM64 하드코딩 서술), `.issueops/OPEN_API_SPEC.md`·`architecture/overview.md`·`docs/ARCHITECTURE.md`·`AGENTS.md`의 IPC 커맨드 수 18과 플랫폼 분기 규칙(§2에 "플랫폼 분기는 outbound와 composition만, `Os` 값으로 분기" 단락), `outbound/AGENTS.md`의 SIGTERM/그룹 서술에 Windows Job Object 병기, `worker/AGENTS.md`의 장치 정책, `.issueops/testing/overview.md`의 Windows 게이트, 루트 `DESIGN.md`(`:67-68` 폰트 스택을 CSS와 일치, `:238` 접힌 "Accepted Debt" 행을 "Intel macOS/Linux는 여전히 미지원, Windows x64는 서명 없는 NSIS까지"로 갱신), `.issueops/DESIGN.md` 필요 시. ADR 레코드 1건(플랫폼 분기 규칙 + CPU 기본/CUDA 선택 + 락 분리 + Power Request)을 `project_docs_append(kind=adr)`로, 주의 레코드 1건(`env_clear`+`SYSTEMROOT`, `\\?\` 경로, `expect(dead_code)` 린트 함정)을 `kind=caution`으로 추가한다.
16. **검증 사이클.** (a) 원장(`.issueops/issues/3/gates.md`)을 "게이트" 절의 원장 게이트로 만들고 `issueops gates check --write`(네트워크 게이트는 `--network --timeout-seconds 900` 이하)로 채운다 → 문서·정리 봉인·구현 리뷰 → 커밋 → 브랜치 푸시(upstream 설정) → `issueops phase --to pr` → `issueops remote create-pr`로 draft PR 발행. (b) 발행 뒤 CI에서 G1, G5, G8, G12a, G17, G9c의 CI 부분을 확인하고 필요 시 수정 커밋을 반복한다. **발행 뒤 수정 커밋은 변경 fingerprint를 바꿔 ai-slop-clean 봉인·project docs 반영 판정·implementation review를 stale로 만들므로**, 수정 커밋마다 (원장 수정이 있으면 봉인 전에 끝낸 뒤) 봉인 3종을 새 fingerprint로 다시 기록하고 푸시하며, PR 본문이 바뀌면 `issueops remote sync-pr --id io-21fd3b1e1f06 --expected-generation <g>`를 미리보기로 실행한 뒤 같은 명령에 `--confirm`을 붙인다. (c) Windows 실기 수동 게이트(G6, G7b, G8·G9c의 수동 부분)는 실기 관측값을 보고서와 `--verification`에 기록한다. **Windows 실기 결과가 없으면 draft PR에서 멈추고 execution complete 없이 blocked로 보고한다**(미수행 사유를 `Not-tested`에 적고 complete하는 경로는 없다). (d) 순서 고정: ① 보고서(로컬·수동 증거)를 작성 → ② 커밋 → ③ 봉인 3종 재기록 → ④ 푸시 → ⑤ 그 HEAD의 `pull_request` CI가 모두 초록이 될 때까지 대기 → ⑥ 그 run id를 보고서가 아니라 `execution complete --verification`에 기록(G1·G5·G8·G12a·G17의 `gh run list --commit <최종 HEAD>`와 같은 run) → ⑦ `issueops pr-readiness --id io-21fd3b1e1f06 --strict --json`이 ready인지 확인 → ⑧ 그 HEAD로 execution complete.

---

## 게이트

범례: **[원장]** mac arm64 워크트리에서 `issueops gates check`로 판정하는 PR 전 게이트(원장에 기록), **[CI]** PR이 생긴 뒤 GitHub Actions Windows 러너에서만 판정, **[수동-Windows]** Windows 실기에서 사람이 판정(`CHECK: manual`). 원장·CI·수동 게이트가 모두 충족돼야 A번호가 완료다.

**원장 형식 규칙**(`issueops gates check`·`issueops policy check`로 확인한 규칙; 불확실하면 `issueops gates check --help`, `issueops policy check --help`): 게이트 하나당 **argv 명령 하나**이고 따옴표 밖의 `&&`, `|`, `;`, `$(...)`, 백틱, `bash -c`를 쓰지 않는다(연쇄는 `G9a`/`G9b`처럼 쪼갠다). 종료 코드 0과 EXPECT 일치가 **둘 다** 필요하다. EXPECT는 출력의 한 줄 전체, 줄 머리(바로 뒤 공백), 또는 `/정규식/` 중 하나이고 서술형 문장은 쓰지 않는다(EXPECT 줄의 정규식은 토크나이저를 거치지 않으므로 `\d`, `\.` 같은 백슬래시를 쓸 수 있다). 음성 검사·긴 출력은 `python3 -c`가 sentinel 한 줄을 출력하게 감싸고, 정규식이 필요한 검사는 헬퍼 스크립트로 옮긴다. **CHECK argv 토큰 규칙**: (1) 어떤 토큰에도 `secret`·`credentials` 부분 문자열이나 토큰 대입(`token` 뒤에 등호) 꼴을 넣지 않는다(정책 `secret_like_argument` 거부 — 예: 비밀 저장 모듈 이름을 그대로 쓴 테스트 필터는 거부되므로 콜론 없는 부분 문자열 필터 `credential`을 쓴다). (2) 큰따옴표 안에 백슬래시를 쓰지 않는다(정책 토크나이저는 큰따옴표 안 백슬래시를 지우고 다음 글자만 남긴다. 필요하면 문자 클래스 `[(]`, `[+]`를 쓴다). (3) 경로성 인자는 워크스페이스 안만 가리킨다. 네트워크가 필요한 게이트(G3a·G3b·G11·G14a)는 `issueops gates check --write --network --timeout-seconds 900`(15분 초과는 정책이 `timeout_exceeds_15m`로 거부)으로 실행하고, 임시 사본은 워크스페이스 안에서 만들고 지운다. **원장 기록 형식**: 아래 게이트 줄(`G2 [원장]: … | CHECK: … | EXPECT: …`)은 계획용 약식이다. 원장 파일에는 대괄호 태그를 빼고 게이트 ID를 공백 없는 첫 토큰으로 하여 `- [ ] G2: <결과>` 아래에 들여쓴 `CHECK: <argv>` / `EXPECT: <기대>` / `EVIDENCE: pending` 세 줄로 직접 쓴다(`gates init --gate`는 spec을 `|`로 자르므로 `|`가 들어간 게이트에는 쓰지 않는다). 원장은 워크트리 안 `.issueops/issues/3/gates.md`이고 봉인 전에 끝낸다. 실행 예: `issueops gates check --file .issueops/issues/3/gates.md --cwd . --workspace-root . --write --json`. 자기검증: 각 게이트 argv를 `issueops policy check --write --json [--network] -- <argv…>`에 넣어 `allowed=true`를 확인했다(G9a의 이전 필터(비밀 저장 모듈 이름)만 `secret_like_argument`로 거부 → 콜론 없는 `credential` 필터로 교체; 나머지 원장 게이트 전부 `allowed=true`).

**원장에는 PR 전에 판정 가능한 게이트(G2, G3a, G3b, G4, G7a, G9a, G9b, G10a, G10b, G11, G12b, G13, G14a, G15, G16)만** 넣는다. `[CI]` 게이트(G1, G5, G8, G12a, G17)와 G9c의 CI 부분, `[수동-Windows]` 게이트(G6, G7b, G9c와 각 수동 부분)는 PR이 있어야 증거가 생기므로 원장에 넣으면 `phase --to pr`가 `gates_incomplete`로 막혀 교착한다. 이들은 execution complete의 전제 조건이다.

### 원장 게이트

G2 [원장]: A1 — 사이드카 타깃 테이블·Windows uv 해시·빌드 분기·Rust/TS 상수가 일치한다 | CHECK: bun test scripts/sidecar-targets.test.ts scripts/build.test.ts | EXPECT: /^\s*0 fail$/m

G3a [원장,네트워크]: A1/A2 — Windows CPU 락이 생성 때와 같은 cwd·인자로 재생성해도 바이트 동일하다 | CHECK: python3 scripts/verify-windows-lock.py cpu | EXPECT: LOCK_REPRODUCIBLE cpu

G3b [원장,네트워크]: A2 — Windows CUDA 락이 같은 방식으로 재현된다 | CHECK: python3 scripts/verify-windows-lock.py cuda | EXPECT: LOCK_REPRODUCIBLE cuda

G4 [원장]: A2 — CUDA 락이 `torch==2.8.0+cu128`을 고정한다 | CHECK: grep -n ^torch==2.8.0+cu128 worker/requirements-windows-cuda.lock | EXPECT: /^\d+:torch==2\.8\.0\+cu128 /m

G7a [원장]: A3 — 녹음 취소가 `.wav.part`와 빈 폴더를 남기지 않는다 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets cancelling_a_recording_leaves_no_partial_file_or_empty_folder | EXPECT: /test result: ok\. 1 passed/

G9a [원장]: A5 — 비밀 저장소 순수부(`secrets/credential.rs`의 `credential_target`, 블롭 인코딩·한도)가 통과한다(테스트 모듈 경로가 `credential` 부분 문자열로 필터됨) | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets credential | EXPECT: /test result: ok\. [1-9]\d* passed/

G9b [원장]: A5 — 재시작 후 '저장됨' 유지(`the_assistant_key_survives_a_relaunch`, `settings.rs:696`)와 비밀 평문 비저장 테스트가 통과한다 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets settings:: | EXPECT: /test result: ok\. [1-9]\d* passed/

G10a [원장]: A6 — Windows에서 Qwen3가 숨겨지고 WhisperX의 CPU/CUDA 선택만 보이며 CUDA는 드라이버 없으면 비활성이다 | CHECK: bun test src/ui/app-view.dom.test.ts src/ui/controller.test.ts src/adapters/tauri-backend.test.ts src/ui/settings-autosave.dom.test.ts | EXPECT: /^\s*0 fail$/m

G10b [원장]: A6 — 설정 어댑터가 `Os::Windows` 주입 시 Qwen3 저장을 거부하고 WhisperX를 기본값으로 낸다(테스트 이름은 `os_windows_` 접두) | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets os_windows_ | EXPECT: /test result: ok\. [1-9]\d* passed/

G11 [원장,네트워크]: A7 — macOS `check:all`이 변경 전과 같은 결과(통과)를 낸다. 변경 전 `main`에서도 통과함을 구현 시작 시 한 번 기록해 기준으로 쓴다 | CHECK: python3 -c "import subprocess,sys; r=subprocess.run(['bun','run','check:all'],capture_output=True); print('CHECK_ALL_PASS' if r.returncode==0 else 'CHECK_ALL_FAIL'); sys.exit(r.returncode)" | EXPECT: CHECK_ALL_PASS

G12b [원장]: A7 — 최종 워크플로 파일에 임시 `continue-on-error`가 남아 있지 않다 | CHECK: python3 -c "import pathlib; hits=[p for p in ['.github/workflows/ci.yml','.github/workflows/release.yml'] if 'continue-on-error' in pathlib.Path(p).read_text()]; print('NO_CONTINUE_ON_ERROR' if not hits else 'FOUND '+','.join(hits))" | EXPECT: NO_CONTINUE_ON_ERROR

G13 [원장]: A8 — 지원 플랫폼 서술이 새 문구(`Windows 10/11 x64`, 루트 `DESIGN.md`는 `Windows x64`, `.issueops/OPERATIONS.md`는 `Windows`)를 담고 낡은 단정 문구가 없다(기준 시점에 `DESIGN.md:238`, `README.md:20`, `README.en.md:20`이 이미 "Windows"를 담고 있어 단순 개수 검사는 공회전하므로 양성·음성을 함께 쓴다) | CHECK: python3 scripts/verify-docs-platform.py | EXPECT: PLATFORM_DOCS_OK (헬퍼의 음성 패턴: `Apple Silicon only`, `Intel Mac, Windows, Linux는 지원하지 않습니다`, `Intel Macs, Windows, and Linux are not supported`, `Add signed Intel/Windows packages`, `macOS 14+ on Apple Silicon;`(정규식 `14[+]`로 작성), `hardcode ARM64`)

G14a [원장,네트워크]: A7 — 워크플로 문법이 유효하다(`release.yml`의 `secrets` 컨텍스트 오류 해소 포함) | CHECK: python3 -c "import subprocess,sys; r=subprocess.run(['uvx','--from','actionlint-py','actionlint','.github/workflows/ci.yml','.github/workflows/release.yml'],capture_output=True,text=True); print('ACTIONLINT_OK' if r.returncode==0 and not r.stdout.strip() else 'ACTIONLINT_FAIL '+r.stdout[:300]); sys.exit(r.returncode)" | EXPECT: ACTIONLINT_OK

G15 [원장]: 플랫폼 분기가 `composition.rs`와 outbound에만 있다(펜스 통과 + `domain`·`application`·`adapters/inbound`에 `cfg` 흔적 없음) | CHECK: python3 scripts/verify-platform-fence.py | EXPECT: PLATFORM_FENCE_OK

G16 [원장]: 프런트 번들이 새 하한(`chrome120`)으로도 빌드된다(변경 전후 `dist/` 총 바이트는 보고서에 기록) | CHECK: bun run vite:build | EXPECT: /built in/

### CI 게이트 (execution complete 전제, 원장 아님)

모든 CI 게이트는 **최종 HEAD SHA**에서 판정한다. `gh run list --branch 3-windows-x64-support --workflow ci.yml --commit <최종 HEAD SHA> --limit 1 --json databaseId -q '.[0].databaseId'`(`-c, --commit` 지원)로 얻은 `<RUN_ID>` 하나를 G1·G5·G8·G12a·G17이 공유하고, 같은 값을 `execution complete --verification`에 적는다.

G1 [CI]: A1 — Windows 러너에서 `bun run build`가 NSIS 설치본을 만든다 | CHECK: gh run view <RUN_ID> --json jobs -q '.jobs[] | select(.name=="bundle-windows") | .conclusion' | EXPECT: `success` (이 job의 마지막 검사 단계가 `src-tauri/target/release/bundle/nsis/*.exe` 존재를 단언하고 `galpi-nsis` 아티팩트를 올린다)

G5 [CI]: A2 — Windows에서 CPU 락 설치 + WhisperX/torch/pyannote import 스모크 + `env_clear` 상태 워커 환경 스모크 + CUDA 락 해석 | CHECK: gh run view <RUN_ID> --json jobs -q '.jobs[] | select(.name=="windows-engine") | .conclusion' | EXPECT: `success`

G8 [CI+수동]: A4 — 전사 취소가 워커와 자식 프로세스 트리를 모두 종료한다 | CHECK: gh run view <RUN_ID> --json jobs -q '.jobs[] | select(.name | startswith("rust")) | select(.name | contains("windows")) | .conclusion' | EXPECT: `success` (이 job의 `cargo test --target x86_64-pc-windows-msvc`에 Job Object 트리 종료 테스트 포함; mac에서는 같은 취지의 unix 손자 프로세스 테스트가 `cargo test` 로컬 실행에 포함). 수동 부분: manual — Windows 실기에서 전사를 시작해 `python.exe`가 작업 관리자에 보이는 동안 앱의 취소를 누르고 `python.exe`(와 `ffmpeg.exe`/`uv.exe` 자식)가 목록에서 사라지는지 확인

G12a [CI]: A7 — CI 매트릭스의 모든 job(Windows 포함)이 통과한다 | CHECK: gh run view <RUN_ID> --json jobs -q '[.jobs[] | select(.conclusion != "success")] | length' | EXPECT: `0`

G17 [CI]: A2 — NSIS 설치본을 무인(`/S`) 설치하고, 설치된 사이드카 경로(`<설치 폴더>\uv.exe`, `<설치 폴더>\resources\worker`)가 존재하며, 설치된 `uv.exe`로 CPU 락 환경을 만들어 `import whisperx, torch, pyannote.audio`와 `python -m galpi_worker --help`가 통과한다 | CHECK: gh run view <RUN_ID> --json jobs -q '.jobs[] | select(.name=="windows-install-smoke") | .conclusion' | EXPECT: `success` (job 정의는 설계 §9. 시크릿·샘플 오디오 불필요. 전사 3파일·GUI 진단·녹음은 이 job이 다루지 않으므로 G6·G7b 수동에 남는다)

### 수동 게이트 (Windows 실기, execution complete 전제, 원장 아님)

G6 [수동-Windows]: A2 — 설치본 실행, GUI 환경 진단 통과, WhisperX로 한국어 샘플 오디오 전사, 결과 3파일 생성(전사 3파일의 유일한 증명) | CHECK: manual — 깨끗한 Windows 10 또는 11 x64에서 CI 아티팩트 `galpi-nsis`의 설치본을 실행(SmartScreen "추가 정보 → 실행"), 설정에서 전사 엔진이 WhisperX 하나만 보이고 장치는 CPU로 시작하는지 확인, 설정에 Hugging Face 토큰을 저장하고 준비 패널의 엔진·모델·ffmpeg 준비를 끝낸 뒤 짧은 한국어 샘플 오디오를 전사하고 `File Explorer`에서 출력 폴더의 `<이름>.srt`, `<이름>_화자별.txt`, `<이름>.aligned.v2.json` 세 파일 존재를 확인 | EXPECT: 진단 3항목(엔진·모델·내장 ffmpeg) 모두 준비됨, 세 파일 존재, `<이름>.srt`가 비어 있지 않음. 관측값(날짜·Windows 빌드 번호·결과)을 보고서와 `--verification`에 기록

G7b [수동-Windows]: A3 — Windows 마이크 녹음 시작→중지, 시작→취소 | CHECK: manual — 앱의 `마이크로 바로 녹음`을 시작→중지(WAV 생성), 시작→취소를 수행하고 출력 폴더에 `.wav.part`가 없고 취소한 세션의 폴더가 비어 있으면 삭제됐는지 확인(Windows 설정의 "데스크톱 앱의 마이크 접근"이 꺼져 있으면 `MICROPHONE_PERMISSION_DENIED` 안내가 나오는 것은 정상 경로이므로 켜고 재시도) | EXPECT: WAV 1개 생성, 취소 뒤 `.wav.part` 0개

G9c [CI+수동]: A5 — Windows 실저장소 왕복과 재시작 유지 | CHECK: CI 부분은 G8과 같은 `rust (windows)` job의 `cargo test`에서 `#[cfg(windows)]` 실저장소 왕복 테스트 통과. 수동 부분: manual — 설정 시트에서 HF 토큰과 assistant API 키를 저장 → `control /name Microsoft.CredentialManager` → "Windows 자격 증명"에 `com.m16khb.galpi:hugging-face-token`, `com.m16khb.galpi:assistant-api-key` 항목 확인 → 앱 종료 후 재실행해 설정 시트가 "API 키 저장됨"을 표시하는지, `%LOCALAPPDATA%\com.m16khb.galpi\settings.json`에 키 원문이 없는지, `icacls`로 해당 폴더가 현재 사용자 전용 접근인지 확인 | EXPECT: 두 항목이 보이고 재시작 뒤 "API 키 저장됨", 설정 파일에 원문 없음, ACL이 현재 사용자·SYSTEM·Administrators로 제한

---

## 검증 명령

게이트에 이미 있는 명령(`bun run check:all`, `bun run vite:build`, `uvx --from actionlint-py actionlint`, 락 재생성, 개별 테스트 필터)은 반복하지 않는다.

- 시작 시 기준선 기록: 변경 전 `main`에서 `bun run check:all`을 한 번 실행해 G11의 비교 기준(통과 여부와 테스트 수)을 PR 본문 초안에 적어 둔다.
- 선택적 로컬 Windows 컴파일 빠른 루프(게이트 아님, CI가 권위): `rustup target add x86_64-pc-windows-msvc`; `brew install llvm`; `export RC_x86_64_pc_windows_msvc=/opt/homebrew/opt/llvm/bin/llvm-rc`(`embed-resource`가 읽는 변수, `embed-resource-3.0.11/src/lib.rs:641-645`); `GALPI_SIDECAR_TARGET=x86_64-pc-windows-msvc bun run sidecar:stage`; `cargo clippy --manifest-path src-tauri/Cargo.toml --lib --target x86_64-pc-windows-msvc -- -D warnings`. `cc` 크레이트가 msvc 타깃용 C 전처리기(`clang-cl`)를 요구할 수 있어 `미확인 가정:` 동작 여부는 구현자가 시도해 보고, 안 되면 건너뛰고 CI 루프로 간다. 스테이징으로 만든 `binaries/uv-x86_64-pc-windows-msvc.exe`는 `.gitignore`(`src-tauri/binaries/`) 대상이라 커밋되지 않는다.
- `uvx basedpyright`(strict)는 설치된 WhisperX venv가 있어야 의미가 있다. 이 mac에는 앱 venv가 없고(`~/Library/Application Support/com.m16khb.galpi/engine/.venv` 부재) CI 워커 job도 실행하지 않는다. 따라서 이번 이슈에서는 실행하지 않고 PR `Not-tested`에 "basedpyright 미실행(venv 없음)"으로 적는다. `runtime.py` 변경은 타입 주석을 보수적으로(`Literal`, 키워드 전용 인자) 유지한다.
- 푸시 후 draft PR을 열고 `gh pr checks --watch`(또는 `gh run watch`)로 CI 루프를 관찰하며, 수정 커밋 때마다 G1·G5·G8·G9c(CI 부분)·G12a·G17을 다시 평가한다.
- `git status`로 생성물(`src-tauri/binaries`, `src-tauri/resources/worker`, `dist`)이 커밋 대상에 섞이지 않았는지 확인한다.

---

## 위험과 열린 질문

열린 질문은 0개다. 아래는 각각 결정과 근거다.

**결정 (이전에 질문처럼 보였던 것들)**

1. **CPU 기본 + CUDA(`cu128`) 선택 설치.** 실측 크기(241 MB vs ≈3.46 GB)와 위 근거로 확정. 사용자 확인이 필요한 사항 아님.
2. **Windows 전원 억제는 `SetThreadExecutionState`가 아니라 Power Request.** 스레드 바인딩 때문에 `spawn_blocking` 풀 구조와 맞지 않음(`recording/mod.rs:61,151,159`). 완료 기준에 영향 없음.
3. **Windows "정중한 종료" = 즉시 `TerminateJobObject`.** 콘솔 없는 자식에게 그룹 단위 SIGTERM에 해당하는 신호가 없고, 워커 산출물 게시는 임시 파일+원자적 교체라 안전하다. 3초 대기 구조는 코드 공용 유지(Windows에서는 즉시 끝남).
4. **ASR은 모든 플랫폼에서 CPU int8.** `worker/AGENTS.md` 원칙. CUDA의 이득은 정렬·화자분리에 한정된다 — README에 명시한다.
5. **Windows에서 저장된 Qwen3 값은 WhisperX로 정정.** 에러로 막으면 설정 파일을 복사해 온 사용자가 앱을 못 쓰게 된다.
6. **로컬 mac에서 Windows 타깃 컴파일은 게이트로 삼지 않음.** `llvm-rc`·타깃·스테이징이 모두 필요하고 C 전처리기 동작이 미확인이기 때문. 권위는 CI Windows job. 대신 Windows 전용 코드를 세 개의 작은 `#[cfg(windows)]` FFI 파일로 격리하고 로직은 전부 순수 함수로 mac에서 테스트한다.
7. **Windows CI는 `bundle-windows`를 PR마다 돌린다**(LTO fat 릴리스 빌드 포함, 시간 부담이 있으나 A1을 재현 가능한 증거로 만드는 가장 저렴한 방법). mac에는 대응 job이 없는 비대칭은 의도적이다.
8. **`README.en.md`도 갱신.** 모순 방지.
9. **VC++ 재배포 가능 패키지는 번들하지 않는다.** NSIS 설치 훅으로 설치하는 방안은 별도 범위이며 README 선행 조건(`Microsoft Visual C++ 2015–2022 x64 재배포 가능 패키지`)과 실기 게이트 G6의 "깨끗한 Windows"에서 필요성을 관찰한다. 필요하다고 판명되면 후속 이슈로 기록하며 이 이슈의 완료 기준에는 영향 없음.
10. **macOS 릴리스 `dmg` job의 `tauri-cli` 미설치는 건드리지 않는다.** 기존 워크플로가 무효라 한 번도 실행된 적이 없어(`gh run view 37789731837`) 실제로 실패하는지 알 수 없다(`미확인 가정:`). 이 이슈는 유효성 복구와 Windows job 추가까지만 하고 mac 릴리스 단계 변경은 하지 않는다.
11. **예약 장치 이름(`CON`, `NUL` …)으로 된 오디오 파일명은 이번에 처리하지 않는다.** `sanitize_name`은 이미 금지 문자를 치환하며(`paths.rs:238-250`), 예약 이름 충돌은 확률이 낮고 별도 규칙이 필요한 별개 문제다.

**위험 (문서 제약과 연결)**

1. *헥사고날 ADR의 안쪽 의존 규칙*: 플랫폼 분기가 `domain/application`으로 새면 설계 전제가 깨진다 → G15가 `check-architecture.ts` 펜스와 grep으로 강제한다. `DesktopAdapter`의 4포트를 쪼개고 싶은 유혹이 생겨도 §6의 "의도적으로 남겨둔 것"을 따른다.
2. *`outbound/AGENTS.md`의 "bare PID 신호 금지, 임의 환경 변수 추가 금지"*: Windows 환경 키(`SYSTEMROOT` 등)와 Job Object를 각각 `platform.rs`와 `ProcessGroupGuard` 한 곳에 가둔다. 필수 키 집합이 틀리면 Python이 기동하지 못하므로 CI의 `windows_worker_environment_starts_python`(`windows-engine` job, `env_clear` 상태에서 실제 `worker_environment(Os::Windows)`로 import 스모크)이 첫 증거이고 G6가 두 번째다.
3. *`worker/AGENTS.md`의 단일 폴백·stdout 순수성*: CUDA 폴백이 "가속기 → CPU 1회"를 넘지 않게 하고, Windows UTF-8(`PYTHONUTF8=1`)과 CRLF 출력에서도 한 줄 JSON이 깨지지 않는지 확인한다(Rust 읽기 루프가 이미 `\r`을 제거한다: `process.rs:259-261`).
4. *Cargo lint(`unwrap`/`expect`/`panic` 거부, 미충족 `expect` 오류)*: Windows FFI와 `#[expect(dead_code)]` 조건 변경에서 기계적으로 터지기 쉽다. 이미 알려진 지점(`secrets.rs:17-18,28-32`)은 설계에 반영했고, 나머지는 Windows CI 린트가 잡는다.
5. *DOM 가시성 주의 기록*: `hidden`만 보면 속는다. CSS 동점(`html [hidden]` vs `.engine-segmented`)을 계산 스타일 테스트로 고정한다.
6. *CONSTITUTION의 비밀 비노출*: Credential Manager 테스트가 실제 사용자 자격 증명 저장소를 쓰므로 테스트 전용 서비스 접두와 반드시 삭제하는 정리 코드를 쓴다. CI 로그·테스트 이름에 키 값이 나오지 않게 한다.
7. *Windows 전용 경로·테스트 가정*: `application/tests.rs:563`처럼 문자열 리터럴 `"/tmp/output/job/meeting_회의록.md"`와 `String` 비교하는 단언은 Windows에서 구분자가 `\`로 바뀔 수 있어 실패할 수 있다(`미확인 가정:` Windows에서 `Path::with_file_name`이 만드는 구분자). CI가 드러내면 기대값을 `Path` 조인으로 만든다. 비슷한 지점은 `application/tests.rs:338,496,591,622-687`.
8. *이슈 문장에 없는 사용자 영향*: 서명 없는 NSIS는 SmartScreen이 막을 수 있다(범위 밖, README에 우회 방법 안내). 첫 엔진 준비는 CPU 선택 시에도 수 GB 규모의 모델·휠 다운로드가 있다(예상 시간은 쓰지 않는다). Windows의 긴 경로(260자) 한계가 깊은 `site-packages`에서 문제될 수 있으나 앱 데이터 경로가 짧아(`%LOCALAPPDATA%\com.m16khb.galpi\engine\.venv`) 여유가 있다는 점은 `미확인 가정:`이며 G6의 설치 성공으로 확인한다.
9. *CI 피드백 루프가 길다*: Windows 전용 코드는 mac에서 컴파일되지 않으므로 첫 푸시에서 컴파일 오류가 여러 건 나올 수 있다. `ci.yml`은 브랜치 푸시에 반응하지 않으므로(`ci.yml:3-6`) 커밋·푸시·`phase --to pr` 뒤 `remote create-pr`로 draft PR을 발행해 `pull_request` 실행에서 반복한다(draft PR은 승인 범위의 종료점이므로 범위 안). 완화: FFI 파일을 작게 유지, 로직은 순수 함수로 분리, 선택적 로컬 교차 검사 시도. **Windows 실기 결과가 없으면 draft PR에서 멈추고 execution complete 없이 blocked로 보고한다.** CI 게이트(G1·G5·G8·G12a·G17)가 최종 HEAD에서 초록이어도 수동 게이트가 비어 있으면 완료가 아니다.
10. *Windows에는 `0600`이 없다*: `settings.json`(`settings.rs:362`)과 워커 컨텍스트 임시 파일(`refinement.rs:181`, 참석자 명단·배경 정보)의 소유자 전용 권한 보장이 Windows에서는 `%LOCALAPPDATA%`·`%TEMP%`의 상속 ACL에 의존한다(`미확인 가정:` 기본 ACL이 현재 사용자·SYSTEM·Administrators로 제한됨). 비밀(HF 토큰·API 키)은 Windows에서 Credential Manager로 가므로 `settings.json`에 남지 않아 노출 면이 mac(평문 0600)보다 작지만, 참석자·용어집 등 비밀이 아닌 개인정보는 여전히 이 파일들에 있다. 완화: `#[cfg(unix)]`로 `mode`/`set_permissions`만 격리하고 Windows는 명시적 ACL 코드를 새로 만들지 않으며(범위 밖), G9c 수동 부분에서 `icacls`로 앱 데이터 폴더와 임시 파일의 ACL을 확인해 PR 본문에 결과를 기록한다. 임시 컨텍스트 파일은 `create_new`로만 만들고 사용 후 삭제하는 기존 동작을 유지한다.
11. *완료 기준 5와 현 macOS 사실의 불일치 처리 결정*: macOS는 Keychain을 쓰지 않는다(`secrets.rs:1-9`, `Cargo.toml:22`). 이슈 계약(A5: Windows Credential Manager 저장·재시작 유지)을 좁히지 않기 위해 Windows는 `CredentialManager`를 실제로 연결하고(비휴면), macOS는 `SettingsFile` 유지 + `Keychain`/`security-framework`를 `target_os = "macos"`로 게이팅(휴면 유지)한다. "Windows도 SettingsFile 유지"안은 A5를 충족하지 못해 기각.
