# 검증 보고서 초안: galpi#10 Google 로그인 필수화 (io-21fd3b1e1f06)

## 범위

- 공유 모듈: `adapters/outbound/browser_sign_in/`(브라우저·루프백·PKCE, `SignInCodes`로 호출자 오류 코드 전달), 테스트용 가짜 HTTP 서버를 같은 위치로 옮겼다.
- Rust: `domain/gateway.rs`(`GatewayTokens`·`GatewayGrant`·`AppAccess`·`GatewayRefreshFailure`), 포트 `GatewayAuthPort`·`GatewaySessionStore`, `application/gateway.rs` `AppAccessGate`, 기능 명령 5개(`prepare`·`refine_transcript`·`transcribe`·`import_transcript`·`start_recording`)의 `AUTH_REQUIRED` 가드, `adapters/outbound/gateway/` HTTP 어댑터, `settings/gateway.rs` 저장소, `Secret::GatewaySession`, Tauri 명령 4개, `composition.rs` 배선.
- 프런트엔드: `BackendPort` 네 메서드와 `AppAccess`, Zod `appAccessSchema`, `access-machine.ts`, `access-screen.ts`, `access-controller.ts`, 로그인 화면·계정 표시 마크업과 스타일, `AppController` 시작 순서 변경.
- 문서: README 두 벌, DESIGN.md, AGENTS.md, src-tauri/AGENTS.md, outbound AGENTS.md, docs/ARCHITECTURE.md, `.issueops` 개요 3개.

## RED → GREEN 증거

- RED: `AppAccessGate::require`를 항상 `Ok`로, 갱신 분류를 "모든 4xx는 거부"로 바꾼 상태에서 `cargo test --all-targets gateway` → 24개 중 8개 실패(기능 차단 7개, `only_invalid_grant_rejects_the_session` 1개). 원래 코드로 되돌린 뒤 24개 모두 통과.
- 게이트 원장 `.issueops/issues/10/gates.md`: G1~G8 11개 모두 PASS(`issueops gates check --write`).

## 실행한 검증

| 명령 | 결과 |
|---|---|
| `cargo test --all-targets chatgpt` | 68 passed |
| `cargo test --all-targets gateway` | 24 passed |
| `cargo test --all-targets adapters::outbound` | 138 passed, 1 ignored |
| `cargo test --all-targets` | 224 passed, 1 ignored |
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | 통과 |
| `bun run check` | 통과(기존 info 수준 Biome 안내만 출력) |
| `bun test` | 201 pass, 0 fail |
| `python3 scripts/verify-docs-platform.py` | PLATFORM_DOCS_OK |
| G7 명령·포트 수 대조 | COUNTS_OK(명령 28, 포트 15) |

## 화면 확인

- Vite 개발 서버 + Chromium에서 `__TAURI_INTERNALS__`를 가짜로 둔 상태로 로그인 화면(로그인 전), 브라우저 대기(취소만 표시), 취소 후 복귀, 오프라인 로그인 후 작업 공간(상단 `Google 계정 / 오프라인 · 저장된 로그인 사용` + `로그아웃`), 로그아웃 후 로그인 화면, 375px 폭을 확인했다. 로그인 전 `.app-shell`은 `inert`였고 작업 공간 IPC(`diagnose_environment` 등)는 로그인 성공 뒤에만 호출됐다.
- `cargo tauri dev`로 실제 앱을 띄워 컴파일·실행까지 확인했다. 창 캡처와 접근성 트리 조회는 이 환경에 화면 기록·손쉬운 사용 권한이 없어 실패했다.

## 수동 확인으로 남긴 항목

- 실제 로그인 스모크: auth-gateway#8 브랜치(`8-desktop-native-login`)의 엔드포인트·본문·오류 형태를 소스로 대조했다(`POST /auth/native/token` JSON + `Content-Type: application/json` 필수, 400 `invalid_grant`, 503 `temporarily_unavailable`, `POST /auth/native/logout` `{refresh_token}`, `GET /me`의 `email`). 로컬 gateway 실행에 필요한 Supabase·Redis 자격 증명(`.env`)이 워크트리에 없어 실제 로그인 스모크는 gateway 배포 뒤 운영 gateway로 확인한다.
- Windows Credential Manager의 `com.m16khb.galpi:gateway-session` 생성·삭제: Windows 실기에서 확인한다.

## side effect

- 파일: `settings.json`에 `gatewayEmail`, `gatewaySessionStored`(macOS는 `gatewaySession` 평문, 0600) 키가 추가된다. 로그아웃하면 지운다.
- OS 비밀 저장소: Windows `com.m16khb.galpi:gateway-session`.
- 네트워크: 시작 시 refresh 1회, 로그인 시 교환·`/me` 2회, 로그아웃 시 1회.
- durable state: 없음(메모리의 접근 상태만 추가).

## 계약 변경

- IPC 명령 4개 추가(`load_app_access`, `sign_in_to_gateway`, `cancel_gateway_sign_in`, `sign_out_of_gateway`). 기존 명령의 인자·응답은 그대로다.
- 새 공개 오류 코드: `AUTH_REQUIRED`(기능 명령 5개, 로그인 전), `GATEWAY_SIGN_IN_FAILED`, `GATEWAY_SIGN_IN_TIMEOUT`, `GATEWAY_SIGN_IN_BUSY`, `GATEWAY_SIGN_OUT_INCOMPLETE`, `CANCELLED`(기존 코드 재사용).
- ChatGPT 경로의 오류 코드는 바뀌지 않았다(G1).

## ai-slop-clean

- 바꾼 것: `adapters/outbound/gateway/mod.rs`에서 한 번만 쓰던 `error_code` 헬퍼(`serde_json::Value` 수동 탐색)를 `ErrorBody` 역직렬화로 바꿨다(category: needless-abstraction). 동작은 같다(`only_invalid_grant_rejects_the_session` 통과).
- 유지한 것: `AppAccessGate.access`의 `pub(super)` 가시성은 application 테스트가 로그인 없이 게이트를 여는 데 필요하다. `AccessController.cancelSignIn`의 오류 무시는 취소 실패 시 대기 중인 로그인이 자기 결과로 화면을 정리하기 때문이다.
- 측정: 정리 전후 `git diff --shortstat`은 57개 파일, 2,973줄 추가·129줄 삭제(정리로 헬퍼 6줄 삭제·구조체 6줄 추가). 정리 뒤 `cargo clippy -D warnings` 통과, `cargo test --all-targets` 224 passed.

## 성능

- `require()`는 메모리 `Mutex` 한 번 잠금이다. 시작 시 네트워크 요청 1회(연결 10초·전체 30초 제한)가 추가된다. 별도 벤치마크는 하지 않았다.
