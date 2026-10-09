# Galpi 아키텍처 원칙 — DDD · 헥사고날 · 클린 · OOP · SOLID

- 작성일: 2026-08-21
- 기준 커밋: `9ed5df5` (main)
- 문서 목적: 프로젝트 전체의 아키텍처 스타일과 그 근거를 코드 위치와 함께 확정하여,
  이후 모든 변경이 같은 원칙 아래에서 이루어지게 한다. `AGENTS.md`와
  `scripts/check-architecture.ts`가 이 문서를 실행 가능한 형태로 보강한다.

---

## 1. 전체 그림: 세 런타임, 하나의 의존성 규칙

Galpi는 세 개의 런타임으로 구성된다. 각각 내부적으로 같은 의존성 규칙을 따른다.

```text
TypeScript (WebView)          Rust (Tauri 호스트)              Python (WhisperX 사이드카)
┌────────────────────┐        ┌──────────────────────────┐     ┌────────────────────────┐
│ ui/        (외부)   │        │ adapters/inbound  (외부)  │     │ __main__.py  (외부)     │
│ ├─ controller       │  IPC   │ ├─ tauri.rs (24 커맨드)   │     │ engine.py    (유스케이스)│
│ ├─ app-view         │ ─────▶ │ └─ TauriEvents            │     │ refine.py             │
│ └─ settings/…       │        │                          │     │ protocol.py (포트: stdout)│
├────────────────────┤        │ application/  (유스케이스) │     ├────────────────────────┤
│ application/        │        │ ├─ use_cases.rs (facade)  │     │ domain 순수 모듈         │
│ ├─ job-machine      │        │ ├─ ports.rs (13개 포트)    │     │ ├─ core.py            │
│ └─ recording-machine│        │ └─ jobs.rs (registry)      │     │ ├─ artifacts.py       │
├────────────────────┤        ├──────────────────────────┤     │ ├─ minutes_*.py       │
│ domain/            │        │ domain/        (내부)      │     │ └─ assistant_stream.py │
│ ├─ job, speaker     │        │ ├─ job, artifact, worker  │     └────────────────────────┘
│ ├─ participant      │        │                          │
│ └─ glossary         │        ├──────────────────────────┤     JSONL 프로토콜 v1
└────────────────────┘        │ adapters/outbound (외부)   │ ◀─ ──────────────────────
                              │ ├─ desktop, setup          │     (v, seq, type별 이벤트)
┌────────────────────┐        │ ├─ process, recording      │
│ adapters/ (외부)     │        │ └─ settings, paths …      │
│ └─ tauri-backend    │        └──────────────────────────┘
│    (Zod 파싱 경계)   │                  │
└────────────────────┘        ┌──────────────────────────┐
                              │ composition.rs (조합 루트) │
                              │ 구현체 생성 + Tauri 등록    │
                              └──────────────────────────┘
```

**하나의 의존성 규칙** (Clean Architecture의 핵심): 모든 의존성은 안쪽(도메인)을 향한다.
- `domain`은 아무것도 import하지 않는다 (TS: 프로젝트 내부 모듈만, Rust: 표준 라이브러리와 serde/thiserror/uuid만).
- `application`은 `domain`만 안다. 포트(trait/interface)로 외부 능력을 선언한다.
- `adapters`는 포트를 구현한다. 프레임워크(Tauri, Zod, CPAL, tokio::process)는 여기에만 산다.
- `composition`/`main`만이 구현체를 생성하고 연결한다.

## 2. 헥사고날 (Ports & Adapters) — 경계의 정의

