# 계획: auth-gateway 의존 제거 (galpi#16)

- Lifecycle ID: `io-d5c9aed664a2`
- 이슈: https://github.com/m16khb-org/galpi/issues/16
- 브랜치: `16-remove-auth-gateway` (base `main` @ `fed2f856e56ac0e545ea3082a60f8bb71f3529df`)
- 사용자 요청 범위: 이슈, 계획, 구현, 검증, 커밋, push, draft PR 발행, execution complete. merge는 범위 밖이다.
- 세션: 워크트리 준비 뒤 `issueops` 라우터의 환경별 자동 세션 인계를 적용한다(준비 세션이 Herdr 안이므로 Herdr 우선). 인계는 실행 위치만 바꾸며 승인 범위와 종료점을 바꾸지 않는다.
- 개정: 2차(2026-10-12). 1차 계획의 구현은 로컬 커밋 `1c52d24`로 끝났고(push 전), 그 뒤 범위가 두 가지 늘었다. (1) GitHub push 보호(GH013) 대응, (2) 사용자가 자기 Google OAuth 클라이언트를 설정 시트에 넣어 내장 기본값 대신 쓰는 기능과 그 만드는 방법 안내. 사용자 결정: (2)를 #16에 합친다. 따라서 `1c52d24`는 그대로 push하지 않고, 이 개정의 구현을 더한 뒤 정리·문서·검증·구현 리뷰를 다시 거친다. 1차 계획 본문은 그대로 두고, 늘어난 범위는 각 절의 `범위 추가` 표시로 덧붙인다.

## 목표와 비목표

목표: Galpi가 auth-gateway(`https://auth.m16khb.dev`)에 아무 요청도 보내지 않게 한다.

1. 앱 접근 게이트를 없앤다. 로컬 기능과 Drive 명령은 로그인 없이 동작하고, 창은 로그인 화면 없이 작업 화면으로 열린다.
2. Drive 코드 교환과 갱신을 Google 토큰 엔드포인트 `https://oauth2.googleapis.com/token`에 직접 보낸다. 데스크톱 클라이언트 보안 비밀번호는 `drive/mod.rs`의 상수로 둔다.

비목표: auth-gateway 저장소 변경·서버 종료, Cloud 콘솔의 기존 보안 비밀번호(끝 `k4Ni`) 중지·삭제, Drive 동기화 방식·`drive.file` 범위·Picker 흐름·ChatGPT 로그인 변경, 워커·워커 프로토콜 변경.


범위 추가 목표:

3. GitHub push 보호를 사용자의 허용으로 통과한다. 내장 클라이언트 ID와 보안 비밀번호는 평문 상수로 되돌린다(base64는 GitHub가 디코딩해 다시 막으므로 아무것도 막지 못하고 코드만 복잡하게 한다).
4. 사용자 지정 클라이언트: 설정 시트 `Google Drive` 절에서 클라이언트 ID와 보안 비밀번호를 저장하면 동의 화면, 코드 교환, 토큰 갱신이 모두 그 클라이언트로 간다. 입력이 없으면 내장 클라이언트를 쓴다. README 두 벌과 설정 시트에 만드는 방법을 안내한다.

범위 추가 비목표: GitHub push 보호 설정 변경이나 에이전트의 우회, 사용자 지정 클라이언트를 여러 개 보관하거나 계정마다 고르는 기능, 사용자 지정 클라이언트의 Picker 외 다른 Google API 사용.

## 적용되는 결정과 주의사항

| 문서 | 항목 | 이 계획에 주는 제약 |
|---|---|---|
| `.issueops/CONSTITUTION.md` | Source of truth, Principles | 최신 사용자 지시(C안)가 `.issueops`보다 우선한다. "raw secret을 docs, logs, test fixtures에 넣지 않는다" 원칙은 유지해, 보안 비밀번호 값은 `drive/mod.rs` 상수 한 곳에만 두고 테스트·문서·로그·이슈·PR 본문에는 값을 쓰지 않는다. 테스트는 상수를 참조한다. |
| `.issueops/adr/2026-10-09-auth-gateway-google.md` | 앱 사용에 gateway 로그인 요구 | 이번 변경이 이 결정을 대체한다. 새 ADR에 대체 사실과 이유(로컬 기능이 gateway를 쓰지 않음, 모든 계정 허용이라 제한 효과 없음)를 남긴다. |
| `.issueops/adr/2026-10-11-google-drive-auth-gateway-appproperties.md` | gateway 대행 교환, "소스 상수 포함" 기각 | Decision 1·2와 기각 대안이 뒤집힌다. appProperties 무상태 동기화, `invalid_grant`만 연결 해제, 자동 재전송 금지는 유지한다. 새 ADR이 대체 관계를 적는다. |
| `.issueops/adr/2026-08-23-adopt-hexagonal-ports-...` / `docs/ARCHITECTURE.md` | 안쪽 의존 규칙, 포트 소유 | 포트 삭제·시그니처 변경은 `application/ports.rs`에서 하고 어댑터·가짜·`composition.rs`를 같은 변경 세트로 맞춘다. 문서의 포트 수(18→16)와 명령 수(40→36)를 고친다. |
| `.issueops/cautions/2026-10-09-new-stored-credential-needs-four-match-arms-and-is-empty.md` | 새 Secret은 네 match 분기와 `is_empty`를 함께 고친다 | 반대 방향도 같다: 열거형 변형 `GatewaySession`을 지울 때 `settings.rs`의 네 분기(111, 173-180, 201·212, 243-246), 필드(318-320), `is_empty`(356-358)를 모두 지운다. |
| `.issueops/cautions/2026-10-09-qa-ipc.md` | 런타임 연결 실패를 로그인 화면에 표시 | 로그인 화면이 없어지므로 #10 이전 동작(`view.showError` 배너)으로 되돌리고, 이 caution은 새 기록으로 대체됨을 표시한다. |
| `.issueops/cautions/2026-08-23-tauri-frontend-renders-in-a-plain-browser-...` | bind 먼저, 구독 뒤 invoke | `start()`는 `bind()` → 구독 → `loadWorkspace()` 순서를 지킨다. |
| `.issueops/cautions/2026-10-11-check.md` | 게이트 CHECK에 비밀처럼 보이는 문자열 금지 | 게이트 CHECK 인자에 보안 비밀번호 접두어나 값을 넣지 않는다. 패턴은 실행 중에 조립한다. |
| `.issueops/cautions/2026-10-11-picker-google-500.md` | Picker 동의 500은 새로고침 | 실제 스모크에서 500을 보면 같은 탭을 새로고침한다. |
| `.issueops/cautions/2026-10-09-readme.md` | README 두 벌 동시 수정 | README.md와 README.en.md를 같은 변경에서 고친다. |
| `.issueops/testing/overview.md` | Drive 검사 | "gateway-brokered exchange", "live check needs auth-gateway", "never commit ... the client secret" 문장을 새 결정에 맞게 고친다. |
| `.issueops/conventions/overview.md` | 오류는 안정적인 ASCII 코드, 확장은 tagged union + 철저한 match | `DriveTokenFailure`에서 변형을 지우면 모든 match가 컴파일러로 드러난다. 기존 공개 코드 `DRIVE_*`는 그대로 둔다. |
| `.issueops/adr/2026-10-12-drop-auth-gateway-drive-client-in-source.md` | 이번 사이클이 만든 ADR. Encoding 항목에 base64 저장 결정이 있다 | 범위 추가로 Encoding 결정이 뒤집힌다. 같은 ADR을 보강해 base64를 지운 이유(GitHub가 base64도 디코딩해 막음), push 보호는 사용자가 허용 링크로 통과한다는 점, 사용자 지정 클라이언트 결정을 적는다. |
| `.issueops/adr/2026-10-09-sign-in-with-chatgpt-assistantsettings.md` | 호스트가 토큰을 소유, IPC 타입에 토큰을 넣지 않음 | 사용자 지정 보안 비밀번호도 같은 규칙이다. IPC 응답 `DriveClientStatus`에는 `custom`과 `clientId`만 있고, 프런트 Zod 스키마는 `strictObject`로 그 밖의 필드를 거부한다. |
| `.issueops/adr/2026-10-09-windows-credential-manager.md` | 2,560바이트를 넘는 비밀은 조각 저장 | 보안 비밀번호는 짧지만 기존 `SecretStore` 경로를 그대로 써서 새 저장 경로를 만들지 않는다. |
| `.issueops/cautions/2026-10-09-new-stored-credential-needs-four-match-arms-and-is-empty.md` (범위 추가) | 새 Secret은 네 match 분기와 `is_empty`를 함께 고친다 | 새 변형 `DriveClientSecret`(계정 이름 `drive-client-secret`)을 넣을 때 `settings.rs`의 네 분기, `LocalSettings` 필드(`drive_client_id`, `drive_client_secret`, `drive_client_secret_stored`), `is_empty`를 모두 고친다. G17이 판별한다. |
| `.issueops/cautions/2026-10-11-check.md` (범위 추가) | 게이트 CHECK에 비밀처럼 보이는 문자열 금지 | 사용자 지정 클라이언트 값도 같다. 테스트는 `123-abc.apps.googleusercontent.com`, `test-client-secret` 같은 가짜 값만 쓰고, 문서 예시는 `<클라이언트 ID>` 같은 자리 표시만 쓴다. 실제 값은 문서·테스트·로그·이슈·PR 본문·게이트 인자 어디에도 쓰지 않는다. |
| `DESIGN.md` (범위 추가) | 상태는 색과 글자를 함께, 라벨은 placeholder가 아님, SEED 토큰만 | 새 입력칸은 보이는 `<label>`을 갖고, 잠금·저장됨·오류 상태는 글자로도 알린다. 스타일은 기존 설정 시트 클래스와 `--seed-*` 토큰만 쓴다. |
| `docs/ARCHITECTURE.md` (범위 추가) | IPC 명령, Zod 스키마, `BackendPort`, 컨트롤러는 한 변경 세트 | 명령 3개는 Rust 명령, `composition.rs` 등록, `BackendPort`, `TauriBackend` Zod 파싱, 컨트롤러를 같은 변경에서 맞춘다. 포트 수는 16 그대로이고 명령 수는 36에서 39가 된다. |

