# 계획: Google Drive 녹음 가져오기와 전사 결과 Drive 동기화 (galpi#12)

- Lifecycle ID: `io-21fd3b1e1f06`
- 이슈: https://github.com/m16khb-org/galpi/issues/12
- 브랜치: `12-google-drive-sync` (base `main` @ `4c24a2479fc4bf294ffeecc81fd06ea7438894fe`)
- 선행 이슈: m16khb-org/auth-gateway#10 (`io-4bf3aa5bab5f`, `POST /drive/native/token`). 이 계획의 단위·application·어댑터 테스트는 가짜 HTTP 서버로 그 계약을 고정하므로 gateway 배포 전에도 구현·검증할 수 있다. 실제 Drive 스모크만 배포된 gateway가 필요하다.
- 사용자 요청 범위: 이슈 생성, 계획, 구현, 검증, 커밋, push, draft PR 발행, execution complete. merge와 배포는 범위 밖이다.
- 세션: 워크트리 준비 뒤 `issueops` 라우터의 환경별 자동 세션 인계(준비 세션이 Herdr 안이므로 Herdr 우선)를 적용한다. 인계는 실행 위치만 바꾸며 승인 범위를 바꾸지 않는다.

## TL;DR

- 요약: Rust 호스트에 Drive 연결(시스템 브라우저 `drive.file` 동의 + gateway 대행 토큰 교환), 데스크톱 Picker 가져오기(스트리밍 다운로드), 회의 결과 업로드·갱신(Drive `appProperties`와 `sha256Checksum`으로 무상태 충돌 판정)을 추가하고, 프런트엔드에 온보딩·연결 상태·가져오기·동기화 상태 UI를 붙인다.
- 산출물: Rust domain/application/adapter/inbound 변경과 테스트, 프런트엔드 domain/application/ui/adapter 변경과 테스트, README 두 벌·INSTALL·DESIGN·docs/ARCHITECTURE 갱신.
- 규모: Large. 병렬: 아니오(한 세션이 순서대로 구현, 조사용 하위 에이전트만 허용). 핵심 경로: T1 → T2 → T3 → T4 → T5 → T6 → T7 → T8.

## 적용되는 결정과 주의사항

| 문서 | 항목 | 이 계획에 미치는 제약 |
|---|---|---|
| `docs/ARCHITECTURE.md` §2·§145–149, `.issueops/architecture/overview.md` | 포트 소유·새 외부 능력 절차 | 새 포트는 `application/ports.rs`에 두고 outbound 구현 → `composition.rs` 배선 → `application/tests.rs` 가짜 포트를 같은 변경에서 추가한다. |
| `.issueops/adr/2026-08-23-adopt-hexagonal-ports-...` | 안쪽 의존 규칙 | Drive 값 객체·충돌 판정 규칙은 framework 없는 `domain/drive.rs`에 두고 reqwest·Tauri는 어댑터에만 둔다. |
| `.issueops/adr/2026-10-09-auth-gateway-google.md` | 기능 명령의 로그인 게이트 | 새 Drive 기능 명령(연결, 가져오기, 동기화)은 Application 메서드 첫 줄에서 `self.access.require()?`를 호출한다. 연결 해제·취소·상태 조회는 막지 않는다. |
| `.issueops/adr/2026-10-09-sign-in-with-chatgpt-assistantsettings.md` | 호스트가 토큰 갱신을 소유, tokio Mutex로 직렬화, 회전 토큰 즉시 저장 | Drive 토큰 갱신도 `application/drive.rs`가 `tokio::sync::Mutex`로 직렬화하고 갱신 결과를 즉시 저장한다. 토큰은 IPC 타입에 들어가지 않는다. |
| ADR "Windows Credential Manager에 긴 비밀을 조각으로 저장한다"(2026-10-09) | 2,560바이트 초과 비밀은 조각 저장 | Drive 토큰 레코드를 기존 비밀 저장소 trait로 저장하면 Windows에서 자동으로 조각 저장된다. 새 저장 경로를 만들지 않는다. |
| CAUTION "새 Secret 값을 추가하면 LocalSettingsStore의 네 match 분기와 is_empty를 함께 고친다"(2026-10-09) | 네 match 분기와 `is_empty` | `DriveSession` 변형 추가 때 `settings.rs`의 네 비밀 함수(L89–110, L126–179, L183–203, L211–235)와 `LocalSettings` 필드(L298–300 옆), `is_empty()`(L304–)를 모두 고치고, 빈 설정에서 저장한 Drive 세션이 재시작 뒤 남는 테스트를 추가한다. |
| `.issueops/cautions/2026-10-09-chatgpt-responses.md` | 토큰·Authorization·인가 URL 비노출, 한도·인증 오류 뒤 재전송 금지 | Drive 어댑터는 토큰·코드·`state`·인가 URL을 로그·오류 문구에 넣지 않고, 실패한 요청을 자동 재시도하지 않는다. `DriveTokens`의 `Debug`는 값을 가린다. |
| `.issueops/cautions/2026-08-23-tauri-frontend-renders-in-a-plain-browser-...` | 이벤트 구독 후 invoke | 이번 기능은 새 Tauri 이벤트를 추가하지 않는다(모든 명령이 단일 await 결과). 기존 bind-first 시작 순서를 유지한다. |
| `.issueops/cautions/2026-10-09-qa-ipc.md` | 브라우저 QA는 IPC를 가짜로 주입 | 화면 QA는 Vite 개발 서버 + 가짜 `__TAURI_INTERNALS__`로 온보딩·연결·가져오기·충돌 표시를 확인한다. |
| `.issueops/cautions/2026-08-23-dom-visibility-is-not-pixel-visibility-...` | DOM 가시성 ≠ 픽셀 가시성 | 새 상태 문구는 DOM 테스트 외에 실제 브라우저에서 `elementsFromPoint`로 가려지지 않는지 확인한다. |
| `.issueops/cautions/2026-10-09-readme.md` | README 두 벌 동시 갱신 | Drive 사용법과 저장 위치를 `README.md`와 `README.en.md`에 같은 변경 세트로 쓴다. |
| `.issueops/cautions/2026-10-09-windows-env-clear-expect-dead-code.md` | 경로 정경화는 `paths::canonical`만 | 다운로드 경로와 포함 검사는 `paths::canonical`을 쓴다. 한 플랫폼 전용 항목이 생기면 `cfg_attr` 조건을 정확히 건다. |
| `AGENTS.md` CONVENTIONS / `DESIGN.md` / `.issueops/adr/2026-10-09-adopt-seed-css-tokens-...` | SEED 토큰만, 버튼은 `src/ui/seed.ts` recipe, 상태는 색+문구, 사용자 문구 한국어 | 새 UI는 `--seed-*` 토큰과 기존 클래스만 쓰고 hex·px를 추가하지 않는다(`src/styles.test.ts`가 검사). |
| `.issueops/OPEN_API_SPEC.md` | 엔드포인트·DTO 게이트 | 이 저장소에는 HTTP 엔드포인트가 없다(IPC만). 계약 변경은 IPC 명령 6개와 Zod 스키마이며 `docs/ARCHITECTURE.md` 표를 갱신한다. |
| `.issueops/CONSTITUTION.md` | 비밀 원문 금지, ETA 금지 | 토큰·클라이언트 보안 비밀번호를 문서·테스트 픽스처·응답에 넣지 않는다. 다운로드 진행은 단계만 알리고 남은 시간을 추정하지 않는다. |