| 방향 | Rust | TypeScript | Python |
|------|------|-----------|--------|
| **-driving (inbound)** | `adapters/inbound/tauri.rs` — 24개 `#[tauri::command]`(`save_compute_device` 포함, `composition.rs`의 `generate_handler!`와 일치) + `TauriEvents` 이벤트 브리지(`job-event`·`recording-event`·`chatgpt-event`) | `adapters/tauri-backend.ts` — `BackendPort` 구현 + Zod 스키마 검증 | `__main__.py` CLI 인자 파싱 |
| **-driven (outbound)** | `DesktopAdapter`(엔진·전사·산출물), `NativeRecorder`(CPAL), `LocalSettingsStore`(`ChatGptStore` 포함), `ChatGptOAuthAdapter`(`ChatGptAuthPort`: OAuth/PKCE·토큰 갱신·폐기·모델 목록), `SystemClock`(`ClockPort`), `process.rs`(워커 감독) | (없음 — 프론트엔드는 driven 포트가 없다; 브라우저 API는 어댑터 내부 처리) | `protocol.py` `EventWriter`(stdout), `assistant_stream.py`·`responses_stream.py`(HTTP) |

**포트 소유 규칙 (DIP)**: 포트 인터페이스는 사용하는 쪽(내부 계층)이 소유하고,
어댑터가 그것을 구현한다. Rust는 `application/ports.rs`에 13개 trait이 있어 정확히 지켜진다.
TypeScript는 `BackendPort`가 **어댑터 모듈에** 정의되어 있어 위반이었다 — 리팩토링으로
`domain/backend.ts`로 옮겼다(§6). 프론트엔드의 `ui → adapters` 의존은 이제 타입 재수출
경로(`domain/backend`)로만 존재한다.

**이벤트는 포트로만 흐른다**: Rust→TS 이벤트(`job-event`, `recording-event`)는
`JobEvents`/`RecordingEvents` 포트를 통해 발행되고, TS는 구독을 `BackendPort.listenToJobs`
뒤에 숨긴다. UI 컨트롤러는 Tauri 타입(`UnlistenFn`)을 몰라야 한다.

**플랫폼 분기는 outbound 어댑터와 `composition.rs`에만 둔다.** outbound 코드는
`adapters/outbound/platform.rs`의 `Os` 값으로 분기하며, `Os::current()`가 유일한
`cfg!(windows)`다. `#[cfg]`는 다른 플랫폼에서 컴파일할 수 없는 코드에만 쓴다: Win32
FFI(`process/guard/windows.rs`, `recording/power/windows.rs`,
`secrets/credential_manager.rs`)와 `nix`(`process/guard/unix.rs`).
`scripts/check-architecture.ts`는 domain/application/inbound에서 `cfg(windows)`,
`cfg(unix)`, `target_os`, `cfg!(`를 금지한다.

## 3. DDD 관점 — 전술적 패턴 매핑

이 프로젝트는 전략적 DDD(바운디드 컨텍스트 매핑)보다 **전술적 패턴**이 실질이다.
단일 제품(회의 전사)이므로 컨텍스트는 하나이고, 내부 계층만 엄격하게 지킨다.

| DDD 요소 | Galpi 구현 | 규칙 |
|----------|-----------|------|
| **값 객체 (Value Object)** | `SpeakerHint`(TS/Rust), `Participant`, `GlossaryEntry`, `AssistantSettings`, `Artifacts`, `JobViewState`, `RecordingViewState` | 불변(`readonly`/`Clone`), 검증 포함(`validate_speaker_hint`, `buildSpeakerHint`, `trimmed()`). Rust 값 객체는 `domain/`에 위치한다 — `application/model.rs`에 두지 않는다 |
| **애그리게이트 루트** | `Artifacts`(작업 산출물 집합; `minutes`는 선택적 파생) | 산출물 경로 접근은 `Artifacts::path_for(kind)`로만 |
| **도메인 서비스** | `minutes_path`(파생 이름 규칙), `pack_asr_hotwords`(워커), `filter_segments`(워커) | 순수 함수, 프레임워크 불가 |
| **레포지터리 (개념적)** | `JobRegistry`(활성 작업 + 산출물 맵), `active_recording` 뮤텍스 | 저장소 추상화는 인메모리로 충분; 포트로 분리하지 않는다(과잉 추상화 방지) |
| **유스케이스 (애플리케이션 서비스)** | Rust `Application` 메서드 13개, TS `AppController` 오케스트레이션 | 하나의 유스케이스 = 하나의 퍼블릭 메서드; 포트 조합으로 구성 |
| **도메인 이벤트** | `WorkerEvent` 6종(`phase`/`completed`/`prepared`/`refined`/`error`/`log`), `RecordingFailure` | 이벤트 모양은 `worker/galpi_worker/protocol.py` ↔ `src-tauri/src/domain/worker.rs` ↔ `src/domain/job.ts` ↔ Zod 스키마가 **한 변경 세트**를 이룬다 |

