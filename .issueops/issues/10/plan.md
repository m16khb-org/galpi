# 계획: auth-gateway Google 로그인이 있어야 Galpi를 사용 (galpi#10)

- Lifecycle ID: `io-21fd3b1e1f06`
- 이슈: https://github.com/m16khb-org/galpi/issues/10
- 브랜치: `10-require-gateway-google-login` (base `main` @ `e77c87e3ca551ab121377024cbf6a43b1398dcae`)
- 선행 조건: m16khb-org/auth-gateway#8 (`io-4bf3aa5bab5f`, 브랜치 `8-desktop-native-login`)의 데스크톱 로그인 계약. 단위·application 테스트는 가짜 HTTP 서버로 그 계약을 고정하므로 gateway 머지 전에도 구현·검증할 수 있다. 실제 로그인 스모크(수동 확인 기록)만 gateway 브랜치를 로컬로 띄워야 한다.
- 사용자 요청 범위: 구현, 검증, 커밋, push, draft PR 발행, execution complete. merge는 범위 밖이다.
- 세션: 워크트리 준비 뒤 `issueops` 라우터의 환경별 자동 세션 인계를 적용한다. 인계는 실행 위치만 바꾸며 승인 범위를 바꾸지 않는다.

## 목표와 비목표

목표: 유효한 gateway 세션이 없으면 Galpi의 기능을 쓸 수 없게 한다. 로그인은 시스템 브라우저 + 루프백 + PKCE, 세션은 기존 비밀 저장소에 보관, gateway에 닿지 못하면 저장 세션으로 계속 사용, gateway가 명시적으로 거부하거나 사용자가 로그아웃하면 다시 로그인을 요구한다.

비목표: 역할·도메인 접근 제어, ChatGPT 로그인 동작 변경(오류 코드 포함), 워커·워커 프로토콜 변경, gateway 쪽 변경.

## 사용하는 gateway 계약 (auth-gateway#8 계획)

| 단계 | 요청 | 응답 |
|---|---|---|
| 시작 | 브라우저로 `GET {gateway}/auth/native/google?redirect_uri=http://127.0.0.1:<port>/auth/callback&code_challenge=<S256>&code_challenge_method=S256&state=<43자>` | Google 로그인 후 `redirect_uri?code=…&state=…`로 302 |
| 교환 | `POST {gateway}/auth/native/token` JSON `{grant_type:"authorization_code", code, code_verifier, redirect_uri}` | 200 `{access_token, refresh_token, expires_in, token_type}` / 400 `{error}` |
| 갱신 | `POST {gateway}/auth/native/token` JSON `{grant_type:"refresh_token", refresh_token}` | 200 같은 형태 / 400 `invalid_grant`(Supabase가 토큰을 거부했거나 폐기 목록에 있음) / 503 `temporarily_unavailable`(gateway가 Supabase에 닿지 못함) / 429 |
| 이메일 | `GET {gateway}/me` + `Authorization: Bearer` | 200 `{sub, role, email, sessionId}` |
| 로그아웃 | `POST {gateway}/auth/native/logout` JSON `{refresh_token}` | 200 `{status:"ok"}` |

로그인 시작 뒤 사용자가 Google 화면에서 취소하거나 gateway의 Supabase 교환이 실패하면 gateway는 `redirect_uri?error=access_denied&state=…`로 돌려보낸다(앱은 `GATEWAY_SIGN_IN_FAILED`). 데스크톱 로그아웃은 이 세션만 끊는다(Supabase `scope=local`). 반대로 사용자가 웹 서비스에서 로그아웃하면(global) 다음 갱신이 400 `invalid_grant`가 되어 앱은 다시 로그인을 요구한다.

`{gateway}`는 `https://auth.m16khb.dev`(gateway `README.md:126`, `docker-compose.yml:51`의 `PUBLIC_URL`)다. 디버그 빌드에서만 `GALPI_AUTH_GATEWAY_URL` 환경 변수로 바꿀 수 있다(로컬 gateway 스모크용). 릴리스 빌드는 상수만 쓴다.

## 현재 동작 (확인한 사실)