## 재사용하는 기존 구현

| 기존 구현 | 위치 | 재사용 방식 |
|---|---|---|
| 루프백 리스너 | `adapters/outbound/browser_sign_in/loopback.rs` (`Loopback::bind_preferring`, `redirect_uri`, `wait`; `Callback::get`은 모든 쿼리 값을 보관) | Drive 동의와 Picker 콜백을 그대로 받는다. `picked_file_ids`·`code`·`state`·`error`를 `Callback::get`으로 읽는다. 선호 포트도 같은 `PREFERRED_PORT`를 쓴다(점유 시 OS 할당 포트로 폴백하므로 gateway 로그인과 동시에 열려도 안전하다). |
| PKCE·난수 | `browser_sign_in/pkce.rs` (`random_urlsafe`, `code_challenge`) | Drive 인가의 verifier·state 생성 |
| 브라우저 열기 | `browser_sign_in/browser.rs` (`BrowserOpener`, `TauriBrowser`) | 인가 URL과 Picker URL을 시스템 브라우저로 연다. 새 capability 권한이 필요 없다(Rust에서 직접 연다). |
| `SignInCodes` | `browser_sign_in/mod.rs` | Drive 전용 코드(`DRIVE_CONNECT_FAILED`, `DRIVE_CONNECT_TIMEOUT`)로 인스턴스화 |
| 가짜 HTTP 서버 | `browser_sign_in/fake_server.rs` (`FakeServer`, `Recorded`) | gateway 대행 엔드포인트와 Drive REST를 흉내 내는 어댑터 테스트. 업로드 본문 검증을 위해 `Recorded`에 요청 헤더(`content-type`) 기록이 필요하면 필드 하나를 추가한다. |
| gateway 어댑터 패턴 | `adapters/outbound/gateway/mod.rs` (`Tuning`, `GatewayOAuthAdapter::with_tuning`, `PRODUCTION_URL`, 디버그 전용 `GALPI_AUTH_GATEWAY_URL`) | Drive 어댑터도 같은 `Tuning` 형태로 테스트에서 주소를 바꾼다. gateway 주소는 기존 `gateway_url()`을 재사용하도록 `pub(crate)`로 연다(새 상수를 만들지 않는다). |
| 로그인 게이트 | `application/gateway.rs` (`AppAccessGate`) | `require()`를 그대로 쓰고, gateway bearer가 필요하므로 유효한 access token을 돌려주는 `bearer()`를 추가한다(아래 T2). |
| 비밀 저장소 | `Secret` 열거형과 `SecretStore` trait(outbound 어댑터의 `secrets.rs`), `LocalSettingsStore`(`settings.rs`) | `DriveSession` 변형과 새 `drive.rs`(기존 `settings` 모듈의 `gateway.rs` 패턴)로 Drive 토큰과 연결 정보를 저장한다. |
| 작업 레지스트리 | `application/jobs.rs` (`JobRegistry::claim_with_id`, `register`, `artifacts`) | 다운로드는 작업 슬롯을 점유하고 취소를 받는다. 동기화는 등록된 산출물(`Artifacts`)을 읽는다. |
| 산출물 집합 | `domain/artifact.rs` (`Artifacts`, `ArtifactKind`, `minutes_path`) | 업로드 대상(`srt`, `speaker_text`, `minutes`)을 `path_for`로 고른다. `checkpoint`는 올리지 않는다. |
| 앱 경로 | `adapters/outbound/paths.rs` (`AppPaths.cache`, `canonical`, `sanitize_name`) | 다운로드 임시 폴더 `cache/drive-imports/<uuid>/`와 파일 이름 정리 |
| 프런트 상태 기계 패턴 | `src/application/access-machine.ts`, `chatgpt-machine.ts` | 새 `drive-machine.ts`도 불변 reducer + 같은 위치 테스트 |
| 프런트 컨트롤러 패턴 | `src/ui/chatgpt-controller.ts`, `access-controller.ts` | 새 `drive-controller.ts`가 같은 의존 주입·dispatch 구조를 따른다. |
| Zod 경계 | `src/adapters/tauri-backend.ts` | 새 응답 스키마를 같은 파일에 둔다. |

새로 만드는 것과 이유:

- Drive REST 클라이언트: 기존 코드에 Drive 호출이 없다. reqwest(`native-tls`, `form`)는 그대로 쓰고 기능을 추가하지 않는다. JSON은 기존처럼 `serde_json`으로 직렬화하고, multipart 본문은 수동으로 만들며, 다운로드는 `Response::chunk()`로 스트리밍한다(`stream` 기능 불필요).
- `DriveAccounts`(application): ChatGPT의 `ChatGptAccounts`와 같은 역할이지만 저장 형태·갱신 계약(gateway 대행, `invalid_grant`만 연결 해제)이 달라 공유하지 않는다.

## 성능 영향

- 전사 핫패스(워커, JSONL 이벤트, CPAL 녹음)는 바뀌지 않는다. 전사 명령의 결과도 바뀌지 않으며, Drive 업로드는 전사 완료 뒤 별도 IPC 호출로 실행된다.
- 다운로드: `chunk()` 단위로 `tokio::fs::File`에 써서 메모리 사용량이 파일 크기와 무관하게 청크 크기로 제한된다. 취소는 청크 사이에서 확인한다.
- 업로드: 산출물(텍스트, 수 MB 미만)을 메모리로 읽어 multipart 한 번에 보낸다. 5 MiB를 넘으면 `DRIVE_UPLOAD_TOO_LARGE`로 그 파일만 실패시킨다(Drive multipart 한도). 파일마다 메타데이터 조회 1회 + 업로드 1회이고, 회의당 최대 3개 파일이다.
- 토큰: access token 만료 5분 전부터만 갱신한다. gateway bearer도 만료 60초 전부터만 갱신한다. 평상시 요청마다 추가 왕복이 없다.
- 측정: 실제 스모크에서 50 MB 이상 녹음 다운로드 중 앱 메모리(Activity Monitor의 Galpi 프로세스 실메모리)가 다운로드 전 대비 50 MB 이상 늘지 않는지 기록한다.

## 하위 호환성과 side effect