**도메인 순수성 기준**: 도메인 모듈이 serde 어노테이션을 갖는 것은 허용한다(직렬화가
계약의 일부이며 serde는 프레임워크가 아니라 데이터 라이브러리다). Tauri, tokio, Zod,
CPAL은 도메인에 금지다.

## 4. SOLID 매핑 (관례로 강제)

| 원칙 | Galpi에서의 형태 | 위반 시 |
|------|-----------------|--------|
| **S**RP | 유스케이스 메서드는 하나의 사용자 의도만; `process.rs`는 감독만, `writer.rs`는 WAV 직렬화만; TS 설정 위젯은 화면 하나씩 | 250 LOC 순수 코드 상향선 초과 시 모듈 분리 |
| **O**CP | 이벤트/요청은 tagged union + `switch`/`match`로 확장 — 새 이벤트 추가는 새 variant 추가와 전 매치 컴파일 오류로 유도 (Rust `match`는 exhaustive) | `default:` 분기로 확장을 흡수하지 않는다 |
| **L**SP | 포트 구현체(`FakePort`, `TauriBackend`, `DesktopAdapter`)는 계약(오류 코드, 이벤트 순서)을 지킨다 | 테스트 페이크가 프로덕션 계약과 어긋나면 페이크를 고친다 |
| **I**SP | Rust 포트 13개는 소비자별 분리(`EnginePort` ≠ `ArtifactPort` ≠ `SettingsPort` ≠ `ChatGptAuthPort`…); TS `BackendPort`는 단일 소비자(controller)를 위한 하나의 응집된 계약 | 컨트롤러가 사용하지 않는 메서드를 포트에 추가하지 않는다 |
| **D**IP | 내부 계층은 포트만 안다: Rust `Arc<dyn Trait>`, TS `BackendPort` 타입 주입, 워커는 `EventWriter` 주입 | `ui/`가 `@tauri-apps/*` 또는 `adapters/` 구현체를 import하면 `check-architecture.ts`가 실패한다 |

## 5. OOP 구조 관례

Rust는 전통적 상속이 없으므로 OOP 원칙은 **trait + 조합**으로 번역된다:

- **캡슐화**: `JobRegistry.active`/`artifacts`는 private + `Mutex`; `AppController`의
  상태(`job`, `lastResult`)는 private 필드. 상태 변경은 반드시 소유 객체의 메서드/리듀서를 거친다.
- **다형성**: 포트 trait의 동적 디스패치(`Arc<dyn EnginePort>`) — 테스트는 `FakePort`로
  같은 trait을 구현해 교체한다 (composition root에서만 갈림).
- **상속 대신 조합**: `DesktopAdapter`는 4개 포트를 하나의 객체가 구현하되, 각 포트는
  별도 trait으로 소비자별 계약을 유지한다 (ISP).
- **불변 상태 전이**: TS 상태 머신(`job-machine.ts`, `recording-machine.ts`)은
  `(state, event) → state` 순수 리듀서. 컨트롤러는 리듀서 결과를 뷰에 반영만 한다.

## 6. 발견된 위반과 시정 (2026-08-21 리팩토링)