- ChatGPT 로그인 어댑터가 브라우저 열기·루프백·PKCE를 한 모듈 안에 갖고 있다: `TauriBrowser`(`src-tauri/src/adapters/outbound/chatgpt/browser.rs:20-33`, 오류 코드 `CHATGPT_SIGN_IN_FAILED`), `Loopback::bind_preferring`(선호 포트 실패 시 OS 할당, `loopback.rs:48-64`), `wait`(취소 `CANCELLED`, 시간 초과 `CHATGPT_SIGN_IN_TIMEOUT`, `loopback.rs:71-103`), `random_urlsafe`·`code_challenge`(`pkce.rs:14-28`, 오류 코드 `CHATGPT_SIGN_IN_FAILED`). 다른 모듈은 이것들을 쓰지 않는다(`composition.rs:2,50`이 `TauriBrowser`만 가져온다).
- 프런트엔드는 ChatGPT 오류 코드 일부를 문자열로 비교한다(`src/ui/chatgpt-controller.ts:19`, `src/domain/chatgpt.ts:61`, `src/ui/controller.test.ts:564`). 공유로 옮겨도 ChatGPT 경로의 코드는 그대로여야 한다.
- 비밀 저장소는 저장 대상별 enum 값(비밀 저장 모듈 `adapters/outbound/secrets` 37-55행)으로 구분되며, Windows는 Credential Manager의 `com.m16khb.galpi:<account>`, macOS는 0600 `settings.json`에 저장된다(같은 모듈 1-12행 주석). ChatGPT 토큰은 `LocalSettingsStore`의 비밀 저장 메서드에 ChatGPT 토큰 값을 넘겨 저장한다(`adapters/outbound/settings/chatgpt.rs:28-35,123-161`).
- `Application`은 포트 7개와 `ChatGptAccounts`를 받아 만든다(`src-tauri/src/application/use_cases.rs:36-74`). 기능 진입점은 `prepare`(88), `refine_transcript`(149), `transcribe`(267), `import_transcript`(280), `start_recording`(389)이다.
- 프런트 시작 순서: 컨트롤 바인딩 → 이벤트 구독 → `diagnose` → 설정 로드 → ChatGPT 로드(`src/ui/controller.ts:46-83`).
- 문서가 세는 숫자: IPC 명령 24개(`AGENTS.md`, `docs/ARCHITECTURE.md` §2), 포트 13개(`docs/ARCHITECTURE.md` §4, `.issueops/conventions/overview.md`).

## 설계

### Rust domain — `src-tauri/src/domain/gateway.rs` (신규)

- `GatewayTokens { access_token, refresh_token, expires_at }` — `Debug`를 직접 구현해 토큰을 가린다(ChatGPT 토큰 타입과 같은 방식).
- `AppAccess` (frontend로 나가는 값): `SignedOut` | `SignedIn { email: Option<String>, offline: bool }`. `offline`은 시작 갱신이 네트워크·5xx로 실패해 저장 세션으로 들어왔음을 뜻한다.
- `GatewayRefreshFailure`: `Rejected` | `Unavailable` — payload 없는 enum. domain은 `crate::application`(=`AppError`)에 의존할 수 없으므로(`scripts/check-architecture.ts`의 domain 울타리) 기존 `RefreshFailure`(`domain/chatgpt.rs:190-197`)처럼 값만 둔다. restore 판정에 오류 내용은 필요 없다.

### Rust application

- `ports.rs`에 포트 두 개:
  - `GatewayAuthPort`: `sign_in(cancel) -> Result<GatewayGrant{tokens, email}, AppError>`, `refresh(refresh_token) -> Result<GatewayTokens, GatewayRefreshFailure>`, `revoke(refresh_token)`(best-effort, 실패를 호출자에게 올리지 않음).
  - `GatewaySessionStore`: `load_email()`(비밀 아님), `load_tokens()`, `save_session(tokens, email)`, `replace_tokens(tokens)`, `clear()`.