- IPC: 기존 28개 명령의 이름·인자·응답은 바뀌지 않는다. `TranscriptionRequest`에 선택 필드 `driveSourceId`(`#[serde(default)]`)만 추가하므로 기존 프런트 요청은 그대로 파싱된다. 새 명령 7개를 추가한다(`load_drive_status`, `connect_drive`, `cancel_drive_operation`, `disconnect_drive`, `dismiss_drive_onboarding`, `import_from_drive`, `sync_meeting_to_drive`). `docs/ARCHITECTURE.md`의 명령 수(28 → 35)와 포트 수(15 → 18)를 고친다.
- 설정 파일: `settings.json`에 `driveEmail`, `drivePermissionId`, `driveFolderId`, `driveOnboardingDismissed`, `driveSession`(macOS 평문 비밀, 기존 gateway와 같은 수준), `driveSessionStored`를 `#[serde(default)]`로 추가한다. 이전 버전 파일은 그대로 읽히고, 이전 버전 앱이 새 파일을 읽어도 모르는 필드는 무시된다(`LocalSettings`는 `#[serde(default, rename_all = "camelCase")]`이고 `deny_unknown_fields`가 없다, `settings.rs` L270–271).
- Windows Credential Manager: 새 항목 `com.m16khb.galpi:drive-session`이 생긴다.
- 원격 side effect(사용자 Drive): 연결 시 `Galpi` 폴더 1개, 회의마다 하위 폴더 1개와 결과 파일 최대 3개를 만든다. 사용자가 고른 원본은 읽기만 하며 옮기거나 바꾸지 않는다. 연결 해제는 Google 토큰 폐기를 요청할 뿐 Drive 파일을 지우지 않는다.
- 로컬 side effect: 가져온 녹음은 `AppPaths.cache/drive-imports/<uuid>/`에 남고 앱 시작 때 이 폴더 전체를 지운다. 다운로드 실패·취소 때는 해당 `<uuid>` 폴더를 즉시 지운다.
- auth-gateway 계약 의존: `POST {gateway}/drive/native/token`(auth-gateway#10). gateway가 아직 배포되지 않으면 연결이 `DRIVE_UNAVAILABLE`로 실패할 뿐 기존 기능에 영향이 없다.
- 롤백: 이 PR을 되돌리면 새 설정 필드는 무시되고 Drive 비밀 항목만 남는다. 남은 항목은 기능에 영향이 없다.
- LLM 프롬프트 본문 변경 없음.

## 설계

### 도메인 (`src-tauri/src/domain/drive.rs`, 신규)

- `DriveTokens { access_token, refresh_token, expires_at }`: `Serialize/Deserialize`는 저장소 전용, 수동 `Debug`로 값을 가린다. `needs_refresh(now, margin) -> bool`.
- `DriveConnection { email: String, permission_id: String, folder_id: String }`: 비밀 아님.
- `DriveStatus`(IPC 직렬화, `tag = "state"`): `Disconnected { onboarding_dismissed: bool }`, `Connected { email: String }`.
- `DriveFileId(String)`: 비어 있지 않고 `[A-Za-z0-9_-]{1,200}`만 허용하는 값 객체(경로 주입 방지). `picked_file_ids` 파싱(`parse_picked_ids(&str) -> Result<Vec<DriveFileId>, AppError>`)도 여기 둔다.
- `MeetingKey`: `drive:<fileId>`(Drive에서 가져온 녹음) 또는 `local:<hex sha256(정경화한 원본 경로)>`. 원본 경로는 전사면 이미 정경화된 `Artifacts.source_audio`(`transcription.rs` L180), 전사문 가져오기면 정경화한 `request.input_path`다. 출력 폴더는 쓰지 않는다: `create_meeting_directory`(`paths.rs` L193–223)가 Galpi 녹음이 아닌 입력에는 실행마다 `{stem}`, `{stem} 2`, …를 새로 만들기 때문이다. `appProperties` 값 길이 한도(키+값 124바이트) 안에 들어간다. Drive 회의 폴더 이름은 원본 파일의 stem(Drive 원본이면 Drive 파일 이름의 stem)이다.
- `SyncDecision`: 순수 함수 `decide(remote: Option<RemoteFile>, local_sha256: &str) -> SyncDecision` — 원격 없음 → `Create`; 원격 `sha256Checksum == appProperties.galpiSha256 == local` → `Unchanged`; 원격 `sha256Checksum == appProperties.galpiSha256 != local` → `Update`; 원격 `sha256Checksum`이 없거나 `galpiSha256`과 다르면 → `Conflict`. `overwrite`가 참이면 `Conflict` 대신 `Update`.
- `DriveSyncState`(IPC): `Uploaded`, `Updated`, `Unchanged`, `Conflict`, `Failed { message }`; `DriveSyncFile { kind: ArtifactKind, state, web_view_link: Option<String> }`; `DriveSyncResult { folder_link: Option<String>, files: Vec<DriveSyncFile> }`. `ArtifactKind`에 `Serialize`를 추가한다(현재 `Deserialize`만, `domain/artifact.rs` L19).
- `DriveImport { job_id, input_path, drive_file_id, name }`(IPC 결과).
- `parse_rfc3339_utc` 같은 날짜 파싱은 domain에 두지 않는다. Drive `modifiedTime` 파싱은 어댑터가 기존 `chrono` 의존으로 한다(필요하면 기존 `chrono` 의존의 기능 플래그만 추가하고 새 크레이트는 넣지 않는다).

### application

- `ports.rs`에 포트 3개:
  - `DriveAuthPort`: `authorize(&self, request: DriveAuthorize, cancel) -> Result<DriveCallback, AppError>`(브라우저 동의 또는 Picker: `picker: bool`, `login_hint: Option<String>`; 결과는 `code`, `redirect_uri`, `verifier`, `picked: Vec<DriveFileId>`), `exchange(bearer, code, verifier, redirect_uri) -> Result<DriveTokens, DriveTokenFailure>`, `refresh(bearer, refresh_token) -> Result<DriveTokens, DriveTokenFailure>`, `revoke(refresh_token)`(최선 노력, Google `https://oauth2.googleapis.com/revoke` 직접 호출).
  - `DriveFilesPort`: `whoami(access) -> DriveAccount{email, permission_id}`, `ensure_root_folder(access, known: Option<&str>) -> String`, `ensure_meeting_folder(access, root, key: &MeetingKey, name) -> RemoteFolder{id, link}`, `find_artifact(access, folder, key, kind) -> Option<RemoteFile>`, `create_file(...)`, `update_file(...)`, `file_metadata(access, id) -> PickedFile{name, mime_type, size, can_download, modified_time}`, `download(access, id, dest, modified_time, cancel) -> Result<(), AppError>`(완료 직전 `.part` 파일의 수정 시각을 Drive `modifiedTime`으로 맞춘다: 증강은 `recorded_on`(`refinement.rs` L72–82)에서 원본의 수정 시각을 회의 날짜로 쓰기 때문이다. `std::fs::File::set_modified` 사용, rust-version 1.88).
  - `DriveSessionStore`: `load_status`, `load_connection`, `load_tokens`, `save_connection(conn, tokens)`, `replace_tokens`, `save_folder_id(id)`(연결 정보 중 폴더 ID만 바꾸는 좁은 쓰기), `dismiss_onboarding`, `clear`.
- `DriveTokenFailure { Rejected, AuthRequired, Unavailable }`: gateway 400 `invalid_grant` → Rejected, 401 → AuthRequired, 그 밖(503 `temporarily_unavailable`·`drive_unavailable`, 연결 실패, 형식 오류) → Unavailable.
- `AppAccessGate::bearer() -> Result<String, AppError>`: `writes` 잠금 아래 저장 토큰을 읽고 `expires_at - 60 > now`면 그대로, 아니면 `auth.refresh` → 성공 시 `replace_tokens` 후 새 access token; Rejected → `store.clear()` + `SignedOut` + `AUTH_REQUIRED`; Unavailable → `GATEWAY_UNAVAILABLE`("로그인 서버에 연결하지 못해 Drive 작업을 할 수 없습니다."). 이를 위해 `AppAccessGate::new`에 `clock: Arc<dyn ClockPort>`를 추가하고 `composition.rs`(L70)와 테스트 생성부를 고친다.
- `application/drive.rs` `DriveAccounts`(ChatGptAccounts 구조): 필드 `auth`, `files`, `store`, `clock`, `store_lock: tokio::sync::Mutex<()>`(토큰 갱신과 모든 Drive 저장소 쓰기, 연결 해제의 `clear`를 직렬화), `operation_active: AtomicBool`, `operation_cancel: Mutex<Option<oneshot::Sender<()>>>`(연결 대기 전용). gateway bearer는 값으로 받지 않고 `&AppAccessGate`를 받아 **gateway를 호출하기 직전마다** `access.bearer().await`로 구한다. 브라우저 대기(최대 `SIGN_IN_TIMEOUT` 5분, `loopback.rs` L20) 동안 gateway access token이 만료돼도 교환에는 갱신된 bearer를 쓰기 위해서다. 메서드:
  - `status()`.
  - `connect(access)`: 연결 대기 슬롯 점유(동시 실행 `DRIVE_BUSY`) → `authorize(picker=false, operation_cancel)` → `access.bearer()` → `exchange` → `whoami` → `ensure_root_folder(None)` → `store_lock` 아래 `save_connection`. `exchange` Rejected → `DRIVE_CONNECT_FAILED`.
  - `access_token(access)`: `store_lock` 아래 연결이 남아 있는지 확인하고, 만료 5분 전이면 `access.bearer()` → gateway 갱신 → `replace_tokens`. Rejected → `clear()` 후 `DRIVE_RECONNECT_REQUIRED`("Drive 연결이 만료되었습니다. 다시 연결해 주세요."); AuthRequired → `AUTH_REQUIRED`; Unavailable → `DRIVE_UNAVAILABLE`. `force_refresh(access)`는 만료와 무관하게 같은 절차를 한 번 수행한다(Drive 401 처리용).
  - `pick(access, cancel)`: 연결 필수(`DRIVE_NOT_CONNECTED`) → `authorize(picker=true, login_hint=email, cancel)` → `access.bearer()` → `exchange` → `whoami`가 저장된 `permission_id`와 다르면 새 refresh token을 `revoke`하고 `DRIVE_ACCOUNT_MISMATCH` → 같으면 `store_lock` 아래 연결이 아직 남아 있을 때만 새 토큰으로 `replace_tokens`(가장 최근 동의를 보관; 그 사이 연결이 해제됐으면 새 토큰을 `revoke`하고 `DRIVE_NOT_CONNECTED`) → `picked`가 정확히 1개인지 확인(0개 `CANCELLED`, 2개 이상 `DRIVE_PICK_INVALID`). `cancel`은 호출자가 넘긴 작업 슬롯의 수신기다.
  - `disconnect()`: 연결 대기 취소 → `store_lock` 획득 → 저장 토큰 `revoke`(결과 무시) → `clear()`. 이후 진행 중이던 동기화·Picker가 저장소에 쓰려 하면 연결이 없으므로 쓰지 않는다.
  - `cancel()`: 연결 대기 취소.
- `Application`(use_cases.rs)에 `drive: DriveAccounts`와 `drive_meetings: Mutex<HashMap<Uuid, MeetingKey>>`, `drive_imports: Mutex<HashMap<PathBuf, DriveFileId>>`를 추가하고 새 메서드:
  - `load_drive_status()`, `dismiss_drive_onboarding()`, `cancel_drive_operation()`, `disconnect_drive()`: 게이트 없음.
  - `connect_drive()`: `require()` → `drive.connect(&self.access)`.
  - `import_from_drive(job_id)`: `require()` → **먼저** `jobs.claim_with_id(job_id)`로 작업 슬롯을 점유하고, 그 취소 수신기를 Picker 대기와 다운로드에 함께 쓴다(기존 `#cancel-button` → `cancel_job` 하나로 두 단계 모두 취소, Picker 대기 중에도 `JOB_NOT_FOUND`가 나지 않는다) → `drive.pick(&self.access, &mut cancel)` → `file_metadata`로 `can_download`와 MIME(`audio/*` 또는 `video/*`, Google 문서 형식 거부 → `DRIVE_UNSUPPORTED_FILE`) 확인 → `cache/drive-imports/<uuid>/<sanitize_name(name)>`로 `download` → 결과 경로를 `drive_imports`에 기록 → `DriveImport` 반환. 실패·취소 때 `<uuid>` 폴더 삭제는 어댑터가 한다.
  - `transcribe`(기존): `request.drive_source_id`가 있으면 그 값이 `drive_imports`에 기록된 경로·ID와 일치할 때만 `MeetingKey::Drive`로, 없으면 `MeetingKey::Local(source_audio)`로 `drive_meetings`에 기록한다(완료 후). 불일치는 `DRIVE_SOURCE_MISMATCH`로 전사 전에 거부.
  - `import_transcript`(기존): 완료 후 정경화한 입력 경로로 `MeetingKey::Local`을 기록.
  - `sync_meeting_to_drive(job_id, overwrite: bool)`: `job_id`는 **전사 또는 전사문 가져오기 작업의 ID**다(증강 결과는 `register_minutes(target, …)`로 그 작업에 등록되므로 증강 뒤에도 같은 ID를 쓴다). `require()` → 연결 필수 → `jobs.artifacts(job_id)`와 `drive_meetings[job_id]` → `drive.access_token(&self.access)` → 루트 폴더 확인(`ensure_root_folder(Some(saved))`, 바뀌면 `store_lock` 아래 연결이 남아 있을 때만 `save_folder_id`) → 회의 폴더(`appProperties.galpiMeeting = key`) → 산출물별로 `find_artifact` → `decide` → `create_file`/`update_file`(모두 `appProperties { galpiMeeting, galpiKind, galpiSha256 }` 포함) → 파일별 결과를 모은다. 한 파일의 실패는 `Failed`로 담고 나머지를 계속한다. Drive 401이면 `drive.force_refresh`로 한 번 갱신한 뒤 원래 요청을 다시 보내지 않고 위 오류 규칙으로 끝낸다. 토큰·연결 오류(`DRIVE_RECONNECT_REQUIRED`, `AUTH_REQUIRED`, `DRIVE_UNAVAILABLE`, `DRIVE_NOT_CONNECTED`)는 전체 `Err`로 돌려준다. 작업 슬롯은 쓰지 않는다(전사·증강과 병행 가능).
- 앱 시작 정리: `composition.rs`의 setup에서 `cache/drive-imports`를 지운다(실패는 무시, 로그만).

### 어댑터 (`src-tauri/src/adapters/outbound/drive/`, 신규)

- `mod.rs`: `DriveOAuthAdapter`(`DriveAuthPort`)와 `DriveRestAdapter`(`DriveFilesPort`), `Tuning { gateway_url, google_auth_url, google_revoke_url, drive_api_url, drive_upload_url, preferred_port, sign_in_timeout }`. 프로덕션 상수: `CLIENT_ID = "747212408320-1io6clrt6t568ot424pv54lepsvfl9pf.apps.googleusercontent.com"`(공개 값), `SCOPE = "https://www.googleapis.com/auth/drive.file"`. 클라이언트 보안 비밀번호는 없다.
- `authorize.rs`: 인가 URL 조립 — 공통 `client_id`, `redirect_uri`, `response_type=code`, `scope`, `code_challenge`(S256), `state`, `access_type=offline`, `prompt=consent`; 연결 때 `include_granted_scopes`는 넣지 않는다; Picker 때 `trigger_onepick=true`, `mimetypes=audio/mpeg,audio/mp4,audio/x-m4a,audio/aac,audio/wav,audio/x-wav,audio/flac,audio/ogg,video/mp4,video/quicktime`, `login_hint`. 콜백: `state` 불일치 → `DRIVE_STATE_MISMATCH`, `error=access_denied` → `CANCELLED`, 그 밖 `error` → `DRIVE_CONNECT_FAILED`, `code` 없음 → `DRIVE_CONNECT_FAILED`.
- `tokens.rs`: `POST {gateway}/drive/native/token`에 gateway access token bearer 헤더와 `Content-Type: application/json`을 붙인다. 본문은 `{"grant_type":"authorization_code","code","code_verifier","redirect_uri"}` 또는 `{"grant_type":"refresh_token","refresh_token"}`. 200 응답에서 `access_token`, `expires_in`, `refresh_token`(갱신 응답에 없으면 기존 값 유지). 오류 매핑은 위 `DriveTokenFailure`.
- `rest.rs`: Drive v3 호출. 모든 요청 `Authorization: Bearer`, `connect_timeout 10s`, 요청 `timeout 60s`(다운로드는 `timeout` 없이 청크 사이 읽기 제한 60s), 리다이렉트 정책 기본.
  - `about?fields=user(emailAddress,permissionId)`.
  - 루트 폴더 찾기: `files?q=` `mimeType='application/vnd.google-apps.folder' and trashed=false and appProperties has { key='galpiRoot' and value='1' }` `&fields=files(id)` → 없으면 `POST files` `{name:"Galpi", mimeType: folder, appProperties:{galpiRoot:"1"}}`. `known`이 있으면 먼저 `files/{id}?fields=id,trashed`로 확인하고 404·trashed면 찾기/만들기.
  - 회의 폴더: `appProperties has { key='galpiMeeting' and value='<key>' } and '<root>' in parents and trashed=false` → 없으면 생성(`parents:[root]`, 이름 = 회의 폴더 이름). `webViewLink`를 함께 받는다.
  - 산출물 찾기: `'<meeting>' in parents and trashed=false and appProperties has { key='galpiKind' and value='<kind>' }` `fields=files(id,sha256Checksum,appProperties,webViewLink)`.
  - 생성: `POST {upload}/files?uploadType=multipart&fields=id,webViewLink` multipart/related(경계 문자열은 `random_urlsafe`), 메타데이터 `{name, parents:[meeting], mimeType, appProperties}`.
  - 갱신: `PATCH {upload}/files/{id}?uploadType=multipart&fields=id,webViewLink` 메타데이터 `{appProperties}`.
  - MIME: srt `application/x-subrip`, txt `text/plain`, md `text/markdown`.
  - `q` 값의 작은따옴표·역슬래시는 이스케이프한다(`MeetingKey`·ID는 값 객체로 이미 안전 문자만 허용).
  - 다운로드: `files/{id}?alt=media`를 `<dest>.part`에 청크로 쓰고 완료 후 `rename`. 취소(`cancel` 수신)나 오류 때 상위 `<uuid>` 폴더를 지운다. HTTP 403 `cannotDownloadFile`/`fileNotDownloadable` 등은 `DRIVE_DOWNLOAD_FAILED`.
  - Drive API 오류 매핑: 401 → 어댑터는 `DRIVE_TOKEN_REJECTED`(내부 코드)를 돌려주고, `DriveAccounts`가 만료와 무관하게 gateway 갱신을 한 번 강제한다. 갱신이 Rejected면 연결 정보를 지우고 `DRIVE_RECONNECT_REQUIRED`, 성공하면 새 토큰을 저장하고 원래 요청은 다시 보내지 않은 채 `DRIVE_UNAVAILABLE`("잠시 뒤 다시 시도해 주세요")로 끝낸다(자동 재전송 금지 규칙). 403 `storageQuotaExceeded` → `DRIVE_STORAGE_FULL`, 404 → 호출 지점별 처리(폴더 재생성), 429·5xx → `DRIVE_UNAVAILABLE`.
- `drive.rs`(`settings` 모듈 아래, 기존 `gateway.rs` 구조): `DriveSessionStore for LocalSettingsStore`, 토큰은 `DriveSession` 변형(account `drive-session`).
- `secrets.rs`: `Secret` 열거형에 `DriveSession` 변형과 account 문자열 추가.
- `gateway/mod.rs`: `gateway_url()`을 `pub(crate)`로 열어 Drive 어댑터가 같은 주소(디버그 덮어쓰기 포함)를 쓴다.

### inbound·composition

- `adapters/inbound/tauri.rs`: 명령 7개(얇게 위임). `import_from_drive(job_id)`, `sync_meeting_to_drive(job_id, overwrite)`.
- `TranscriptionRequest`(`domain/job.rs` L24): `#[serde(default)] pub drive_source_id: Option<String>`.
- `composition.rs`(기존 `.setup(|app| …)` L28 안): `DriveOAuthAdapter::new(browser.clone())?`, `DriveRestAdapter::new(paths)?`, `DriveAccounts::new(...)`, `AppAccessGate::new(gateway_auth, gateway_store, clock.clone())`, `Application::new(..., drive)`, `generate_handler!`에 7개 등록, setup에서 `drive-imports` 정리.

### 프런트엔드

- `src/domain/drive.ts`(신규): `DriveStatus`, `DriveImport`, `DriveSyncResult`, `DriveSyncFile`, `DriveSyncState`.
- `src/domain/backend.ts` `BackendPort`: `loadDriveStatus`, `connectDrive`, `cancelDriveOperation`, `disconnectDrive`, `dismissDriveOnboarding`, `importFromDrive(jobId)`, `syncMeetingToDrive(jobId, overwrite)`; `startTranscription` 요청에 선택 `driveSourceId`.
- `src/adapters/tauri-backend.ts`: 위 명령의 `invoke`와 Zod 스키마(`driveStatusSchema`, `driveImportSchema`, `driveSyncResultSchema`).
- `src/application/drive-machine.ts`(신규, 불변 reducer + `drive-machine.test.ts`): 상태 `unknown → disconnected{onboarding} | connected{email}`; 작업 `idle | connecting | importing | syncing`(가져오기는 Picker 대기와 다운로드를 한 단계로 보여 준다: 둘 사이를 알리는 이벤트가 없기 때문이다); 동기화 결과(job별 마지막 `DriveSyncResult`), 오류 문구. 이벤트: `statusLoaded`, `connectStarted/Succeeded/Failed/Cancelled`, `onboardingDismissed`, `disconnected`, `importStarted/Imported/Failed`, `syncStarted/Finished/Failed`, `reconnectRequired`(→ disconnected, onboarding 다시 표시).
- `src/ui/drive-controller.ts`(신규): 시작 때 `loadDriveStatus`; `connect`, `cancel`, `disconnect`, `dismiss`, `importFromDrive`, `syncAfter(meetingJobId)`(연결 상태일 때 자동 호출), `overwrite(meetingJobId)`. 가져오기는 `AppController`가 넘긴 콜백 계약으로 작업 수명 주기 전체를 정리한다: 시작할 때 `onImportStarted(jobId)`(AppController가 `begin("driveImport", jobId)`로 busy 상태와 `this.job.jobId`를 설정), 성공 시 `onImported({ path, driveSourceId, name })`(오디오 선택 값을 교체하고, 전사 전용인 `completeJob`(`job-machine.ts` L95, `TranscriptionResult`를 받음)은 쓰지 않는다. 대신 `this.job = initialJobState`(L15)로 되돌리고 새 뷰 메서드 `renderDriveImported(name)`을 부른다. 이 메서드는 기존 `renderImportedTranscript`(`app-view.ts` L353–363)처럼 `transcriptionSummary = false`로 두고 `#job-panel`을 숨기며, `#audio-path`에 가져온 파일 이름을 표시한다. 그래서 완료 요약 행 "전사 완료"(`app-template.ts` L146, `styles.css` L1526·L1546)가 전사하지 않은 파일에 보이지 않는다), 실패·취소 시 `onImportFailed(error)`(AppController가 `handleFailure(error)`로 `#job-panel`을 `data-status="failed"`로 두고 `#error-message`에 이유를 표시; 취소는 `cancel()`의 `cancelJob`이 먼저 `cancelled`로 두고 뒤이은 실패를 `failJob`이 무시, `job-machine.ts` L107–116), 끝나면 성공·실패와 관계없이 `finally`에서 `onImportFinished()`(AppController가 `setBusy(null)`). `app-view.ts`의 `renderJobTitle`(L458–466)과 `busyLabel`(L624–627)에 `driveImport` 분기를 추가한다: 진행 "Drive에서 녹음을 가져오고 있습니다", 실패 "Drive에서 녹음을 가져오지 못했습니다", 취소 "Drive 가져오기를 취소했습니다", busy 라벨 "Drive 가져오기". Drive 명령이 `AUTH_REQUIRED`로 실패하면 `AccessController.expire()`를 부른다. `access-machine.ts`에 `signedIn`에서 `signedOut`으로 가는 새 이벤트 `sessionExpired`를 추가하고, `expire()`는 이 이벤트를 dispatch한 뒤 기존 `onClosed`를 호출해 로그인 화면으로 돌아간다(기존 `check()`는 `signedIn`에서 아무 일도 하지 않으므로 쓰지 않는다, `access-controller.ts` L31–33, `access-machine.ts` L41–42).
- `src/ui/controller.ts`: 오디오 선택 상태를 `audioPath` 하나에서 `{ path, driveSourceId? }` 한 값으로 바꾸고, 경로를 바꾸는 모든 지점(녹음 완료 콜백 L41–44, `chooseAudio` L325, Drive 가져오기)에서 이 값을 통째로 교체한다. 그래서 Drive에서 가져온 뒤 마이크로 녹음하거나 로컬 파일을 고르면 `driveSourceId`가 남지 않는다. 전사 완료(`TranscriptionResult` 수신, L400) 직후 `syncAfter(result.jobId)`, 전사문 가져오기 완료(L353) 직후 `syncAfter(imported.jobId)`, 증강 완료 직후에는 증강 요청의 `target`과 같은 `this.lastResult.jobId`(L426·L436)로 `syncAfter`를 부른다(증강 결과의 `jobId`가 아니다).
- `src/ui/app-template.ts` 추가 요소(선택자 고정):
  - 온보딩 카드 `#drive-onboarding`(transcription-panel 위, 연결 안 됨 + 미해제일 때만 표시): 제목 "Google Drive 연결", 설명 "Drive에 있는 녹음을 가져오고 전사 결과를 내 Drive의 Galpi 폴더에 자동으로 올립니다. Galpi는 직접 만든 파일과 직접 고른 파일에만 접근합니다.", 버튼 `#drive-connect-button`(`data-action="connect-drive"`, primary) "Drive 연결", `#drive-dismiss-button`(`data-action="dismiss-drive-onboarding"`, ghost) "나중에", `#drive-onboarding-status`(`role="status"`).
  - 상단바 `#drive-chip`(account-chip 옆, 연결 시 표시): `#drive-email`과 상태 문구.
  - `#drive-import-button`(`data-action="import-from-drive"`, `#audio-selection` 옆): "Drive에서 가져오기". 활성 조건은 "Drive 연결됨 그리고 `!jobBusy && !recordingActive`"이며, 다른 작업 시작 버튼과 같이 `app-view.ts` `refreshActions`(L546–568)에서 계산한다. 그래서 전사·증강·엔진 준비·녹음 중에는 누를 수 없고 실행 중인 작업의 `this.job`과 busy 상태를 덮어쓰지 않는다. 진행 중에는 기존 `#job-panel`이 "Drive에서 녹음을 고르고 내려받는 중입니다" 단계를 보여 준다. 이를 위해 `app-view.ts`의 `#job-panel` 숨김 조건(L241 `setBusy`, L289–290 `renderJob`)과 `data-status` 설정(L234–237, L293–294)에 `jobKind === "driveImport"`를 `"transcription"`과 같이 포함해 패널을 보이고 `data-status="running"`으로 둔다. `#cancel-button` 표시 조건(L245–246)도 `"transcription"` 또는 `"driveImport"`로 넓힌다. `controller.ts` L411–417의 `cancel()`은 그대로 `this.job.jobId`(가져오기 작업 ID)로 `cancel_job`을 부르므로 Picker 대기와 다운로드를 모두 취소한다.
  - 결과 영역 `#drive-sync-row`(`#results-panel`과 `#augment-panel` 산출물 목록 아래): 상태 문구 `#drive-sync-status`(`role="status"`), 파일별 상태, Drive 폴더 열기 링크 버튼 `#drive-open-folder-button`(기존 opener 경로로 `webViewLink` 열기 — capability allow 목록에 `https://drive.google.com/*` 추가), 충돌 시 `#drive-overwrite-button`(`data-action="overwrite-drive"`, danger) "Drive 버전 덮어쓰기".
  - 설정 대화상자에 "Google Drive" 절 `#drive-settings`: 연결 계정, `#drive-settings-connect-button`, `#drive-disconnect-button`(danger) "연결 해제".
  - 연결 대기 취소: 온보딩 카드에 `#drive-onboarding-cancel-button`, 설정 절에 `#drive-settings-cancel-button`(둘 다 `data-action="cancel-drive-operation"`, secondary) "취소"를 두고 연결 중일 때만 표시한다. id를 나누는 이유는 하위 뷰가 root의 `querySelector`로 첫 요소만 찾고(`dom.ts` L12), 설정 대화상자가 열리면 배경이 inert가 되어(`token-settings.ts` L56–60) 온보딩 쪽 버튼을 누를 수 없기 때문이다.
- 사용자 문구(오류 코드별): `DRIVE_NOT_CONNECTED` "먼저 Google Drive를 연결해 주세요.", `DRIVE_ACCOUNT_MISMATCH` "연결된 Drive 계정({email})과 다른 계정에서 파일을 골랐습니다.", `DRIVE_RECONNECT_REQUIRED`, `DRIVE_UNAVAILABLE` "Google Drive 또는 로그인 서버에 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요.", `DRIVE_UNSUPPORTED_FILE` "오디오나 영상 파일만 가져올 수 있습니다.", 충돌 "Drive의 파일이 다른 곳에서 바뀌어 덮어쓰지 않았습니다.". 상태는 색과 문구를 함께 쓴다(DESIGN.md).
- `src/styles.css`: 새 요소의 배치만 기존 토큰으로 추가한다. `DESIGN.md`에 Drive 온보딩·칩·동기화 상태 절을 추가한다.

## TODOs

- [ ] 1. 도메인 값 객체와 충돌 판정
  - What: `domain/drive.rs`(위 설계), `domain/mod.rs` 등록, `ArtifactKind`에 `Serialize`(snake_case) 추가.
    - Must NOT: reqwest·Tauri·tokio 의존.
  - References: `domain/gateway.rs` L11–28(비밀 값 `Debug` 가리기 패턴), `domain/artifact.rs` L19–25.
  - Acceptance(RED→GREEN): `cargo test --manifest-path src-tauri/Cargo.toml --lib domain::drive` — `decide`의 다섯 경우(원격 없음, 같음, 로컬만 바뀜, 원격 바뀜, 원격 체크섬 없음)와 `overwrite`, `DriveFileId` 거부 사례(빈 값, `'`, `/`, 201자), `parse_picked_ids`(빈 문자열, 쉼표 목록), `MeetingKey` 형식과 124바이트 한도, `DriveTokens` `Debug`에 값이 없음.
  - Commit: NO(한 PR, 마지막에 원자 커밋으로 나눔 — Commit Strategy 참고).
- [ ] 2. 로그인 게이트 bearer
  - What: `AppAccessGate::new(.., clock)`, `bearer()`.
  - References: `application/gateway.rs` L77–103(restore의 갱신·거부·오프라인 분기), L140–160.
  - Acceptance: `application/tests/gateway.rs`에 — 만료 전이면 refresh 0회, 만료 60초 안이면 refresh 1회 + 회전 토큰 저장, Rejected면 저장소 비움 + `require()`가 `AUTH_REQUIRED`, Unavailable면 `GATEWAY_UNAVAILABLE`이고 세션 유지, 저장 토큰 없음 → `AUTH_REQUIRED`. 기존 gateway 테스트 그대로 통과.
- [ ] 3. Drive 저장소
  - What: `Secret` 열거형의 `DriveSession` 변형, `settings.rs` 네 match + 필드 + `is_empty`, `settings` 모듈 아래 `drive.rs`.
  - Acceptance: `cargo test ... --lib settings::drive` — SettingsFile 백엔드에서 빈 설정에 저장한 연결·토큰이 새 store로 다시 읽힘, OS 비밀 저장소 백엔드에서는 파일에 토큰 문자열이 없음, `clear()` 뒤 다른 설정이 없으면 파일 삭제, `dismiss_onboarding` 유지, 이전 버전 JSON(Drive 필드 없음) 파싱.
- [ ] 4. Drive 인증 어댑터
  - What: `drive/mod.rs`, `authorize.rs`, `tokens.rs`, gateway URL 공유.
  - References: `gateway/mod.rs` L131–145(인가 URL), L162–192(교환), L215–280(sign_in·refresh·revoke), `gateway/tests.rs` L76–110(가짜 서버 하네스).
  - Acceptance: `cargo test ... --lib drive::tests` — 연결 URL의 필수 파라미터와 Picker 추가 파라미터, 콜백 `state` 불일치·`access_denied`·`code` 없음, 교환 요청의 bearer·JSON 본문, 교환 응답에서 토큰만 추출, 400 `invalid_grant` → Rejected, 401 → AuthRequired, 503·연결 거부·형식 오류 → Unavailable, 갱신 응답에 refresh token 없으면 기존 값 유지, revoke가 실패해도 패닉 없음, 오류 문구에 토큰·코드가 없음.
- [ ] 5. Drive REST 어댑터
  - What: `rest.rs`.
  - Acceptance: 가짜 서버 테스트 — about 파싱, 루트 폴더 재사용/404 시 재생성/없으면 생성(`appProperties.galpiRoot`), 회의 폴더 재사용과 생성, multipart 본문(경계·메타데이터 JSON·내용), 갱신이 `PATCH`, `q` 이스케이프, 다운로드가 `.part`를 거쳐 최종 파일로 rename되고 최종 파일의 수정 시각이 가짜 서버가 준 `modifiedTime`과 같음, 다운로드 중 cancel 시 폴더 삭제와 `CANCELLED`, 403·404·429·5xx 매핑, 5 MiB 초과 업로드 거부.
- [ ] 6. DriveAccounts와 Application use case
  - What: `application/drive.rs`, `use_cases.rs` 새 메서드·필드, `ports.rs` 포트 3개, `application/tests.rs` 가짜 포트, `application/tests/drive.rs`.
  - Acceptance(GIVEN/WHEN/THEN): 로그인 전 `connect_drive`·`import_from_drive`·`sync_meeting_to_drive`가 `AUTH_REQUIRED`; 연결 성공 시 연결 정보 저장; 동시 연결 `DRIVE_BUSY`; 연결 대기 중 gateway 토큰이 만료 구간에 들어가도 교환에는 대기 뒤 갱신된 bearer가 쓰임(가짜 시계를 대기 중에 전진); 연결 대기를 `cancel_drive_operation`으로 취소하면 `CANCELLED`이고 다시 연결할 수 있음; 가져오기 계정 불일치 → `DRIVE_ACCOUNT_MISMATCH` + 새 토큰 미저장 + revoke 1회; 같은 계정이면 새 토큰 저장; Google 문서 MIME 거부; Picker 대기 중 `cancel_job`이 `CANCELLED`로 끝나고 슬롯을 놓음; 다운로드 중 취소도 슬롯을 놓음; `driveSourceId`가 기록과 다르면 전사 전 `DRIVE_SOURCE_MISMATCH`; 같은 로컬 오디오를 두 번 전사(두 번째는 출력 폴더가 `{stem} 2`)한 뒤 두 번째 동기화가 새 회의 폴더를 만들지 않고 같은 파일 ID를 `Updated` 또는 `Unchanged`로 돌려줌; 동기화 — 첫 실행 `Uploaded`×2, 같은 내용 재실행 `Unchanged`, 내용 변경 후 `Updated`(같은 파일 ID), 원격 변경 시 `Conflict` + 원격 파일 미갱신, `overwrite`면 `Updated`, 한 파일 실패는 `Failed`로 담고 나머지 진행, 증강 뒤 같은 전사 작업 ID로 동기화하면 회의록이 `Uploaded`, 갱신 Rejected → 연결 정보 삭제 + `DRIVE_RECONNECT_REQUIRED`, Drive 401 뒤 강제 갱신이 Rejected면 `DRIVE_RECONNECT_REQUIRED`이고 성공이면 원래 요청 재전송 없이 `DRIVE_UNAVAILABLE`, 루트 폴더가 지워졌으면 다시 만들고 저장된 ID 갱신; 연결 해제는 revoke 실패와 무관하게 비움; 동기화나 Picker 교환이 진행 중일 때 연결을 해제하면 끝난 뒤에도 저장소가 비어 있음(토큰·폴더 ID가 되살아나지 않음); gateway 갱신이 거부돼 `bearer()`가 세션을 지우면 이후 `require()`가 `AUTH_REQUIRED`.
- [ ] 7. IPC·조합
  - What: `tauri.rs` 명령 7개, `composition.rs`, `TranscriptionRequest.drive_source_id`, `capabilities/default.json` opener allow에 `https://drive.google.com/*`, 시작 시 `drive-imports` 정리.
  - Acceptance: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `bun run architecture:check` 통과(`package.json` L26).
- [ ] 8. 프런트엔드
  - What: 위 프런트엔드 설계 전부.
  - Acceptance: `drive-machine.test.ts`(모든 이벤트 전이), `access-machine.test.ts`(`signedIn`에서 `sessionExpired`를 받으면 `signedOut`), `tauri-backend.test.ts`(새 스키마의 성공·거부 사례, 토큰 필드가 있어도 상태에 남지 않음), `drive.dom.test.ts`(온보딩 표시·해제·연결 성공 후 숨김, 온보딩에서 연결 중 `#drive-onboarding-cancel-button` 표시와 취소 호출, 설정에서 연결 중 `#drive-settings-cancel-button` 표시와 `cancelDriveOperation` 1회 호출, 가져오기 버튼 활성 조건, 충돌 시 덮어쓰기 버튼, 재연결 필요 시 온보딩 재표시, Drive 명령이 `AUTH_REQUIRED`로 실패하면 `#access-sign-in-button`이 보이는 로그인 화면으로 돌아감), `controller.test.ts`(`styles.css`를 주입한 상태(L64–66 방식)에서 검사한다: 전사 요약이 없는 새 세션에서 `importFromDrive` 응답을 보류한 동안 `#cancel-button.closest("[hidden]") === null`, `#job-panel`의 `data-status`가 `"running"`, `#job-title`이 "Drive에서 녹음을 가져오고 있습니다"이며, 클릭하면 `importFromDrive`에 넘긴 jobId로 `backend.cancel`이 1회 호출됨; 가져오기가 성공하면 `#job-panel`이 hidden이고 화면 어디에도 보이는 "전사 완료" 문구가 없으며 `#audio-path`에 가져온 파일 이름이 보임(이전 전사를 마친 세션에서도 같음); `DRIVE_UNSUPPORTED_FILE`로 실패하면 `data-status === "failed"`이고 `#error-message`에 이유가 보임; 가져오기가 끝나면 busy가 풀림; `transcribe` 응답을 보류한 동안 `#drive-import-button.disabled === true`이고 `#cancel-button`은 여전히 전사 작업 ID로 `backend.cancel`을 부름; 전사·전사문 가져오기·증강 완료 후 연결 상태에서만 `syncMeetingToDrive` 호출; 증강 뒤 호출의 첫 인자가 전사 작업 ID; Drive 원본이면 요청에 `driveSourceId`; Drive에서 가져온 뒤 로컬 파일을 고르거나 마이크로 녹음해 전사하면 요청에 `driveSourceId`가 없음), `styles.test.ts` 통과. `bun run check`(tsc)가 통과해야 하며 `DriveImport`를 `TranscriptionResult` 자리에 넘기는 호출이 없다.
- [ ] 9. 문서
  - What: `README.md`/`README.en.md`(Drive 연결·가져오기·자동 업로드·충돌·연결 해제·저장 위치·권한 범위·개인정보처리방침 링크), `INSTALL.md`(첫 실행 단계에 Drive 연결은 선택), `DESIGN.md`(Drive 요소 상태), `docs/ARCHITECTURE.md`(포트 18, 명령 35, Drive 어댑터).
  - Acceptance: `grep`으로 README 두 벌에 `drive.file`과 `Galpi` 폴더 설명이 모두 있음.

## 게이트

- G1: 도메인 규칙 고정 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib domain::drive | EXPECT: 실패 0, 충돌 판정 다섯 경우 포함
- G2: 로그인 게이트 bearer | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib application::tests::gateway | EXPECT: 실패 0, bearer 다섯 경우 포함
- G3: Drive 저장소 영속성 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib settings::drive | EXPECT: 실패 0
- G4: Drive 어댑터 계약 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib adapters::outbound::drive | EXPECT: 실패 0
- G5: Drive use case | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib application::tests::drive | EXPECT: 실패 0
- G6: Rust 전체 | CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets | EXPECT: 실패 0
- G7: Rust 린트 | CHECK: cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings | EXPECT: 종료 코드 0
- G8: Rust 형식 | CHECK: cargo fmt --manifest-path src-tauri/Cargo.toml --check | EXPECT: 종료 코드 0
- G9: 프런트 정적 검사와 아키텍처 펜스 | CHECK: bun run check | EXPECT: 종료 코드 0
- G10: 프런트 테스트 | CHECK: bun test | EXPECT: 실패 0
- G11: 소스에 클라이언트 보안 비밀번호 없음 | CHECK: git grep -n GOCSPX | EXPECT: 출력 없음(종료 코드 1)
- G12: 실제 스모크(배포된 auth-gateway#10 필요) | CHECK: bun run dev | EXPECT: 증거 파일 `.issueops/evidence/task-12-drive-smoke.md`에 연결→폴더 생성, Picker 가져오기→전사→업로드, 재처리 Unchanged/Updated, Drive 쪽 수정 후 Conflict, 연결 해제 결과와 시각이 기록됨. gateway가 아직 배포되지 않았으면 그 사실과 gateway 이슈 상태를 근거로 보류를 기록한다.

## 실제 QA 시나리오

- 정상: 로그인 → 온보딩 `Drive 연결` → 브라우저 동의 → 칩에 이메일 → Drive 웹에서 `Galpi` 폴더 확인 → `Drive에서 가져오기` → Picker에서 m4a 선택 → 다운로드 진행 표시 → 전사 완료 → `#drive-sync-status`가 업로드 완료 → Drive에 회의 폴더와 srt·txt → AI 증강 → 회의록 업로드 → 같은 회의 증강 재실행 → 회의록 `Updated`, 파일 ID 동일.
- 실패: Picker를 다른 계정으로 선택 → 계정 불일치 문구, 토큰 미교체; 다운로드 중 취소 → `drive-imports`에 남은 폴더 없음; Drive 웹에서 txt 새 버전 업로드 → 재처리 시 충돌 문구와 덮어쓰기 버튼 → 덮어쓰기 후 `Updated`; 네트워크 차단 상태 동기화 → `DRIVE_UNAVAILABLE` 문구, 로컬 결과 정상; Google 계정 권한 페이지에서 Galpi 접근 해제 → 다음 동기화에서 재연결 안내와 온보딩 재표시.
- 화면 QA: Vite 개발 서버 + 가짜 `__TAURI_INTERNALS__`로 온보딩·칩·동기화 행·충돌 버튼을 열어 `elementsFromPoint`로 가려지지 않음을 확인.

## 의존성과 로컬 설정

- 워크트리에서 `bun install`. Rust 크레이트는 추가하지 않는다(reqwest·serde_json·sha2·base64·getrandom·tokio fs 이미 사용 중).
- 로컬 설정: 없음. 디버그 스모크에서 로컬 gateway를 쓸 때만 셸 환경 변수 `GALPI_AUTH_GATEWAY_URL`.

## Commit Strategy

Conventional Commit(`feat(drive): …`, `docs(readme): …`). 구현이 끝난 뒤 `atomic-commit-push`로 도메인·application, 어댑터, IPC·프런트, 문서 단위의 원자 커밋으로 나눈다. 각 커밋은 빌드 가능해야 한다.

## Success Criteria

이슈 #12의 완료 기준 전부와 G1–G11 통과. G12는 gateway 배포 여부에 따라 실행 또는 근거 있는 보류.