| # | 위반 | 원칙 | 시정 |
|---|------|------|------|
| 1 | `BackendPort`, `SetupResult`, `ArtifactKind`, `RecordingStatus/Result/Failure`, `errorMessage`/`errorDetail`이 `adapters/tauri-backend.ts`에 정의되어 `ui/`, `application/`이 어댑터 모듈을 import | DIP | 계약을 `src/domain/backend.ts`로 이동. `tauri-backend.ts`는 Zod 스키마와 `TauriBackend` 구현만 남김 |
| 2 | `src/ui/controller.ts`가 `@tauri-apps/api/event`의 `UnlistenFn` 타입을 직전 import | 프레임워크 격리 | `BackendPort`가 `() => void` 언리스너를 반환하도록 포트 계약을 프레임워크 중립화 |
| 3 | `check-architecture.ts`가 프론트엔드 펜스를 검사하지 않음 | 경계 강제 | TS 펜스 추가: `domain`↔`application` 순수성, `ui`의 `adapters` 구현체/`@tauri-apps` import 금지 (타입 재수출 경유만 허용) |
| 4 | Rust `use_cases.rs::asr_context`가 `serde_json::json!`으로 워커 와이어 포맷을 생성 | 프로토콜 계약 소재 | 포맷 생성을 `domain/worker.rs::AsrContext::into_wire_json`으로 이동 — 계약이 Rust 파서와 같은 모듈에 |
| 5 | `Participant`/`GlossaryEntry`/`AssistantSettings` 값 객체 + `trimmed()` 규칙이 `application/model.rs`에 위치 | DDD 값 객체 소재 | `domain/roster.rs`로 이동; `model.rs`는 직렬화 DTO(`EnvironmentStatus`, 결과 래퍼)만 |
| 6 | AGENTS.md가 IPC 커맨드 수를 실제와 다르게 기술 (9개로 적혀 있었음) | 문서-코드 일치 | 정정 |

**의도적으로 남겨둔 것 (과잉 리팩토링 회피)**:
- `DesktopAdapter` 하나가 4개 포트를 구현하는 구조 — 포트 소비자가 모두 `Application`으로
  동일하고, 분리하면 조합 루트에 보일러플레이트만 늘어난다.
- `JobRegistry`를 포트 뒤에 숨기지 않음 — 인메모리 상태이며 저장 매체가 생기기 전까지
  추상화 비용만 발생한다.
- TS `BackendPort`의 통합 인터페이스 — 단일 소비자(controller)에게 ISP 분할은 이득이 없다.
- Python의 계층 비분리 — 단일 진입 CLI 사이드카에서 디렉터리 계층을 강제하면 가치 없는
  이동만 발생한다. 순수 모듈(`core`, `artifacts`, `minutes_*`)이 ML 스택 없이 테스트되는
  것이 이 워커의 실제 경계다.

## 7. 변경 시 필수 동반 세트 (변경 전 확인)

1. **워커 프로토콜**: `worker/galpi_worker/protocol.py` ↔ `src-tauri/src/domain/worker.rs`
   ↔ `src/domain/job.ts`(및 `tauri-backend.ts` Zod 스키마) ↔ `application/job-machine.ts`
   리듀서 — 한 커밋 세트.
2. **IPC 커맨드 추가**: `adapters/inbound/tauri.rs` + `composition.rs` 등록 +
   `BackendPort`/Zod 스키마 + 필요 시 `docs/ARCHITECTURE.md` §2 표 갱신(커맨드 수는
   `composition.rs`의 `generate_handler!` 항목 수와 맞춘다). 백엔드 이벤트를 추가하면
   `TauriEvents`에 해당 `*Events` 포트 구현과 `tauri-backend.ts`의 `listen` 파싱을 함께 갱신한다.
3. **새 외부 능력**: `application/ports.rs`에 trait → outbound 구현 → `composition.rs` wiring
   (ChatGPT 로그인 계열은 `ChatGptAuthPort`/`ChatGptStore`/`ChatGptEvents`/`ClockPort`)
   → `application/tests.rs`의 `FakePort` 확장.
4. **새 UI 상태**: `application/*-machine.ts`에 리듀서 + 동반 테스트; 뷰는 렌더만.

## 8. 검증 게이트

```bash
bun run check        # 아키텍처 펜스(§6-3 확장 포함) + Biome + tsc
bun test             # 리듀서·컨트롤러·DOM 테스트
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
uvx ruff check worker && uvx ruff format --check worker
PYTHONPATH=. python3 -m unittest discover -s worker/tests -t . -v
```

아키텍처 위반은 게이트 실패로: 펜스는 `scripts/check-architecture.ts`가 권위다.