- `application/gateway.rs` (신규) `AppAccessGate`:
  - 메모리 상태 `Mutex<AppAccess>`(초기값 `SignedOut`).
  - `restore()`: 토큰 없음 → `SignedOut`. 있으면 `refresh` 1회: 성공 → `replace_tokens` 후 `SignedIn{offline:false}`, `Rejected` → `clear` 후 `SignedOut`, `Unavailable` → `SignedIn{offline:true}`(저장 이메일 사용). refresh는 tokio `Mutex`로 직렬화해 회전 토큰을 한 번만 쓴다(ChatGPT `refresh_lock` 패턴).
  - `sign_in()`: ChatGPT와 같은 단일 슬롯(`AtomicBool` + 취소 `oneshot`)으로 동시 로그인 하나만 허용, 성공 → `save_session` 후 `SignedIn`.
  - `sign_out()`은 ChatGPT와 같이(`src-tauri/src/application/chatgpt.rs:80`) 진행 중인 로그인을 먼저 `cancel_sign_in()`으로 취소한 뒤 아래 순서를 따른다.
  - `cancel_sign_in()`: ChatGPT와 같이(`src-tauri/src/application/chatgpt/sign_in.rs:48-65`) 대기 중인 로그인에 `oneshot` 신호만 보낸다. 메모리 상태와 저장소는 바꾸지 않으며, 로그인이 끝난 뒤 도착한 취소는 슬롯이 sender를 비워 효과가 없다(`sign_in.rs:17-24`).
  - `sign_out()`: 메모리 상태를 먼저 `SignedOut`으로 바꾼 뒤(이후 기능 명령은 즉시 `AUTH_REQUIRED`), 토큰이 있으면 `revoke` 시도 → 결과와 관계없이 `clear`. `clear`가 실패하면 오류를 올리지만 메모리 상태는 `SignedOut`으로 남고 프런트는 `signed-out{error}`로 간다. 다음 실행의 `restore`가 남은 토큰을 다시 쓸 수 있다는 사실을 오류 문구로 알린다.
  - `require()`: 상태가 `SignedIn`이 아니면 `AppError::new("AUTH_REQUIRED", "Galpi를 사용하려면 Google로 로그인해 주세요.")`.
- `Application`: 생성자에 `AppAccessGate`를 추가하고, `prepare`·`refine_transcript`·`transcribe`·`import_transcript`·`start_recording` 첫 줄에서 `self.access.require()?`를 호출한다. `stop_recording`·`cancel_recording`·`cancel`은 진행 중인 작업을 끝내는 동작이라 막지 않는다. 설정 저장·진단도 막지 않는다(로그인 화면 뒤에서 호출되지 않으며, 막으면 시작 순서만 복잡해진다).
- 새 use case 메서드: `load_app_access()`(= `restore`), `sign_in_to_gateway()`, `cancel_gateway_sign_in()`, `sign_out_of_gateway()`.

### Rust adapters

- 공유 모듈 이동: `chatgpt/browser.rs`·`loopback.rs`·`pkce.rs`를 `adapters/outbound/browser_sign_in/`로 옮긴다. 오류 코드는 호출자가 `SignInCodes { failed: &'static str, timed_out: &'static str }`로 넘긴다. ChatGPT는 기존 `CHATGPT_SIGN_IN_FAILED`·`CHATGPT_SIGN_IN_TIMEOUT`을 그대로 넘기므로 관찰 가능한 동작이 같다. `BrowserOpener::open`은 코드를 인자로 받는다.
- `adapters/outbound/gateway/` (신규) `GatewayOAuthAdapter`:
  - `sign_in`: `Loopback::bind_preferring(PREFERRED_PORT)` → verifier(64바이트)·state(32바이트) 생성 → 시작 URL을 시스템 브라우저로 연다 → `wait`(5분, 취소) → `state` 일치 확인(불일치·`error` 파라미터·`code` 없음은 `GATEWAY_SIGN_IN_FAILED`) → 토큰 교환 → `GET /me`로 이메일(실패하면 `None`, 로그인은 성공). 루프백 경로는 `/auth/callback`.
  - `refresh`: 200 → 토큰. 상태가 400이고 본문 `error`가 `invalid_grant`일 때만 `Rejected`. 그 밖의 모든 비성공(400 `invalid_request`·본문 없는 400·401·403·429·5xx·연결 실패·파싱 실패)은 `Unavailable` — 이슈 정책 "gateway가 세션을 명시적으로 거부했을 때만 다시 로그인"과 기존 ChatGPT 분류(`chatgpt/oauth.rs:182-188`, 나머지는 `Transient`)를 따른다.
  - `revoke`: `POST /auth/native/logout` 1회, 실패는 무시하고 로그만 남기지 않는다(토큰·URL을 로그에 쓰지 않는다).
  - HTTP 클라이언트는 ChatGPT와 같은 설정(연결 10초, 전체 30초, 리다이렉트 비활성).