## 현재 동작 (확인한 사실)

- `AppAccessGate::require()`는 메모리 상태만 본다(`src-tauri/src/application/gateway.rs:82-87`). `require()` 호출은 `use_cases.rs`의 10곳(135, 210, 342, 365, 380, 406, 452, 480, 504, 635)이다.
- `AUTH_REQUIRED`는 `application/gateway.rs:40`에서만 만들어진다. 프런트 소비처는 `src/ui/drive-controller.ts:68, 242`다.
- Drive 교환·갱신: `drive/mod.rs:115-117`이 `{gateway_url}/drive/native/token`을 만들고, `drive/tokens.rs:26-58`이 JSON 본문에 `bearer_auth`를 붙인다. 401은 `DriveTokenFailure::AuthRequired`(48-50), 400 `invalid_grant`는 `Rejected`(51-55), 나머지는 `Unavailable`이다.
- `application/drive.rs`는 `access.bearer()`를 242, 343에서 부르고, `token_failure`(72-85)가 `AuthRequired`일 때 `access.expire()`를 부른다. `&AppAccessGate`를 받는 함수는 12곳이다(73, 201, 237, 297, 309, 320, 340, 365, 399, 454, 482, 559).
- `revoke`는 이미 Google에 form으로 직접 보낸다(`drive/mod.rs:181-189`). reqwest `form` 기능은 켜져 있다(`src-tauri/Cargo.toml:25`).
- 프런트 시작: `controller.ts:109-133`이 구독 실패 시 `access.runtimeUnavailable(...)`, 성공 시 `access.check()`를 부르고, 로그인이 열리면 `loadWorkspace()`(136-156)를 부른다. #10 이전(`3732162^`)에는 구독 실패를 `view.showError("네이티브 런타임에 연결할 수 없습니다. 앱을 다시 실행해 주세요.")`로 알리고 바로 작업 화면을 불러왔다.
- `drive-view.ts:66-74`는 `#account-chip`의 이메일과 Drive 계정이 같으면 "연결됨"만 보인다.
- `DriveController.reset()`(`drive-controller.ts:53-56`)의 호출자는 `controller.ts:69` 하나이고, 그것이 `drive-machine.ts`의 `signedOut` 이벤트(73, 193-194)의 유일한 발신자다.
- 설정 저장은 `LocalSettings` 전체를 `serde_json::to_vec`로 다시 쓴다(`settings.rs:509-516`). 구조체는 `#[serde(default)]`이고 `deny_unknown_fields`가 없다(286-288). 필드를 지우면 옛 파일은 계속 읽히고, 다음 설정 쓰기에서 옛 gateway 키가 사라진다.


범위 추가 시점(`1c52d24`)에 확인한 사실:

- 내장 클라이언트는 `drive/mod.rs:36-54`의 `CLIENT_ID_B64`, `CLIENT_SECRET_B64`, `decoded()`, `client_id()`, `client_secret()`이다. 호출처는 `drive/authorize.rs:38`(동의 URL), `drive/tokens.rs:66`(교환), `drive/tokens.rs:91`(갱신) 세 곳이다. `base64` 의존성은 base부터 있었고(`Cargo.toml:18`) `browser_sign_in/pkce.rs`도 쓰므로, 내장 값을 평문으로 되돌려도 의존성은 그대로다.
- push 보호: 구현 세션의 push가 GH013으로 막혔다. 평문 두 값과 base64 두 값(`mod.rs` 36·37행) 모두 막혔다. 1차 인계 자료의 "push 보호가 꺼져 있다"는 틀렸다. 저장소 API의 `secret_scanning_push_protection: disabled`는 저장소 설정일 뿐이고, 공개 저장소에는 사용자 단위 push 보호가 기본으로 켜져 있다([GitHub 문서](https://docs.github.com/en/code-security/secret-scanning/working-with-secret-scanning-and-push-protection/push-protection-for-users)). 사용자 단위 보호에서 `Allow me to push this secret`을 누르면 3시간 안에 push해야 하고, 지나면 다시 허용해야 한다([GitHub 문서](https://docs.github.com/en/code-security/secret-scanning/working-with-secret-scanning-and-push-protection/working-with-push-protection-from-the-command-line)). 이미 받아 둔 허용 링크는 개정 구현이 끝날 때쯤 만료될 수 있다.
- `DriveSessionStore`(`ports.rs:361-379`)에는 클라이언트 항목이 없고, `clear()`(`settings/drive.rs:101-110`)는 Drive 세션과 연결 정보만 지우고 온보딩 선택은 남긴다.
- `DriveAccounts::connect`(`application/drive.rs:189-217`)는 먼저 연결 슬롯(`operation_active`)을 잡고 동의를 기다린 뒤, 저장할 때만 `store_lock`을 잡는다. 갱신(`renew_locked`, 317-330)은 `store_lock` 아래에서 한다. Picker(`pick`)는 연결이 있어야 시작한다.
- 명령은 `tauri.rs`에 `#[tauri::command]` 36개, 포트 trait은 `ports.rs`에 16개다. 문서의 숫자는 `AGENTS.md:50`(`36 frontend commands`), `docs/ARCHITECTURE.md:19`(`36 커맨드`)·`53`(`36개 #[tauri::command]`, "Drive 10개 포함"), `.issueops/architecture/overview.md:35`(`36 #[tauri::command]s`)에 있다.
- 설정 시트의 Drive 절은 `src/ui/app-template.ts:404-420`(`#drive-settings`)이다.
- Google 공식 문서: 데스크톱 Picker를 쓰려면 Cloud 프로젝트에서 Google Picker API를 사용 설정해야 한다([Picker 데스크톱 개요](https://developers.google.com/workspace/drive/picker/guides/overview-desktop)). 외부 사용자 유형에 게시 상태가 `테스트`인 프로젝트는 refresh token이 7일 뒤 만료된다([OAuth 2.0 문서](https://developers.google.com/identity/protocols/oauth2)). `drive.file`은 Drive 범위 표에서 민감하지 않은 범위로 분류된다([Drive 범위 문서](https://developers.google.com/workspace/drive/api/guides/api-specific-auth)). Cloud 콘솔은 보안 비밀번호를 만든 직후에만 보여 준다(galpi-511214 콘솔에서 직접 확인).

## 설계

### Rust domain

- `domain/gateway.rs` 삭제, `domain/mod.rs`에서 `pub mod gateway` 제거.
- `domain/drive.rs`의 `DriveTokenFailure`를 `Rejected | Unavailable`로 줄이고 doc을 Google 기준으로 고친다.

### Rust application

- `application/gateway.rs`, `application/tests/gateway.rs` 삭제. `tests/gateway.rs`의 `tokens_expiring` 헬퍼는 `GatewayTokens`를 돌려주고 `tests/drive.rs`의 호출 네 곳(584, 729, 741, 1462)이 모두 지워질 gateway 세션 시드이므로 옮기지 않고 함께 지운다.
- `ports.rs`: `GatewayAuthPort`, `GatewaySessionStore` 삭제. `DriveAuthPort::exchange(&self, callback)`, `refresh(&self, refresh_token)`로 bearer 인자를 뺀다.
- `application/drive.rs`: `&AppAccessGate` 인자 12곳과 `bearer()` 호출 2곳을 지운다. `token_failure`는 `fn token_failure(failure, rejected) -> AppError` 동기 함수가 된다. `unavailable()` 문구에서 "또는 로그인 서버"를 뺀다. 모듈 doc 4-7행을 "교환과 갱신은 Google 토큰 엔드포인트에 직접 한다"로 고친다.
- `use_cases.rs`: `access` 필드·생성자 인자·메서드 4개·`require()` 10곳·Drive 호출의 `&self.access` 인자를 지운다.
- `application/tests.rs`: `FakePort`의 gateway 구현, `gate`, `signed_out_application` 같은 로그인 전제 헬퍼를 지운다.

### Rust adapters

- `adapters/outbound/gateway/` 디렉터리, `settings/gateway.rs` 삭제. `adapters/outbound/mod.rs`, `settings.rs`의 `mod gateway` 제거.
- `drive/mod.rs`: `use super::gateway::gateway_url` 제거. `Tuning.gateway_url`을 `token_url`로 바꾸고 프로덕션 값은 새 상수 `GOOGLE_TOKEN_URL = "https://oauth2.googleapis.com/token"`. `CLIENT_SECRET` 상수를 `CLIENT_ID` 옆에 두고, doc에 "Google은 설치형 앱 비밀번호를 기밀로 보지 않는다. 코드는 PKCE가 지킨다"를 적는다. `token_endpoint()`는 지우고 `tuning.token_url`을 그대로 넘긴다.
- `drive/tokens.rs`: `post(http, endpoint, form: &[(&str, &str)])`가 `.form(form)`으로 보낸다. 교환 form은 `client_id`, `client_secret`, `grant_type=authorization_code`, `code`, `code_verifier`, `redirect_uri`, 갱신 form은 `client_id`, `client_secret`, `grant_type=refresh_token`, `refresh_token`. 401 분기를 지워 `invalid_client` 같은 401은 `Unavailable`이 된다(사용자가 고칠 수 없는 앱 설정 오류). 400 `invalid_grant` → `Rejected`, 나머지 → `Unavailable`은 그대로다. 모듈 doc을 고친다.
- `secrets.rs`: 열거형 변형 `GatewaySession`과 계정 문자열 `gateway-session` 제거. `settings.rs`: 네 match 분기, 필드 3개, `is_empty` 항목 3개 제거.
- `settings/drive.rs:310` 테스트는 "모르는 옛 키가 있는 설정 파일도 읽힌다"는 회귀 방지로 유지하고 이름·주석만 고친다(리터럴 `gatewayEmail`은 옛 파일의 실제 모양이므로 남긴다).
- `inbound/tauri.rs`: 명령 4개(`load_app_access`, `sign_in_to_gateway`, `cancel_gateway_sign_in`, `sign_out_of_gateway`)와 import 제거.
- `composition.rs`: `GatewayOAuthAdapter`·`AppAccessGate`·gateway store 배선과 `generate_handler!`의 4개 항목 제거.

### 프런트엔드

- 삭제: `src/application/access-machine.ts`, `access-machine.test.ts`, `src/ui/access-controller.ts`, `access-screen.ts`, `access.dom.test.ts`.
- `src/domain/backend.ts`: `AppAccess` 타입과 `BackendPort`의 4개 메서드 제거. `src/adapters/tauri-backend.ts`: `appAccessSchema`와 invoke 4개 제거, `tauri-backend.test.ts`의 해당 스키마 테스트 제거.
- `controller.ts`: `AccessController` 제거. `start()`는 구독 실패 시 #10 이전처럼 `view.showError("네이티브 런타임에 연결할 수 없습니다. 앱을 다시 실행해 주세요.")` 후 반환하고, 성공하면 `await this.loadWorkspace()`를 부른다. `bind()`의 gateway 이벤트 4개와 `onSessionExpired` 훅 제거.
- `drive-controller.ts`: `onSessionExpired` 훅, `AUTH_REQUIRED` 분기 2곳, `reset()` 제거. `drive-machine.ts`의 `signedOut` 이벤트와 그 테스트 단언 제거.
- `drive-view.ts`: Drive 칩은 항상 연결된 계정 이메일을 보인다(계정 칩 비교 제거).
- `app-template.ts`: `#access-screen` 섹션, `#account-chip`, `#sign-out-button` 제거. `app-view.ts`: `access` 필드·생성·`setSignOutBlocked` 호출 제거. `styles.css`: `.account-chip`·`.sign-out-button`·`.access-*` 규칙 제거(`.brand-mark`, `.eyebrow`는 다른 곳에서 쓰므로 유지).
- 테스트 모의(`controller.test.ts`, `drive.dom.test.ts`, `settings-autosave.dom.test.ts`, `chatgpt-settings.dom.test.ts`)에서 4개 메서드와 로그인 전제를 지운다. `controller.test.ts`의 "native runtime 없음" 시나리오는 오류 배너 단언으로 바꾸고, 시작 시 로그인 없이 `diagnose`가 불리는 것을 단언한다. `drive.dom.test.ts`의 `AUTH_REQUIRED → 로그인 화면` 시나리오는 지우고, Drive 칩이 이메일을 보이는 단언으로 맞춘다.

### 문서

- README.md·README.en.md: "Google 계정으로 로그인" 절(78-90)과 74행 참조 제거, 196행 Drive 설명을 "앱이 Google과 직접 토큰을 교환한다"로, 254행 저장 위치 표에서 gateway 세션 행을 지우고 "이전 버전이 남긴 Credential Manager의 gateway 세션 항목과 설정 파일의 `gateway*` 키는 더 쓰지 않으며 지워도 된다"는 한 줄을 남긴다.
- INSTALL.md: `GALPI_AUTH_GATEWAY_URL` 문구(22-23)와 로그인 단계(163-177) 정리. 설치 뒤 사용자에게 로그인을 넘기는 대목이 있으면 Drive 연결 안내로 바꾼다.
- DESIGN.md: "Login Screen and Account Chip" 절(149-155) 삭제, 171행 Action order와 237행 Drive 칩 설명 정리.
- AGENTS.md: 40, 50(40→36), 51-53, 55-56, 83(`40 command paths`→36)행. `src-tauri/src/adapters/outbound/AGENTS.md` 15, 16, 19행.
- docs/ARCHITECTURE.md: 24·57·106(18→16), 19(`40 커맨드`→36), 53(40→36), 54, 76-79, 153-154.
- `.issueops/architecture/overview.md`(21의 18→16, 35의 `37 #[tauri::command]s`→36, 39-42), `.issueops/testing/overview.md`(Drive 검사), `.issueops/adr/overview.md`(새 ADR 링크). 아키텍처 개요의 결정 기록 링크는 새 ADR을 가리키게 바꾼다(ADR 파일 이름의 `auth-gateway-`는 G10 패턴에 걸리지 않는다).
- 새 ADR 1건(`project_docs_append`): "auth-gateway 의존을 없애고 Drive 데스크톱 클라이언트 비밀번호를 소스에 둔다". 2026-10-09·2026-10-11 ADR을 대체한다고 적는다. 기존 두 ADR 파일 맨 위에 대체 표시 한 줄을 넣는다. qa-ipc caution을 대체하는 caution 1건(런타임 연결 실패는 다시 앱 배너로 표시).


### 범위 추가 (1): 내장 클라이언트와 push 보호

- `drive/mod.rs`: `CLIENT_ID_B64`, `CLIENT_SECRET_B64`, `decoded()`를 지우고 평문 상수 `DEFAULT_CLIENT_ID`, `DEFAULT_CLIENT_SECRET`으로 되돌린다. doc에는 Google이 설치형 앱 값을 공개 값으로 본다는 점과, GitHub push 보호가 이 두 값을 막으면 사용자가 허용 링크로 통과시킨다는 점을 적는다. `client_id()`, `client_secret()`은 아래 `credentials()`로 대체한다.
- push 절차(커밋·push 단계): push가 GH013으로 막히면 구현 세션은 우회하지 않고 GitHub가 돌려준 허용 URL을 사용자에게 그대로 보여 준다. 사용자가 `Allow me to push this secret`을 누르면 3시간 안에 같은 커밋을 push한다. 3시간이 지나면 다시 push해 새 링크를 받는다.

### 범위 추가 (2): 사용자 지정 Google 클라이언트

동작 규칙:

- 저장된 사용자 지정 클라이언트가 없으면 내장 클라이언트를 쓴다. 저장하면 동의 화면(연결과 Picker), 코드 교환, 토큰 갱신이 모두 그 클라이언트로 간다. 토큰 폐기(`revoke`)는 토큰만 보내므로 클라이언트와 무관하다.
- Drive가 연결된 동안과 연결 동의를 기다리는 동안에는 저장도 되돌리기도 할 수 없다(`DRIVE_CLIENT_LOCKED`, "Drive 연결을 해제한 뒤에 바꿀 수 있습니다."). refresh token은 발급한 클라이언트에서만 갱신되기 때문이다. 연결 해제(`clear`)는 사용자 지정 클라이언트를 지우지 않는다.
- 보안 비밀번호는 창으로 돌려보내지 않는다. 저장에 성공하면 두 입력칸을 비우고 `저장됨` 상태만 보인다. 실패하면 입력값을 그대로 둬 고칠 수 있게 한다.
- 형식: 클라이언트 ID는 앞뒤 공백을 지운 뒤 `<숫자>-<영문 소문자·숫자>.apps.googleusercontent.com`이어야 하고, 보안 비밀번호는 앞뒤 공백을 지운 뒤 비어 있지 않고 공백 문자가 없으며 256자 이하여야 한다. 접두어(`GOCSPX-`)는 요구하지 않는다(오래된 클라이언트의 비밀번호에는 접두어가 없다). 어긋나면 `DRIVE_CLIENT_INVALID`이고, 메시지는 어느 칸이 왜 틀렸는지 한국어로 말한다("클라이언트 ID는 숫자-문자.apps.googleusercontent.com 형태여야 합니다.", "보안 비밀번호에 공백이 있으면 안 됩니다." 등).
- 저장 위치: 클라이언트 ID는 `settings.json`의 `driveClientId`, 보안 비밀번호는 기존 비밀 저장소의 `DriveClientSecret`(Windows Credential Manager `com.m16khb.galpi:drive-client-secret`, macOS는 설정 파일 `driveClientSecret`, 0600). 둘 다 있을 때만 사용자 지정 클라이언트로 본다. 저장은 ID 지우기 → 비밀번호 → ID 순서, 되돌리기는 ID → 비밀번호 순서로 써서, 처음 저장이든 덮어쓰기든 중간에 멈추면 ID가 없는 상태가 되어 "사용자 지정 아님"으로 읽히게 한다. 화면은 기존 비밀 입력칸 관례(`assistant-settings.ts:70-75` "Changing a stored key means clearing it", `DESIGN.md` Credentials)를 따라 사용자 지정 클라이언트가 저장된 동안 입력칸과 `저장`을 숨기고 `기본값으로 되돌리기`만 보인다. 백엔드는 덮어쓰기도 위 순서로 안전하게 처리한다.

IPC 명령 3개(36 → 39):

| 명령 | 인자 | 응답 |
|---|---|---|
| `load_drive_client` | 없음 | `{ custom: boolean, clientId: string \| null }` |
| `save_drive_client` | `clientId`, `clientSecret` | 같은 모양 |
| `clear_drive_client` | 없음 | 같은 모양(`custom: false`, `clientId: null`) |

Rust domain(`domain/drive.rs`):

- 값 객체 `DriveClient { id, secret }`와 `DriveClient::parse(id, secret) -> Result<DriveClient, DriveClientError>`. `DriveClientError`는 `Id | Secret` 값만 가진 enum이다(domain은 `AppError`에 의존하지 않는다). `Debug`는 비밀번호를 가리고, `Serialize`를 구현하지 않는다.
- `DriveClientStatus { custom, client_id }`(`Serialize`, camelCase). 비밀번호 필드는 없다.

Rust application:

- `ports.rs`: `DriveSessionStore`에 `load_client() -> Option<DriveClient>`, `save_client(DriveClient)`, `clear_client()`. `DriveAuthorize`와 `DriveCallback`에 `client: Option<DriveClient>`(`None`은 내장). `DriveAuthPort::refresh(client: Option<&DriveClient>, refresh_token)`. 포트 수는 16 그대로다.
- `DriveAccounts`: `client_status()`, `save_client(id, secret)`, `clear_client()`. 저장과 되돌리기는 connect와 같은 연결 슬롯을 잡고(이미 잡혀 있으면 `DRIVE_CLIENT_LOCKED`), `store_lock` 아래에서 연결 정보가 있으면 `DRIVE_CLIENT_LOCKED`, 형식이 틀리면 `DRIVE_CLIENT_INVALID`로 끝낸다. 슬롯을 함께 잡으므로 동의를 기다리는 connect와 클라이언트 변경이 겹치지 않는다. connect는 슬롯을 잡은 뒤 `load_client()`로 `DriveAuthorize.client`를 채우고, Picker(`pick`)도 같은 값을 넣는다. Picker는 연결 슬롯이 아니라 작업 슬롯을 쓰므로, Picker 동의를 기다리는 동안 연결 해제 → 클라이언트 저장 → 재연결이 일어날 수 있다. 그래서 `pick`의 `store_lock` 블록(`application/drive.rs:399-410`)은 지금의 `permission_id` 비교에 더해, 저장된 클라이언트(`load_client()`의 ID, 없으면 내장)와 `callback.client`의 ID가 다르면 새 토큰을 revoke하고 `not_connected()`로 끝낸다. 옛 클라이언트가 발급한 토큰이 새 클라이언트의 연결에 붙지 않게 하는 장치이며, 기존의 다른 계정 재연결 보호와 같은 자리다. `consent`의 교환은 `callback.client`를 쓰고, `renew_locked`는 `load_client()`로 읽은 값을 `refresh`에 넘긴다.
- `use_cases.rs`: 메서드 3개. 오류 생성 함수 `client_locked()`, `client_invalid(DriveClientError)`.

Rust adapters:

- `drive/mod.rs`: `fn credentials(client: Option<&DriveClient>) -> (&str, &str)`가 사용자 지정 값 또는 내장 상수를 돌려준다. `authorize.rs`의 동의 URL은 `request.client`, `tokens.rs`의 교환은 `callback.client`, 갱신은 넘겨받은 `client`로 `credentials()`를 부른다.
- `settings/drive.rs`: `load_client`, `save_client`, `clear_client` 구현. `secrets.rs`에 `DriveClientSecret`과 계정 이름 `drive-client-secret`. `settings.rs`의 네 match 분기, 필드 3개(`drive_client_id`, `drive_client_secret`, `drive_client_secret_stored`), `is_empty` 항목 3개.
- `inbound/tauri.rs`: 명령 3개. `composition.rs`: `generate_handler!` 등록 3줄.

프런트엔드:

- `src/domain/drive.ts`: `DriveClientStatus` 타입. `src/domain/backend.ts`: `BackendPort`에 `loadDriveClient`, `saveDriveClient(clientId, clientSecret)`, `clearDriveClient`. `src/adapters/tauri-backend.ts`: `driveClientStatusSchema = z.strictObject({ custom: z.boolean(), clientId: z.string().nullable() })`.
- `src/application/drive-machine.ts`: 상태에 `client`(`unknown | default | custom(clientId)`)와 저장 진행·메시지를 더하고, 이벤트 `clientLoaded`, `clientSaveStarted`, `clientSaved`, `clientFailed`를 더한다. 잠금은 연결 상태가 `connected`이거나 연결 중일 때로 상태에서 계산한다.
- `src/ui/drive-controller.ts`: `load()`에서 클라이언트 상태도 읽고, `saveClient`, `clearClient`를 둔다. 저장 성공 때 뷰가 두 입력칸을 비운다.
- `src/ui/app-template.ts`의 `#drive-settings` 안에 `내 Google 클라이언트 사용` 하위 절: 보이는 라벨이 있는 `#drive-client-id`(text, `autocomplete="off"`, `spellcheck="false"`)와 `#drive-client-secret`(`type="password"`, `autocomplete="off"`, `spellcheck="false"`), 상태 줄 `#drive-client-status`(`role="status"`, "기본 클라이언트를 쓰는 중입니다." / "내 클라이언트를 쓰는 중입니다 · <clientId>" / "저장됨" / 오류 문구), 잠금 안내 `#drive-client-lock`("Drive 연결을 해제한 뒤에 바꿀 수 있습니다."), 버튼 `저장`, `기본값으로 되돌리기`(기본값일 때 숨김). 연결 중에는 입력칸과 버튼을 `disabled`로 둔다. 만드는 방법은 같은 절의 `<details>`에 요약(Google Drive API·Google Picker API 사용 설정, 동의 화면과 `프로덕션` 게시, `데스크톱 앱` 클라이언트, 붙여 넣기, 새 Galpi 폴더 주의)으로 보이고, 자세한 절차는 README를 가리킨다. 접근성(`DESIGN.md` 205-208, 236, 243): 형식 오류는 틀린 칸에 `aria-invalid="true"`를 걸고, 오류 줄 `#drive-client-error`를 그 칸의 `aria-describedby`로 연결한다(도움말 ID와 함께). 포커스 이동은 `drive-view.ts`의 `FOCUS_HANDOFF` 표(26-29행 부근)에 더한다: `drive-client-reset-button`이 성공해 숨겨지면 비워진 `#drive-client-id`로, `drive-client-save-button`이 저장 성공으로 숨겨지면 `#drive-client-reset-button`으로 옮긴다. 연결이 시작돼 입력칸과 버튼이 `disabled`가 될 때는 기존 Drive 버튼 이동 규칙을 따른다.
- `src/ui/drive-view.ts`: 위 상태를 그린다. `src/styles.css`는 기존 설정 시트 클래스를 재사용하고 새 규칙이 필요하면 `--seed-*` 토큰만 쓴다.

만드는 방법(README 두 벌 `내 Google 클라이언트 사용(선택)` 절, 설정 시트 요약의 원문):

1. Google Cloud 콘솔에서 프로젝트를 만듭니다.
2. `API 및 서비스`에서 Google Drive API와 Google Picker API를 사용 설정합니다.
3. `Google 인증 플랫폼`에서 앱 정보와 대상(외부)을 설정합니다. 게시 상태가 `테스트`이면 Drive 연결이 7일 뒤 끊기므로, 앱을 `프로덕션`으로 게시합니다. `drive.file`은 민감하지 않은 범위입니다.
4. `클라이언트`에서 애플리케이션 유형 `데스크톱 앱`으로 클라이언트를 만들고, 보안 비밀번호를 바로 복사합니다. 콘솔은 만든 직후에만 보여 줍니다.
5. 만든 클라이언트 ID와 보안 비밀번호를 `설정` → `Google Drive` → `내 Google 클라이언트 사용`에 붙여 넣고 `저장`합니다. Drive가 연결돼 있으면 먼저 `연결 해제`합니다.
6. `Drive 연결`을 눌러 동의합니다. 범위는 `drive.file` 하나입니다.
7. 되돌리려면 `기본값으로 되돌리기`를 누릅니다(연결 해제 필요).

이 목록 뒤에 주의 한 단락을 둔다: 클라이언트를 바꾸면 Drive에 새 `Galpi` 폴더가 생기고 이전 클라이언트로 올린 파일은 이어지지 않는다(영문: "a new Galpi folder").

[미확인] 3번에서 민감하지 않은 범위만 쓰는 앱을 프로덕션으로 게시할 때 Google이 추가 확인을 요구하지 않는다는 점은 #12의 실측(`drive.file` 동의에 "확인되지 않은 앱" 경고 없음)과 범위 분류 문서에 근거한 추론이다. 실제 스모크에서 사용자가 만든 클라이언트로 확인하고, 다르면 README를 고친다.

### 범위 추가 문서

- README.md·README.en.md(같은 변경): `내 Google 클라이언트 사용(선택)` 절 추가(위 1~7), 저장 위치 표에 `drive-client-secret`과 `driveClientId` 추가. 내장 클라이언트를 소스에 둔 이유와 push 보호 문단은 기존 Drive 절에 한 줄로 둔다.
- INSTALL.md: Drive 연결 안내 뒤에 "자기 클라이언트를 쓰려면 README의 해당 절" 한 줄.
- DESIGN.md: 설정 시트 Google Drive 절의 새 하위 절, 잠금·저장됨·오류 상태와 비밀번호 칸 규칙.
- AGENTS.md: 50행 `36 frontend commands`와 80행 `36 command paths` → 39, Google Drive 행의 use case 설명에 사용자 지정 클라이언트 추가.
- docs/ARCHITECTURE.md: 19행 `36 커맨드` → 39, 53행 `36개`·"Drive 10개" → 39개·Drive 13개, Drive 포트 설명에 클라이언트 메서드.
- `src-tauri/src/adapters/outbound/AGENTS.md`: `settings/drive.rs`가 사용자 지정 클라이언트를 저장한다는 줄.
- `.issueops/architecture/overview.md`: 35행 36 → 39. `.issueops/testing/overview.md`: 새 테스트 묶음(`drive_client_*`)과 사용자 지정 클라이언트 실제 스모크 절차.
- ADR `2026-10-12-drop-auth-gateway-drive-client-in-source.md` 보강(Encoding 결정 교체, 사용자 지정 클라이언트 결정과 기각한 대안: 연결 중 변경 허용, 비밀번호를 창에 다시 보여 주기, 클라이언트 ID까지 비밀 저장소에 두기).
- 새 caution: "공개 저장소의 사용자 단위 push 보호는 저장소 설정 API에 보이지 않고, base64도 디코딩해 막으며, 허용은 3시간만 유효하다".

## 재사용하는 기존 구현

| 기존 구현 | 위치 | 재사용 방식 |
|---|---|---|
| form 요청 | `drive/mod.rs:181-189` `revoke`의 `.form(...)` | 토큰 교환·갱신도 같은 방식으로 보낸다. 새 의존성 없음. |
| 오류 본문 파싱 | `drive/tokens.rs:20-24` `ErrorBody` | Google 응답 `{"error":"invalid_grant",...}`에도 그대로 맞는다. |
| 토큰 응답 | `drive/tokens.rs:13-18` `TokenBody`, `tokens_from` | Google 응답 필드와 같아 그대로 쓴다. |
| PKCE·루프백·브라우저 | `browser_sign_in/` | 바꾸지 않는다. gateway 전용 코드가 없음을 확인했다(ChatGPT·Drive가 모든 공개 항목을 씀). |
| 가짜 HTTP 서버 | `browser_sign_in/fake_server.rs` `Recorded::field()` | form 필드 단언에 쓴다(ChatGPT 토큰 테스트와 revoke 테스트가 이미 씀). |
| 런타임 실패 표시 | #10 이전 `controller.ts`의 `view.showError` | 그대로 되돌린다. 새 UI를 만들지 않는다. |
| Drive 테스트 하네스 | `application/tests/drive.rs` `DriveFake`, `adapters/outbound/drive/tests.rs` `tuning()` | bearer 기록 필드만 지우고 나머지는 유지한다. |
| 범위 추가: 비밀 저장 | `SecretStore`, `LocalSettingsStore`의 네 분기(`settings.rs`), `settings/drive.rs` | 보안 비밀번호를 새 `Secret` 변형으로 저장한다. 새 저장 경로를 만들지 않는다. |
| 범위 추가: 연결 슬롯 | `DriveAccounts::claim_operation`, `OperationSlot`(`application/drive.rs:114-126`) | 클라이언트 저장·되돌리기가 connect와 겹치지 않게 같은 슬롯을 잡는다. 새 잠금을 만들지 않는다. |
| 범위 추가: 형식 검증 위치 | `domain/roster.rs`의 trim·검증 값 객체 관례 | `DriveClient::parse`를 domain에 둔다. |
| 범위 추가: IPC 경계 | `TauriBackend`의 Zod `strictObject` 파싱, `DriveStatus` 흐름 | 같은 방식으로 `DriveClientStatus`를 파싱한다. |
| 범위 추가: 설정 시트 | `#drive-settings` 절, `settings-section` 클래스, `seed.ts` 버튼 레시피, `assistant-settings.ts`의 비밀 입력칸(비우기·`저장됨` 표시) | 비밀번호 칸 동작은 AI 증강 API 키 입력칸과 같은 방식으로 맞춘다. |

새로 만드는 것은 1차의 상수 두 개(`CLIENT_SECRET`, `GOOGLE_TOKEN_URL`)와, 범위 추가의 값 객체 `DriveClient`·`DriveClientStatus`, 명령 3개, 설정 시트 하위 절이다. 사용자 지정 클라이언트를 담을 기존 타입은 없다(ChatGPT 등록 정보 `ChatGptRegistration`은 동적 등록 결과라 형식 규칙과 저장 위치가 다르다).

## 성능 영향

- hot path가 아니다. 토큰 교환은 연결·Picker 때 1회, 갱신은 Drive access token 만료 5분 전부터만 일어난다(변경 없음).
- gateway bearer를 얻던 왕복이 사라진다. 이전에는 gateway access token 만료 60초 이내일 때 Drive 토큰 요청 전에 gateway refresh 1회가 더 있었다(`application/gateway.rs:95-126`). 이제 Drive 토큰 요청은 Google 1회다.
- 앱 시작 시 gateway refresh 1회(`restore`)가 사라져 시작 경로의 네트워크 요청이 하나 줄고, 오프라인에서도 기다림 없이 작업 화면이 열린다.
- 측정: 별도 측정 없음. 위 감소는 호출 경로를 지운 결과로 확인한다(코드 삭제).

- 범위 추가: 클라이언트 읽기는 연결·Picker 동의 때 1회, 토큰 갱신 때 1회(이미 `store_lock` 아래에서 설정 파일을 읽는 자리)만 더해진다. 설정 파일 읽기 하나이며 hot path가 아니다. 명령 3개는 사용자 조작 때만 불린다.

## 하위 호환성과 side effect

- IPC: 명령 4개(`load_app_access`, `sign_in_to_gateway`, `cancel_gateway_sign_in`, `sign_out_of_gateway`)를 지운다. 프런트와 백엔드는 같은 빌드로 배포되므로 외부 소비자는 없다. 다른 명령의 이름·인자·응답은 그대로다.
- 공개 오류 코드: `AUTH_REQUIRED`, `GATEWAY_SIGN_IN_FAILED`, `GATEWAY_SIGN_IN_TIMEOUT`, `GATEWAY_SIGN_IN_BUSY`, `GATEWAY_SIGN_OUT_INCOMPLETE`, `GATEWAY_UNAVAILABLE`이 사라진다. 소비처는 같은 저장소의 프런트뿐이다. `DRIVE_*` 코드는 그대로이고, `DRIVE_UNAVAILABLE` 문구만 바뀐다.
- 설정 파일: `gatewayEmail`, `gatewaySession`, `gatewaySessionStored` 필드를 지운다. 옛 파일은 계속 읽히고(모르는 키 무시), 다음 설정 쓰기에서 키가 사라진다. 이전 버전 앱이 새 파일을 읽으면 gateway 세션이 없으므로 로그인 화면을 보인다(롤백 시 재로그인 필요).
- 비밀 저장소: Windows Credential Manager에 이전 버전이 남긴 gateway 세션 항목(계정 이름 `gateway-session`)은 코드가 지우지 않고 남는다. macOS 설정 파일의 평문 gateway 세션 필드는 다음 설정 쓰기 때 사라진다. 결정: 정리 코드를 넣지 않는다. 이유는 (1) 그 토큰은 이제 앱이 부르지 않는 gateway에만 쓸 수 있고, (2) 시작 정리 코드는 모든 사용자 기기에서 한 번만 필요한 코드를 영구히 남기며, (3) 이 저장소는 바이너리를 배포하지 않아 영향 범위가 개발자 기기 수준이다. README에 지워도 되는 항목으로 안내한다. 기각한 대안: 열거형 변형 `GatewaySession`을 정리 전용으로 남기고 시작 때 한 번 지우기.
- 저장된 Drive 연결: 기존 Drive refresh token은 같은 OAuth 클라이언트(`CLIENT_ID` 동일)가 발급했으므로 새 비밀번호로 갱신해도 유효하다 [미확인 가정: 같은 클라이언트의 다른 활성 비밀번호로 갱신하면 Google이 받아들인다. 실제 스모크에서 기존 연결 갱신으로 확인하거나, 확인할 수 없으면 새 연결로만 확인했다고 기록한다].
- 원격 side effect: Cloud 콘솔 `galpi-511214`의 `Galpi Desktop` 클라이언트에 비밀번호 1개(`****YPUH`)가 추가됐다(계획 전 이슈 단계에서 수행). 기존 `****k4Ni`는 auth-gateway가 쓰므로 건드리지 않는다. 콘솔은 활성 비밀번호가 둘이면 경고를 띄운다. gateway를 내릴 때 `k4Ni`를 중지·삭제하는 일은 비목표로 남긴다.
- 공개 저장소에 비밀번호가 커밋된다. GitHub secret scanning·push protection은 꺼져 있다(`gh api repos/m16khb-org/galpi` 실측). Google이 공개된 데스크톱 클라이언트 비밀번호를 자동으로 폐기할 가능성은 확인하지 못했다 [미확인]. 폐기되면 Drive 연결이 `DRIVE_UNAVAILABLE`로 실패하고 다른 기능은 영향이 없다. [범위 추가로 정정: 이 실측은 저장소 설정일 뿐이고, 공개 저장소에는 사용자 단위 push 보호가 기본으로 켜져 있어 실제 push가 GH013으로 막혔다. 범위 추가 사실 참고.]
- 롤백: PR을 되돌리면 앱이 다시 gateway 로그인을 요구한다. 추가한 비밀번호는 남아도 기능에 영향이 없다.
- LLM 프롬프트 본문 변경 없음.

- 범위 추가: IPC 명령 3개 추가(36 → 39), 기존 명령의 인자·응답은 그대로다. 새 오류 코드 `DRIVE_CLIENT_INVALID`, `DRIVE_CLIENT_LOCKED`.
- 범위 추가: 설정 파일에 `driveClientId`, `driveClientSecret`(macOS 평문 비밀, 0600, 기존 Drive 세션과 같은 수준), `driveClientSecretStored`를 `#[serde(default)]`로 더한다. 옛 파일은 그대로 읽히고, 이전 버전 앱은 모르는 키를 무시한다. Windows Credential Manager에 `com.m16khb.galpi:drive-client-secret`이 생긴다.
- 범위 추가: 사용자 지정 클라이언트로 연결한 상태에서 이전 버전으로 롤백하면, 이전 버전은 내장 클라이언트로 갱신을 시도해 `invalid_client` 계열 실패(`DRIVE_UNAVAILABLE`)를 낸다. 사용자는 연결을 해제하고 다시 연결하면 된다. README의 해당 절에 적는다.
- 범위 추가: Drive 동기화 연속성은 Cloud 프로젝트별로 갈린다. 루트 폴더와 결과 파일은 `appProperties`로 찾는데(`drive/rest.rs:3-6`, `rest.rs:671-693`), `appProperties`는 요청한 앱(프로젝트)에만 보이고 `drive.file`도 그 앱이 만들거나 연 파일에만 접근하게 한다([files 리소스](https://developers.google.com/workspace/drive/api/reference/rest/v3/files)). 따라서 다른 프로젝트의 클라이언트로 바꾸면 Drive에 새 `Galpi` 폴더가 생기고, 회의 폴더·녹음은 다시 올라가며, 충돌·최신 판정은 이전 파일과 이어지지 않는다. 되돌리면 사용자 지정 클라이언트로 올린 파일은 보이지 않는다(Drive에서 지워지지는 않는다). 이 사실을 README 두 벌의 해당 절, 설정 시트 `<details>` 안내, ADR 보강에 적는다. 2026-10-11 ADR의 appProperties 무상태 동기화는 클라이언트(프로젝트)마다 성립한다.
- 범위 추가: 사용자 지정 보안 비밀번호는 사용자의 값이다. 로그·IPC 응답·오류 메시지에 넣지 않는다. `DriveClient`의 `Debug`가 가리고, `Serialize`가 없다.
- 범위 추가: 내장 값을 평문으로 되돌리므로 push 때 GitHub 사용자 단위 보호가 다시 막는다. 사용자가 허용해야 push할 수 있고, 허용은 3시간 유효하다.

## 작업 순서

1. T1 Rust Drive 토큰 경로: `domain/drive.rs`, `ports.rs`(DriveAuthPort), `drive/tokens.rs`, `drive/mod.rs`, `application/drive.rs`, 해당 테스트. RED: `drive/tests.rs`의 교환 테스트를 form·client 필드 단언으로 먼저 바꿔 실패를 확인한다.
2. T2 Rust 게이트 제거: gateway 파일 삭제, `use_cases.rs`, `composition.rs`, `tauri.rs`, `settings.rs`, `secrets.rs`, `application/tests.rs`, `tests/drive.rs`.
3. T3 프런트엔드 제거와 시작 순서 복원, 테스트 갱신.
4. T4 문서·ADR·caution 갱신.
5. T5 전체 게이트, 실제 스모크(앱 창이 로그인 없이 열림, Drive 연결·Picker·업로드).


범위 추가 작업 순서(이미 끝난 T1~T5 위에서):

6. T6 내장 클라이언트 평문 상수와 `credentials()`(범위 추가 1).
7. T7 domain `DriveClient`·`DriveClientStatus`, 포트 확장, `DriveAccounts`의 저장·되돌리기·잠금, connect·pick·갱신이 저장된 클라이언트를 쓰게 함. RED: `drive_client_locked_*`, `drive_client_used_*` 테스트를 먼저 써서 실패를 확인한다.
8. T8 저장소(`Secret` 변형, 네 분기, `is_empty`)와 어댑터(동의 URL·교환·갱신).
9. T9 명령 3개, 프런트엔드(BackendPort, Zod, 상태 머신, 컨트롤러, 설정 시트), 테스트.
10. T10 문서·ADR 보강·caution.
11. T11 정리(ai-slop-clean), 원장의 G1~G11 체크를 모두 비우고 G1~G22를 `gates check --write`로 다시 채움(`gates check`는 이미 충족된 게이트를 다시 실행하지 않으므로), 실제 스모크(기본 경로 재확인과 사용자 지정 클라이언트 경로), 구현 리뷰, 커밋, push(사용자 허용), draft PR, execution complete.

## 게이트

- G1: Drive 토큰 어댑터가 Google 토큰 엔드포인트에 form으로 client id·보안 비밀번호·code·code verifier·redirect uri를 보내고, 갱신도 같은 엔드포인트에 보내며, 400 invalid_grant만 Rejected, 401·5xx·연결 실패는 Unavailable로 매핑한다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib adapters::outbound::drive
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
- G2: Drive use case가 로그인 없이 연결·가져오기·동기화·업로드하고 invalid_grant 갱신 실패만 연결을 지운다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib application::tests::drive
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
- G3: Rust 전체 테스트가 통과한다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
- G4: Rust 린트가 경고 없이 통과한다
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"cargo\",\"clippy\",\"--manifest-path\",\"src-tauri/Cargo.toml\",\"--all-targets\",\"--\",\"-D\",\"warnings\"],capture_output=True);print(\"CLIPPY_OK\" if r.returncode==0 else \"CLIPPY_FAIL\");sys.exit(r.returncode)"
  EXPECT: CLIPPY_OK
- G5: Rust 형식이 맞다
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"cargo\",\"fmt\",\"--manifest-path\",\"src-tauri/Cargo.toml\",\"--check\"],capture_output=True);print(\"FMT_OK\" if r.returncode==0 else \"FMT_FAIL\");sys.exit(r.returncode)"
  EXPECT: FMT_OK
- G6: 프런트 아키텍처 펜스, Biome, tsc가 통과한다
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"bun\",\"run\",\"check\"],capture_output=True);print(\"CHECK_OK\" if r.returncode==0 else \"CHECK_FAIL\");sys.exit(r.returncode)"
  EXPECT: CHECK_OK
- G7: 프런트 테스트가 통과한다(시작 시 로그인 없이 작업 화면을 불러오고, 런타임 연결 실패는 오류 배너로 보인다)
  CHECK: bun test
  EXPECT: /^\s*0 fail$/m
- G8: 소스에 gateway 흔적(AUTH_REQUIRED, GATEWAY_ 코드, gateway 주소, 대행 경로, 접근 게이트 심볼, gateway 명령 4개, 열거형 변형·필드·모듈 이름)이 남지 않는다. 옛 설정 파일 모양을 고정하는 `settings/drive.rs`의 `gatewayEmail` 리터럴은 패턴에 걸리지 않는다
  CHECK: python3 -c "import subprocess,sys;p='AUTH_REQUIRED|GATEWAY_|auth[.]m16khb|drive/native/token|AppAccess|app_access|[-_]gateway|gateway[-_:;]|Gateway';r=subprocess.run(['git','grep','--untracked','-nE',p,'--','src','src-tauri/src'],capture_output=True);print('GATEWAY_GONE' if r.returncode==1 else 'GATEWAY_LEFT');sys.exit(0 if r.returncode==1 else 1)"
  EXPECT: GATEWAY_GONE
- G9: 실제 스모크의 필수 단계(로그인 화면 없음, Drive 연결, Picker 가져오기, 결과 업로드, 사용자 클라이언트 저장, 사용자 클라이언트 연결, 사용자 클라이언트 Picker, 사용자 클라이언트 갱신, 연결 중 변경 거부, 기본값으로 되돌리기)가 모두 통과로 기록되고 토큰 엔드포인트가 Google임이 적혀 있다
  CHECK: python3 -c "import pathlib,sys;t=pathlib.Path('.issueops/evidence/task-16-smoke.md').read_text().splitlines();need=['로그인 화면 없음','Drive 연결','Picker 가져오기','결과 업로드','사용자 클라이언트 저장','사용자 클라이언트 연결','사용자 클라이언트 Picker','사용자 클라이언트 갱신','연결 중 변경 거부','기본값으로 되돌리기'];miss=[n for n in need if not any(n in l and '| 통과 |' in l for l in t)];ok=not miss and any('oauth2.googleapis.com/token' in l for l in t);print('SMOKE_OK' if ok else 'SMOKE_MISSING');sys.exit(0 if ok else 1)"
  EXPECT: SMOKE_OK
- G10: gateway를 전제로 쓴 문서(README 두 벌, INSTALL, DESIGN, AGENTS, docs/ARCHITECTURE, outbound AGENTS, .issueops 아키텍처·테스트 개요)에 gateway 문장과 옛 포트 수(18)·명령 수(40, 37)가 남지 않는다. `auth-gateway` 토큰은 뒤에 하이픈이 오는 ADR 파일 이름을 통과시킨다. README의 잔여물 안내 문장은 이 패턴의 토큰을 쓰지 않는다(예: '이전 버전의 로그인 세션 항목 `gateway-session`')
  CHECK: python3 -c "import subprocess,sys;p='auth[.]m16khb|GALPI_AUTH_GATEWAY_URL|gateway-brokered|drive/native/token|GatewayAuthPort|GatewaySessionStore|AppAccessGate|access-controller|access-machine|access-screen|account-chip|AUTH_REQUIRED|auth-gateway([^-]|$)|18 ports|18개|40 frontend|40 커맨드|40개|40 command|37 .#.tauri::command';r=subprocess.run(['git','grep','-nE',p,'--','README.md','README.en.md','INSTALL.md','DESIGN.md','AGENTS.md','docs/ARCHITECTURE.md','.issueops/architecture/overview.md','.issueops/testing/overview.md','src-tauri/src/adapters/outbound/AGENTS.md'],capture_output=True);print('DOCS_CLEAN' if r.returncode==1 else 'DOCS_STALE');sys.exit(0 if r.returncode==1 else 1)"
  EXPECT: DOCS_CLEAN
- G11: 두 옛 ADR(2026-10-09 auth-gateway, 2026-10-11 Drive gateway 대행)을 모두 가리키는 새 ADR이 있다
  CHECK: python3 -c "import pathlib,sys;old={'overview.md','2026-10-09-auth-gateway-google.md','2026-10-11-google-drive-auth-gateway-appproperties.md'};f=[p.name for p in pathlib.Path('.issueops/adr').glob('*.md') if p.name not in old and '2026-10-09-auth-gateway-google' in p.read_text() and '2026-10-11-google-drive-auth-gateway-appproperties' in p.read_text()];print('ADR_OK' if f else 'ADR_MISSING');sys.exit(0 if f else 1)"
  EXPECT: ADR_OK


범위 추가 게이트(테스트 이름 접두어로 묶는다: `drive_client_invalid_`, `drive_client_locked_`, `drive_client_used_`, `drive_client_secret_hidden_`, `drive_client_stored_`):

- G12: 형식 검증. 클라이언트 ID 형식 오류, 보안 비밀번호의 빈 값·공백, 앞뒤 공백 trim이 domain과 use case에서 `DRIVE_CLIENT_INVALID`로 판정된다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib drive_client_invalid_
  EXPECT: /test result: ok\. ([3-9]|[1-9][0-9]+) passed; 0 failed/
- G13: Drive가 연결된 동안의 저장(`drive_client_locked_save_while_connected`), 연결된 동안의 되돌리기(`drive_client_locked_clear_while_connected`), 연결 동의를 기다리는 동안의 저장(`drive_client_locked_save_while_consent`)과 되돌리기(`drive_client_locked_clear_while_consent`)가 `DRIVE_CLIENT_LOCKED`로 거부되고 저장소가 바뀌지 않으며, Picker 동의 대기 중 해제·저장·재연결이 일어나면 옛 클라이언트 토큰이 붙지 않는다(`drive_client_locked_picker_client_changed`)
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--lib','drive_client_locked_'],capture_output=True,text=True);o=r.stdout;need=['drive_client_locked_save_while_connected','drive_client_locked_clear_while_connected','drive_client_locked_save_while_consent','drive_client_locked_clear_while_consent','drive_client_locked_picker_client_changed'];miss=[n for n in need if not any(n in l and l.rstrip().endswith('ok') for l in o.splitlines())];ok=r.returncode==0 and not miss;print('LOCKED_OK' if ok else ('LOCKED_FAIL',miss));sys.exit(0 if ok else 1)"
  EXPECT: LOCKED_OK
- G14: 저장한 클라이언트가 동의 URL의 `client_id`(`drive_client_used_consent_url`), 교환 form의 `client_id`·`client_secret`(`drive_client_used_exchange_form`), 갱신 form(`drive_client_used_refresh_form`)에 쓰이고, 저장하지 않았으면 내장 값이 쓰이며(`drive_client_used_default_when_none`), use case가 connect(`drive_client_used_connect_passes_stored`), Picker(`drive_client_used_picker_passes_stored`), 갱신(`drive_client_used_renewal_passes_stored`)에 저장한 클라이언트를 넘긴다
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--lib','drive_client_used_'],capture_output=True,text=True);o=r.stdout;need=['drive_client_used_consent_url','drive_client_used_exchange_form','drive_client_used_refresh_form','drive_client_used_default_when_none','drive_client_used_connect_passes_stored','drive_client_used_picker_passes_stored','drive_client_used_renewal_passes_stored'];miss=[n for n in need if not any(n in l and l.rstrip().endswith('ok') for l in o.splitlines())];ok=r.returncode==0 and not miss;print('USED_OK' if ok else ('USED_FAIL',miss));sys.exit(0 if ok else 1)"
  EXPECT: USED_OK
- G15: IPC 응답 `DriveClientStatus`의 JSON 키가 `custom`, `clientId` 둘뿐이고, `DriveClient`의 `Debug`에 비밀번호가 없다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib drive_client_secret_hidden_
  EXPECT: /test result: ok\. ([2-9]|[1-9][0-9]+) passed; 0 failed/
- G16: 사용자 지정 클라이언트가 재시작 뒤에도 남고, 연결 해제(`clear`)는 그것을 지우지 않으며, 클라이언트만 있는 설정 파일이 지워지지 않고, 되돌리기 뒤 두 값이 모두 사라지며, 덮어쓰기 중간 상태(ID 없음 + 새 비밀번호)가 `custom: false`로 읽힌다(`drive_client_stored_overwrite_interrupted_reads_default`)
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--lib','drive_client_stored_'],capture_output=True,text=True);o=r.stdout;n=sum(1 for l in o.splitlines() if l.startswith('test ') and 'drive_client_stored_' in l and l.rstrip().endswith('ok'));ov=any('drive_client_stored_overwrite_interrupted_reads_default' in l and l.rstrip().endswith('ok') for l in o.splitlines());ok=r.returncode==0 and n>=4 and ov;print('STORED_OK' if ok else ('STORED_FAIL',n,ov));sys.exit(0 if ok else 1)"
  EXPECT: STORED_OK
- G17: 새 비밀 변형이 `settings.rs`의 네 match 분기와 `is_empty`에 모두 들어 있다
  CHECK: python3 -c "import re,sys,pathlib as P;t=P.Path('src-tauri/src/adapters/outbound/settings.rs').read_text();n=t.count('Secret::DriveClientSecret');m=re.search(r'fn is_empty\(&self\) -> bool \{(.*?)\n    \}',t,re.S);b=m.group(1) if m else '';ok=n>=4 and all(k in b for k in ['drive_client_id','drive_client_secret.','drive_client_secret_stored']);print('STORE_ARMS_OK' if ok else ('STORE_ARMS_FAIL',n));sys.exit(0 if ok else 1)"
  EXPECT: STORE_ARMS_OK
- G18: 프런트의 사용자 지정 클라이언트 동작(연결 중 비활성과 잠금 안내, 저장 성공 뒤 입력칸 비움과 `저장됨`, 사용자 지정 상태에서 입력칸·저장 숨김, 비밀번호 칸 속성, 형식 오류 문구와 틀린 칸의 `aria-invalid`·`aria-describedby`, 되돌리기 성공 뒤 포커스가 비워진 클라이언트 ID 칸으로 이동, 기본값일 때 되돌리기 숨김, Zod가 비밀번호 필드가 붙은 응답을 거부)이 테스트 이름에 `Drive client`를 넣은 테스트로 고정된다
  CHECK: python3 -c "import re,subprocess,sys;r=subprocess.run(['bun','test','-t','Drive client'],capture_output=True,text=True);o=r.stdout+r.stderr;p=re.search(r'^\s*(\d+) pass',o,re.M);f=re.search(r'^\s*(\d+) fail',o,re.M);ok=r.returncode==0 and p and int(p.group(1))>=8 and f and f.group(1)=='0';print('BUN_CLIENT_OK' if ok else 'BUN_CLIENT_FAIL');sys.exit(0 if ok else 1)"
  EXPECT: BUN_CLIENT_OK
- G19: 명령 39개·포트 16개가 코드와 문서에서 일치하고 옛 숫자 36이 남지 않는다
  CHECK: python3 -c "import re,sys,pathlib as P;t=P.Path('src-tauri/src/adapters/inbound/tauri.rs').read_text().count('#[tauri::command]');q=len(re.findall(r'^pub trait',P.Path('src-tauri/src/application/ports.rs').read_text(),re.M));old=['36 frontend','36 command','36 커맨드','36개','36 '+chr(96)+'#[tauri'];new=[('AGENTS.md','39 frontend'),('AGENTS.md','39 command paths'),('docs/ARCHITECTURE.md','39 커맨드'),('docs/ARCHITECTURE.md','39개'),('.issueops/architecture/overview.md','39 '+chr(96)+'#[tauri')];fs=['AGENTS.md','docs/ARCHITECTURE.md','.issueops/architecture/overview.md','src-tauri/AGENTS.md','.issueops/conventions/overview.md'];left=[(f,x) for f in fs for x in old if x in P.Path(f).read_text()];miss=[(f,x) for f,x in new if x not in P.Path(f).read_text()];ok=t==39 and q==16 and not left and not miss;print('COUNTS_OK' if ok else ('COUNTS_FAIL',t,q,left,miss));sys.exit(0 if ok else 1)"
  EXPECT: COUNTS_OK
- G20: README 두 벌에 사용자 지정 클라이언트 만드는 방법(Drive API와 Picker API 사용 설정, 데스크톱 앱 유형, 테스트 게시 상태의 7일 만료, 설정 시트 위치)과 클라이언트를 바꾸면 새 Galpi 폴더가 생긴다는 주의가 있다
  CHECK: python3 -c "import sys,pathlib as P;k=P.Path('README.md').read_text();e=P.Path('README.en.md').read_text();ko=all(x in k for x in ['내 Google 클라이언트 사용','Google Picker API','Google Drive API','데스크톱 앱','7일','새 Galpi 폴더']);en=all(x in e for x in ['Google Picker API','Google Drive API','Desktop app','7 days','new Galpi folder']);ok=ko and en;print('GUIDE_OK' if ok else ('GUIDE_FAIL',ko,en));sys.exit(0 if ok else 1)"
  EXPECT: GUIDE_OK
- G21: 내장 클라이언트가 평문 상수로 돌아왔고 base64 저장이 남지 않는다
  CHECK: python3 -c "import sys,pathlib as P;t=P.Path('src-tauri/src/adapters/outbound/drive/mod.rs').read_text();ok='_B64' not in t and 'fn decoded' not in t and 'const DEFAULT_CLIENT_ID' in t and 'const DEFAULT_CLIENT_SECRET' in t;print('PLAIN_CLIENT_OK' if ok else 'PLAIN_CLIENT_FAIL');sys.exit(0 if ok else 1)"
  EXPECT: PLAIN_CLIENT_OK
- G22: 설정 시트의 안내 요약에 만드는 방법의 핵심(Drive API·Picker API 사용 설정, 데스크톱 앱, 프로덕션 게시)과 새 Galpi 폴더 주의가 있다
  CHECK: python3 -c "import pathlib,sys;t=pathlib.Path('src/ui/app-template.ts').read_text();ok=all(x in t for x in ['Google Drive API','Google Picker API','데스크톱 앱','프로덕션','새 Galpi 폴더']);print('SHEET_GUIDE_OK' if ok else 'SHEET_GUIDE_FAIL');sys.exit(0 if ok else 1)"
  EXPECT: SHEET_GUIDE_OK

기존 게이트와의 관계: G8 패턴(`[-_]gateway|gateway[-_:;]|Gateway` 등)과 G10 패턴(`40 커맨드|40개|40 command|37 .#.tauri::command|18개` 등)은 새 이름(`drive_client`, `DriveClient`, `39`)에 걸리지 않으므로 의미가 그대로다. G9는 아래처럼 필수 단계를 늘린다.

## 실제 스모크 계획

- 앱 창: 이 기기에서 `bun run dev`로 앱을 띄워 로그인 화면 없이 작업 화면이 열리는지, 설정 시트에 계정·로그아웃이 없는지 확인한다. 엔진·모델이 없어 전사를 못 하면 그 사실과 확인한 범위를 기록한다.
- Drive: #12처럼 운영 어댑터(`DriveOAuthAdapter`, `DriveRestAdapter`)와 운영 use case(`DriveAccounts`)를 쓰는 임시 ignored 테스트 하네스로 실제 Google 동의 → 직접 교환 → 폴더 확인 → Picker 가져오기 → 결과 업로드 → 연결 해제를 실행하고, 끝나면 하네스를 지운다. 브라우저 조작은 기본 브라우저(Aside)에서 한다. 앱 창에서 연결까지 직접 해 볼 수 있으면 그것을 우선한다.
- 증거 파일 `.issueops/evidence/task-16-smoke.md`는 `| 시각(UTC) | 단계 | 결과 | 비고 |` 표로 쓰고, 필수 단계 이름(`로그인 화면 없음`, `Drive 연결`, `Picker 가져오기`, `결과 업로드`)을 단계 칸에, `통과` 또는 실패 사유를 결과 칸에 쓴다. 교환·갱신 요청이 `https://oauth2.googleapis.com/token`으로 갔음을 비고나 본문에 적는다. 토큰·code·비밀번호 값은 쓰지 않는다. 스모크가 만든 Drive 파일은 휴지통으로 옮긴다.


범위 추가 스모크:

- 기본 경로 재확인: 코드가 바뀌므로 1차 스모크의 네 단계를 개정 구현 위에서 다시 실행해 증거 파일을 새로 쓴다.
- 사용자 지정 클라이언트 값 받기: 이 컴퓨터에는 사용자 지정 클라이언트가 없다. 사용자가 README 절차대로 자기 Cloud 프로젝트에 데스크톱 클라이언트를 만든다(이 과정이 README 안내의 검증도 된다). 값은 대화나 별도 파일로 받지 않는다. 사용자가 직접 `bun run dev` 앱의 설정 시트에 붙여 넣고 저장한다. 하네스가 필요한 단계는 앱과 같은 설정 저장소(`LocalSettingsStore`, 앱 데이터 경로)에서 클라이언트와 토큰을 읽으며, 값을 출력하지 않는다.
- 사용자 지정 경로 단계: `사용자 클라이언트 저장`(설정 시트 저장, 입력칸이 비고 `저장됨`, 입력칸·저장이 숨고 되돌리기만 보임) → `사용자 클라이언트 연결`(동의 화면에 사용자 프로젝트의 앱 이름이 보이고 교환 성공, Drive에 새 `Galpi` 폴더) → `사용자 클라이언트 Picker`(가져오기 1건) → `연결 중 변경 거부`(연결된 상태에서 되돌리기가 비활성이고 잠금 안내 표시) → 앱 종료 → `사용자 클라이언트 갱신`(임시 ignored 하네스가 앱 설정 저장소에서 클라이언트와 토큰을 읽어 갱신을 강제하고 성공을 확인. `LocalSettingsStore`는 설정을 메모리에 캐시하고 문서 전체를 다시 쓰므로(`settings.rs:33-39`) 하네스를 돌리는 동안 앱은 꺼 둔다) → 앱 다시 실행 → 연결 해제 → `기본값으로 되돌리기`(상태가 기본 클라이언트로 돌아감). 하네스는 끝나면 지운다.
- 사용자가 클라이언트를 만들 수 없거나 만들지 않기로 하면, 사용자 지정 단계는 실패로 기록하고 사용자에게 범위 결정을 다시 묻는다. 그 상태로 draft PR을 발행하지 않는다.

## 롤백

PR revert. 사용자 기기에서는 gateway 재로그인이 필요하다.