- 비밀 저장 모듈(`adapters/outbound/secrets`): 저장 대상 enum에 `GatewaySession` 값(account `gateway-session`)을 추가한다. Windows는 긴 값 조각 저장(`.issueops/adr/2026-10-09-windows-credential-manager.md`)이 그대로 적용된다. macOS의 설정 파일 백엔드는 read가 항상 없음·write가 no-op이고(`secrets` 모듈 140-160행), 실제 값은 `LocalSettingsStore`의 네 `match` 분기(값 읽기 104-107행, 값 쓰기 143-170행, 존재 표시 176-189행, 존재 확인 205-220행)가 `LocalSettings` 필드에 직접 쓴다. 그래서 네 분기에 `GatewaySession` arm을 모두 추가한다.
- `LocalSettings`(`adapters/outbound/settings.rs:255~`)에 `gateway_session: Option<String>`(macOS 평문, 0600), `gateway_session_stored: bool`, `gateway_email: Option<String>`을 추가하고 `is_empty()`(285-308행, 필드를 손으로 나열)에 세 필드를 넣는다. 빠뜨리면 다른 설정이 없는 새 설치에서 로그인 직후 `store_settings`가 파일을 지운다(430-435행). `settings/gateway.rs` (신규): `GatewaySessionStore for LocalSettingsStore`. 토큰은 기존 비밀 저장 메서드에 `GatewaySession` 값으로 넘기고, 이메일은 `gateway_email`에 둔다.
- `inbound/tauri.rs`: 명령 4개(`load_app_access`, `sign_in_to_gateway`, `cancel_gateway_sign_in`, `sign_out_of_gateway`). 새 이벤트는 만들지 않는다 — `sign_in_to_gateway`가 끝날 때까지 프런트는 "브라우저에서 로그인 중" 상태를 보여 준다.
- `composition.rs`: 어댑터·저장소 배선과 명령 등록.

### Frontend

- `src/domain/backend.ts`: `BackendPort`에 네 메서드와 `AppAccess` 타입. `src/adapters/tauri-backend.ts`: Zod 스키마로 파싱.
- `src/application/access-machine.ts` (신규, 순수 reducer + 테스트): `checking` → `signed-out` | `signed-in{email, offline}` | `failed{message, retryable}`; `failed{retryable:true}` → `checking`(다시 시도) | `signing-in`(다시 로그인 — 새 세션이 손상된 저장 항목을 덮어쓴다); `signed-out` → `signing-in` → `signed-in` | `signed-out{error}`; `signed-in` → `signing-out` → `signed-out` | `signed-out{error}`. 버튼 규칙은 허용 목록이다: 로그인 버튼은 `signed-out`·`signed-out{error}`·`failed{retryable:true}`에서만 보인다(ChatGPT의 `signIn.hidden = signedIn || flowActive`, `src/ui/chatgpt-settings.ts:148,151`와 같은 방식). `signing-in`에서는 취소 버튼만, `checking`·`signing-out`·`failed{retryable:false}`에서는 로그인 화면의 버튼을 모두 숨긴다. `failed{retryable:false}`에서 나가는 전이는 없다(재실행만 안내). 셸이 inert·무응답으로 남지 않는다는 기존 원칙(`controller.ts:47-48`)을 지킨다.
- `src/ui/access-controller.ts` (신규): 시작 때 `loadAppAccess`, 로그인·취소·로그아웃 처리, 로그인 화면 표시와 앱 셸 `inert` 전환.
- `src/ui/app-template.ts`·`app-view.ts`·`styles.css`: 로그인 화면 섹션(제목, 설명, `Google로 로그인` 버튼, 진행 중 `취소`, `role="status"` 메시지)과 상단 계정 표시(이메일, 오프라인이면 텍스트로 "오프라인 · 저장된 로그인 사용", `로그아웃` 버튼). 스타일은 SEED 토큰만, 버튼은 `src/ui/seed.ts` recipe.
- `src/ui/controller.ts`: `start()`를 "구독 → 접근 확인 → (로그인돼 있으면) 작업 공간 로드"로 나누고, 구독이 실패하면(`controller.ts:61-65`) access-machine을 `failed{message:"네이티브 런타임에 연결할 수 없습니다. 앱을 다시 실행해 주세요.", retryable:false}`로 보내 로그인 화면 자체에 문구를 보여 준다(셸 배너는 로그인 전 가려지므로). 이 실패에서는 다시 시도·로그인 버튼을 숨기고 재실행을 안내한다 — 구독 없이 재시도하면 작업 이벤트를 잃는다. 로그인 성공 시 작업 공간 로드를 실행, 로그아웃 시 진행 중 작업이 없을 때만 허용하고 로그인 화면으로 돌아간다. 녹음·전사·정제·준비가 진행 중이면 로그아웃 버튼을 비활성화한다.

### 문서

- `README.md`·`README.en.md`: 첫 실행에 Google 로그인 단계, 저장 위치(`com.m16khb.galpi:gateway-session`, macOS `settings.json`), 오프라인 동작, 로그아웃.
- `DESIGN.md`: 로그인 화면·계정 표시 상태와 문구.
- `.issueops/OPEN_API_SPEC.md`(13행 "24 Tauri IPC commands"), `AGENTS.md`·`src-tauri/AGENTS.md`(11행 "all 24 IPC commands")·`src-tauri/src/adapters/outbound/AGENTS.md`(STRUCTURE 표에 `browser_sign_in/`·`gateway/`)·`docs/ARCHITECTURE.md`·`.issueops/conventions/overview.md`·`.issueops/architecture/overview.md`: 명령 24→28, 포트 13→15, gateway 위치 행.

## 적용되는 결정과 주의사항

- `AGENTS.md` — Conventions — 의존성은 안쪽(domain)으로만 향하고, `#[tauri::command]`는 `inbound/tauri.rs`, 배선·`generate_handler!`는 `composition.rs`에만 둔다.
- `docs/ARCHITECTURE.md` §2·§4·§7 — 명령 수 표, 포트 수와 ISP 설명, "새 외부 능력은 port → outbound → composition → FakePort" 순서를 같은 변경에서 갱신한다.
- `scripts/check-architecture.ts` — inbound↔outbound 교차 import 금지, domain·application·inbound의 `cfg(windows)`/`cfg!(` 금지, ui의 `@tauri-apps`·adapters import 금지 — 디버그 전용 gateway URL 분기(`cfg!(debug_assertions)`)는 outbound 어댑터에만 둔다.
- `src-tauri/AGENTS.md` — 취소 가능한 작업은 `oneshot` receiver, `unwrap`/`expect`/`panic` 금지, 테스트는 `application/tests.rs`의 `FakePort` Given/When/Then.
- `src-tauri/src/adapters/outbound/AGENTS.md` — `ChatGptStore`의 저장 구조는 바꾸지 않는다 — gateway 세션은 새 `Secret` 값과 별도 store 구현으로 추가하고 ChatGPT 키·필드는 건드리지 않는다.
- `.issueops/CONSTITUTION.md` — 토큰을 로그·문서·픽스처·IPC 응답에 넣지 않는다 — `AppAccess`에는 이메일·상태만 있고, 테스트 토큰은 `galpi-test-…` 더미 값이다.
- `.issueops/adr/2026-10-09-windows-credential-manager.md` — 2,560바이트를 넘는 비밀은 세대별 조각 저장 — Supabase 세션 토큰도 같은 경로로 저장되므로 별도 처리가 필요 없다.
- `.issueops/adr/2026-10-09-windows-x64-os-cpu-cuda.md` — macOS 비밀은 서명 전까지 0600 설정 파일 — gateway 세션도 같은 정책을 따른다.
- `.issueops/cautions/2026-10-09-windows-env-clear-expect-dead-code.md` — 한 플랫폼에서만 쓰는 항목은 `cfg_attr`로 정확히 조건을 건다 — 새 `Secret` 값의 `account()` 사용처도 기존 `expect(dead_code)` 조건을 따른다.
- `.issueops/cautions/2026-10-09-readme.md` — README 두 벌을 같은 변경에서 고친다.
- `.issueops/conventions/overview.md` — 포트는 실제 경계에만, 각 포트에 프로덕션 어댑터와 테스트 페이크 — 두 포트 모두 `FakePort`에 구현한다. 새 변형은 태그드 유니온과 완전 match.
- `.issueops/testing/overview.md` — `styles.test.ts`(hex·px 리터럴·비 `--seed-` 커스텀 속성 금지), `seed.test.ts`(버튼 recipe), happy-dom 가시성 한계 — 로그인 화면은 실제 앱(`bun run dev`)에서 눈으로 확인한다. Windows 자격 증명 저장은 실기 수동 확인 항목이다.
- `DESIGN.md` — 상태는 색과 텍스트를 함께, 40px 이상 터치 영역, `role="status"`, `prefers-reduced-motion`, `word-break: keep-all`, 창은 토큰을 갖지 않고 계정 상태·이메일만 — 로그인 화면과 계정 표시에 적용한다.
- `DESIGN.md` — Tauri 이벤트는 invoke 전에 구독 — 이번 변경은 새 이벤트를 만들지 않으므로 기존 구독 순서를 유지한다.
- 대조했으나 해당 없음: `.issueops/adr/2026-10-09-7-fluent-korean.md`(회의록 프롬프트), `.issueops/cautions/2026-10-09-gated-token-implicit-token.md`(워커 HF 토큰), `DESIGN.md` §8 Accepted Debt.

## 재사용하는 기존 구현

- `TauriBrowser`·`BrowserOpener`, `Loopback`(선호 포트·OS 할당·취소·5분 시간 초과·완료 페이지), `random_urlsafe`·`code_challenge`(RFC 7636 벡터 테스트 포함) — 공유 모듈로 옮겨 두 로그인 어댑터가 같이 쓴다. 다시 만들지 않는다.
- `ChatGptAccounts`의 패턴(`refresh_lock`으로 회전 토큰 1회 사용, `sign_in_active`·`sign_in_cancel` 단일 슬롯, 로그아웃 시 서버 폐기 후 무조건 로컬 삭제) — `AppAccessGate`가 같은 구조를 따른다. `ChatGptAccounts` 자체는 OpenAI 등록·모델·동의 개념이 섞여 있어 재사용하지 않는다.
- `LocalSettingsStore::store_secret`·`secret`·캐시(`settings.rs:19-37`) — gateway 세션 저장에 그대로 쓴다.
- `SecretStore` 구현(Credential Manager 조각 저장, 설정 파일, 테스트용 `InMemorySecrets`).
- `application/tests.rs`의 `FakePort` — 두 포트를 구현해 application 테스트를 쓴다.
- 프런트: `chatgpt-controller.ts`의 로그인·취소·로그아웃 흐름과 `chatgpt-machine.ts` reducer 형식, `src/ui/seed.ts` 버튼 recipe, `required()` DOM helper.

새로 만드는 것: gateway HTTP 어댑터(엔드포인트·오류 분류가 OpenAI와 다름), `AppAccessGate`(앱 사용 게이트라는 새 규칙), 로그인 화면.

## 성능 영향

- 시작 시 네트워크 요청이 최대 1회(refresh) 늘어난다. 연결 10초·전체 30초 제한이므로 gateway가 응답하지 않으면 로그인 확인이 최대 30초 걸릴 수 있다. 이 시간 동안 로그인 화면에 "로그인 상태 확인 중"을 보여 준다(시간 추정은 표시하지 않음 — `AGENTS.md`의 ETA 금지).
- `require()`는 메모리 `Mutex` 읽기 한 번이라 기능 진입점에 측정할 만한 비용을 더하지 않는다.
- 로그인 시 HTTP 요청 2회(교환, `/me`), 로그아웃 시 1회.

## 하위 호환성과 side effect

- 동작 변경(의도): 업데이트 후 첫 실행에는 로그인하기 전까지 기능을 쓸 수 없다. 오프라인 첫 실행은 로그인할 수 없으므로 사용할 수 없다(사용자 결정 범위 안: 저장된 세션이 있을 때만 오프라인 허용).
- ChatGPT 로그인: 공유 모듈 이동 뒤에도 오류 코드·완료 페이지·포트 동작이 같다. 기존 ChatGPT 어댑터 테스트(`chatgpt/tests/`)를 수정 없이(import 경로 제외) 통과시켜 증명한다.
- 저장 데이터: `settings.json`에 `gatewayEmail`, `gatewaySessionStored`가, macOS에서는 `gatewaySession`(토큰 JSON 평문, 파일 권한 0600 — HF 토큰·ChatGPT 토큰과 같은 정책)이 추가된다. 구버전은 모르는 키를 무시한다(`LocalSettings`에 `deny_unknown_fields` 없음 — `settings.rs:252-255` 정의 확인). Windows는 Credential Manager에 `com.m16khb.galpi:gateway-session` 항목이 생긴다. 로그아웃하면 둘 다 지운다.
- IPC: 명령 4개 추가, 기존 명령의 인자·응답은 그대로다. 단 기능 명령 5개가 로그인 전에는 `AUTH_REQUIRED`로 실패한다.
- 롤백: 커밋 되돌리기. 남는 것은 설정 파일의 `gatewayEmail`·`gatewaySessionStored`, macOS의 `gatewaySession`(평문 refresh token, 0600), Windows Credential Manager 항목이다. 구버전은 읽지 않지만 macOS 평문 refresh token은 사용자가 지우기 전까지 남으므로 릴리스 노트에 적는다.

## 구현 순서 (RED → GREEN)

1. 공유 모듈 이동(동작 불변): 파일 이동 + `SignInCodes` 인자. 기존 ChatGPT 테스트가 그대로 통과(G1).
2. domain·포트·`AppAccessGate` RED: `FakePort`로 (a) 토큰 없음 → `SignedOut`·기능 5개 `AUTH_REQUIRED`, (b) refresh 성공 → 회전 토큰 저장·`SignedIn{offline:false}`, (c) `Rejected` → 저장소 비움·`SignedOut`, (d) `Unavailable` → `SignedIn{offline:true}`·토큰 유지, (e) 로그인 성공 → 저장·기능 허용, (f) 로그인 중 두 번째 시도 거부·취소, (g) 로그아웃 → revoke 1회·저장소 비움·`AUTH_REQUIRED`, revoke 실패해도 비움, (h) 로그인 진행 중 로그아웃 → 로그인이 취소되고 이후 저장된 세션 없음, (i) 저장 세션이 있는 상태에서 로그인 시작 후 취소 → `revoke` 0회·저장 세션 유지·로그인 결과 `CANCELLED`. 테스트 이름에 `gateway`를 넣어 G2 필터로 실행되게 한다. GREEN.
3. 어댑터 RED: 로컬 가짜 HTTP 서버로 교환 요청 본문(JSON 필드 4개), refresh 분류(200 → 토큰, 400 `invalid_grant` → `Rejected`, 400 `invalid_request`·본문 없는 400·401·429·503·연결 거부 → `Unavailable`), `/me` 실패 시 이메일 `None`, state 불일치 거부, 시작 URL 쿼리(redirect_uri·challenge·method·state). 설정 store: (i) 빈 설정에서 설정 파일 백엔드로 `save_session` → 같은 경로로 새로 만든 store가 토큰·이메일을 읽음(재시작), (ii) `InMemorySecrets` 백엔드에서 재시작 후 이메일 유지·설정 파일 평문에 토큰 없음, (iii) `clear` 후 둘 다 비움·다른 설정이 없으면 파일 삭제. GREEN.
4. Tauri 명령·배선, 프런트 reducer 테스트(`failed`에서 다시 시도·다시 로그인, 로그아웃 실패 → `signed-out{error}` 포함), DOM 테스트(로그인 전 앱 셸 `inert`·로그인 화면 표시, `loadAppAccess` 실패 시 오류 문구와 로그인 버튼 표시, `signInToGateway`가 끝나지 않은 동안 로그인 버튼 `hidden`·취소 버튼 표시·두 번째 `signInToGateway` 호출 0회, 모든 IPC가 거부되는 백엔드에서 로그인 화면 영역에 '네이티브 런타임' 문구가 보이고 상태가 `checking`이 아니며 로그인·다시 시도 버튼이 둘 다 `hidden`이고 `signInToGateway` 호출이 0회, 로그인 성공 후 작업 공간 로드, 로그아웃 후 복귀, 오프라인 표시 텍스트), Zod 경계 테스트.
5. 문서 갱신.

## 게이트

```text
G1: 공유 모듈 이동 뒤 ChatGPT 로그인 테스트가 그대로 통과한다 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets chatgpt | EXPECT: /test result: ok\. [1-9]\d* passed/
G2: 접근 게이트 application 테스트 (a)~(i) | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets gateway | EXPECT: /test result: ok\. [1-9]\d* passed/
G3: gateway 어댑터·설정 store 테스트 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets adapters::outbound | EXPECT: /test result: ok\. [1-9]\d* passed/
G4: Rust 서식 | CHECK: python3 -c 'import subprocess,sys;r=subprocess.run(["cargo", "fmt", "--manifest-path", "src-tauri/Cargo.toml", "--check"]);print("FMT_OK" if r.returncode==0 else "FMT_FAIL");sys.exit(r.returncode)' | EXPECT: FMT_OK
G4b: Clippy | CHECK: python3 -c 'import subprocess,sys;r=subprocess.run(["cargo", "clippy", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings"]);print("CLIPPY_OK" if r.returncode==0 else "CLIPPY_FAIL");sys.exit(r.returncode)' | EXPECT: CLIPPY_OK
G4c: Rust 전체 테스트 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets | EXPECT: /test result: ok\. [1-9]\d* passed/
G5: 아키텍처 검사·Biome·tsc | CHECK: python3 -c 'import subprocess,sys;r=subprocess.run(["bun", "run", "check"]);print("CHECK_OK" if r.returncode==0 else "CHECK_FAIL");sys.exit(r.returncode)' | EXPECT: CHECK_OK
G5b: 프런트 reducer·DOM·Zod 테스트 | CHECK: bun test | EXPECT: /^\s*0 fail$/
G6: 문서 플랫폼 문구 검사 | CHECK: python3 scripts/verify-docs-platform.py | EXPECT: PLATFORM_DOCS_OK
G7: 명령 28·포트 15와 문서 숫자 일치 | CHECK: python3 -c 'import re,sys,pathlib as P;t=P.Path("src-tauri/src/adapters/inbound/tauri.rs").read_text().count("#[tauri::command]");q=len(re.findall(r"^pub trait",P.Path("src-tauri/src/application/ports.rs").read_text(),re.M));pats=["24 frontend","24 command","24 IPC","24 Tauri","24 커맨드","24개","24 `#[tauri","13 Rust traits","13 ports","13개 포트","13개 trait","포트 13개"];fs=["AGENTS.md","src-tauri/AGENTS.md","docs/ARCHITECTURE.md",".issueops/conventions/overview.md",".issueops/architecture/overview.md",".issueops/OPEN_API_SPEC.md"];left=[(f,x) for f in fs for x in pats if x in P.Path(f).read_text()];ok=t==28 and q==15 and not left;print("COUNTS_OK" if ok else ("COUNTS_FAIL",t,q,left));sys.exit(0 if ok else 1)' | EXPECT: COUNTS_OK
G8: IPC로 나가는 AppAccess JSON에 토큰이 없다 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets app_access_serializes_only_state_and_email | EXPECT: /test result: ok\. 1 passed/
```

수동 확인 기록(원장 게이트가 아님 — argv 명령 하나로 관찰할 수 없음):

- 실제 로그인 스모크: auth-gateway#8 브랜치를 로컬로 띄우고 `GALPI_AUTH_GATEWAY_URL`로 `bun run dev` → 로그인 → 앱 재시작 후 유지 → 로그아웃 → 로그인 화면. gateway 로컬 실행에 Supabase 자격 증명이 필요하며, 쓸 수 없으면 근거와 함께 보류하고 gateway 배포 후 운영 gateway로 확인한다.
- Windows Credential Manager의 `com.m16khb.galpi:gateway-session` 항목 생성·삭제(실기).

모든 CHECK는 한 개의 argv 명령이며 `|`·`&&`·`;` 셸 연결자를 쓰지 않는다(`gates-ledger` 규칙). 여러 단계는 `python3 -c` 하나로 감싸 표지 문자열을 출력한다.

## 의존성과 로컬 설정

- 의존성: 워크트리에서 `bun install`. Rust 크레이트는 추가하지 않는다(reqwest·serde·sha2·base64·getrandom 이미 사용 중).
- 로컬 설정: 없음. 디버그 스모크의 `GALPI_AUTH_GATEWAY_URL`은 셸 환경 변수로만 넘긴다.
