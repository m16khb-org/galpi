# 검증 보고: auth-gateway 의존 제거와 사용자 Google 클라이언트 (galpi#16)

- lifecycle: `io-d5c9aed664a2`, generation 4, branch `16-remove-auth-gateway`, base `fed2f856e56ac0e545ea3082a60f8bb71f3529df`
- 이슈: https://github.com/m16khb-org/galpi/issues/16
- 구현 세션: omp(`anthropic/claude-sonnet-5-5`). Rust와 게이트·스모크는 이 세션이, 프런트엔드와 문서는 하위 에이전트가 맡았고 결과를 직접 검증했다.
- 범위 변화: 처음 계획은 내장 클라이언트를 소스에 두는 것이었다. 개정 계획이 사용자 지정 클라이언트를 더했고, 마지막에 사용자가 **내장 클라이언트를 제거하라고 직접 지시**했다(이 지시는 개정 계획 문서에 없다). 최종 변경은 소스에 Google OAuth 값이 하나도 없고, Drive는 사용자가 저장한 클라이언트로만 연결된다. 이 세션의 push는 GitHub push 보호(GH013)에 두 번 막혔는데, 값이 소스에서 사라져 허용 절차가 필요 없어졌다.
- 위험도: 로그인 흐름, Drive 토큰 경로, 비밀 저장소 항목, 연결된 기존 사용자의 동작을 바꾸므로 전체 모드.

## 성공 기준과 증거

| 기준 | 증거 | 결과 |
|---|---|---|
| 세션 없이 열면 로그인 화면 없이 작업 화면이 열리고, 기능과 Drive 명령이 로그인 없이 동작한다. `AUTH_REQUIRED`·`GATEWAY_*`·gateway 주소가 코드에 없다 | G8, `controller.test.ts`, 실제 앱 창(스모크 `로그인 화면 없음`) | 통과 |
| Drive 교환·갱신은 Google에 직접 보내고 `invalid_grant`만 연결을 지운다 | G1, G2 | 통과 |
| 소스에 클라이언트 ID·보안 비밀번호·내장 상수·base64 흔적이 없다 | G21(추적·미추적 파일 전체 값 검색, 이슈 12 계획 문서의 옛 ID는 범위 밖이라 제외) | 통과 |
| 클라이언트를 저장하지 않으면 연결과 Picker가 Google을 부르기 전에 `DRIVE_CLIENT_REQUIRED`로 멈추고, 내장 클라이언트로 만든 옛 연결은 시작 때 해제·폐기되며 클라이언트 없이는 갱신하지 않는다 | G23: `drive_client_required_*` 4건 | 통과 |
| 저장한 클라이언트가 동의, 교환, 갱신에 쓰인다 | G14: `drive_client_used_*` 6건 | 통과 |
| 형식 검증과 trim | G12: `drive_client_invalid_*` | 통과 |
| 연결 중·동의 대기 중 저장·삭제를 거부하고 Picker 대기 중 클라이언트가 바뀌면 옛 토큰이 붙지 않는다 | G13: `drive_client_locked_*` 5건 | 통과 |
| 보안 비밀번호가 IPC·`Debug`에 나타나지 않는다 | G15 | 통과 |
| 저장은 재시작 뒤에도 남고, 연결 해제는 지우지 않으며, 덮어쓰기 중간 상태가 저장 안 됨으로 읽힌다 | G16, G17 | 통과 |
| 프런트 동작(클라이언트가 없으면 연결·가져오기 대신 설정 진입, 연결 중 잠금, 저장됨, 입력칸 숨김, 비밀번호 칸 속성, 오류 `aria`, 포커스 이동, 옛 연결 처리) | G18: `Drive client` 테스트 21건 이상 | 통과 |
| 명령 39개·포트 16개가 코드와 문서에서 일치 | G19 | 통과 |
| 문서·ADR·caution과 만드는 방법 안내 | G10, G11, G20(README 두 벌), G22(설정 시트 안내) | 통과 |
| 실제 Google 스모크 | G9: `.issueops/evidence/task-16-smoke.md`(클라이언트 없음 → 저장 → 연결 → 업로드·갱신·Picker → 해제 → 삭제) | 통과 |
| `bun run check`, `bun test`, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --all-targets` | G3~G7: Rust 346건 통과·ignored 1건, `bun test` 282건 | 통과 |

## 변경 규모

- 삭제 11파일(Rust 6: gateway 어댑터·세션 저장소·use case·도메인·테스트, 프런트 5).
- IPC 명령 40→39(gateway 4개 삭제, `load_drive_client`·`save_drive_client`·`clear_drive_client` 3개 추가), 포트 trait 18→16(`GatewayAuthPort`, `GatewaySessionStore` 삭제, `DriveSessionStore`에 클라이언트 읽기·저장·삭제), `DriveAuthPort::exchange`·`refresh`의 bearer 인자 삭제, `DriveAuthorize`·`DriveCallback`의 `client`는 필수 값, `refresh`는 `&DriveClient`, `DriveTokenFailure::AuthRequired` 삭제.
- 새 값 객체 `DriveClient`(검증·trim, `Debug`가 비밀번호를 가림, `Serialize` 없음), `DriveClientStatus {configured, clientId}`. 새 `Secret::DriveClientSecret`(계정 이름 `drive-client-secret`), 설정 파일 키 `driveClientId`·`driveClientSecret`·`driveClientSecretStored`.
- 지운 것: 내장 클라이언트 상수와 `credentials()` 대체 경로. 이슈 단계에서 추가한 Cloud 콘솔 `Galpi Desktop`의 두 번째 비밀번호는 코드와 무관해졌다.

## 게이트 원장 변경(계획 대비)

원장을 최종 범위의 G1~G23으로 다시 만들었다. 계획의 문구에서 벗어난 부분은 다음과 같다.

- G3: `-- --quiet`를 붙였다. 테스트가 늘어 출력이 길어지면 `test result` 줄이 잘려 EXPECT가 맞지 않았다.
- G15, G17, G21: CHECK 인자에 `secret`이 들어가면 command policy가 `secret_like_argument`로 거부해, 단어를 실행 중에 조립하도록 바꿨다(`.issueops/cautions/2026-10-11-check.md`). G15는 같은 이유로 테스트 이름 접두어가 `drive_client_hidden_`이다.
- G17, G18: 하네스가 CHECK의 백슬래시를 지워서 정규식의 `\s`, `\n`을 `[ ]*`, 문자열 슬라이스로 바꿨다.
- G9, G14, G15, G16, G18, G20, G21: 내장 클라이언트 제거에 맞춰 기대값을 바꿨다(스모크 필수 단계, `custom`→`configured`, 기본값 테스트 삭제, 평문 상수 확인 → 값이 없는지 확인). 새 게이트 G23을 더했다.
- 이미 충족된 게이트는 `gates check`가 다시 실행하지 않으므로, 범위가 바뀔 때마다 원장을 새로 만들어 모두 다시 실행했다.

## 부작용 목록

- **Drive를 쓰려면 사용자가 자기 Google 클라이언트를 만들어 저장해야 한다.** Galpi는 내장 클라이언트를 더 이상 싣지 않는다. 안내는 README 두 벌과 설정 시트 `만드는 방법`에 있다.
- **내장 클라이언트로 연결해 둔 기존 사용자의 Drive는 다음 시작 때 해제된다.** 그 연결의 refresh token은 사라진 클라이언트에서만 갱신되므로 이어 쓸 수 없다. 해제 때 Google에 토큰 폐기를 시도하고, 사용자는 클라이언트를 저장한 뒤 다시 연결해야 한다(README에 적음). 이미 Drive에 올린 파일은 지워지지 않는다.
- 설정 파일에 `driveClientId`, `driveClientSecret`(macOS 평문, 0600, 기존 Drive 세션과 같은 수준), `driveClientSecretStored`가 생긴다. 옛 파일은 그대로 읽히고(`#[serde(default)]`), 이전 버전 앱은 모르는 키를 무시한다. Windows Credential Manager에는 `com.m16khb.galpi:drive-client-secret`이 생긴다.
- 롤백: 이 변경 이후 연결한 상태에서 이전 버전으로 돌아가면, 이전 버전은 내장 클라이언트로 갱신을 시도해 `DRIVE_UNAVAILABLE`로 실패한다. 연결을 해제하고 다시 연결하면 된다.
- Drive 동기화 연속성은 Cloud 프로젝트별로 갈린다. 다른 프로젝트의 클라이언트로 바꾸면 Drive에 새 `Galpi` 폴더가 생기고 이전 파일과 이어지지 않는다(README, 설정 시트 안내, ADR에 적음). 스모크에서도 새 폴더가 생겼다.
- 클라이언트 삭제와 저장은 Drive가 연결돼 있거나 연결 동의를 기다리는 동안 `DRIVE_CLIENT_LOCKED`로 거부된다. 연결 해제는 클라이언트를 지우지 않는다.
- 설정 파일의 `gatewayEmail`, `gatewaySession`, `gatewaySessionStored` 키는 다음 설정 쓰기에서 사라진다. Windows Credential Manager의 `gateway-session` 항목은 코드가 지우지 않고 남는다(README에 안내).
- 공개 오류 코드 `AUTH_REQUIRED`, `GATEWAY_*` 6종이 사라지고 `DRIVE_CLIENT_INVALID`, `DRIVE_CLIENT_LOCKED`, `DRIVE_CLIENT_REQUIRED`가 생긴다. 소비처는 같은 저장소의 프런트뿐이다.
- 공개 저장소에 OAuth 값이 들어가지 않는다. 테스트·문서·로그·증거 파일에도 값이 없다. 스모크용 사용자 클라이언트는 콘솔에서 삭제했고 값 파일도 지웠다. Cloud 콘솔의 `Galpi Desktop` 클라이언트는 이제 Galpi가 쓰지 않으며, 지울지는 소유자가 정한다.
- 시작 때 gateway refresh 1회, Drive 토큰 요청 앞의 gateway refresh 1회가 사라진다. Drive 연결 상태를 읽을 때 클라이언트 저장 여부 읽기 1회가 더해진다.

## UI 판단

- 기존 디자인 시스템: 로그인 화면과 계정 칩·로그아웃을 지웠고, 설정 시트 `Google Drive` 절에 `Google 클라이언트` 하위 절을 더했다. 기존 `settings-section`, `settings-input`, SEED 버튼 레시피, `--seed-*` 토큰만 쓰고 새 색·크기 값이 없다.
- 클라이언트가 없으면 `Drive 연결` 대신 설정 진입(온보딩 카드는 `Google 클라이언트 설정`)을 보이고, 가져오기 버튼은 숨기며, 그 이유를 글로 적는다(색만 쓰지 않음).
- 접근성: 입력칸마다 보이는 `<label>`, 비밀번호 칸은 `type="password"`·`autocomplete="off"`·`spellcheck="false"`. 형식 오류는 틀린 칸에 `aria-invalid="true"`와 오류 줄 `aria-describedby`, 상태 줄은 `role="status"`. 잠금·저장됨·오류 상태는 색과 글자를 함께 쓴다. 삭제 성공 뒤 포커스는 비워진 클라이언트 ID 칸, 저장 성공 뒤는 `클라이언트 삭제`로 옮긴다(`FOCUS_HANDOFF`).
- 반응형·모션: 추가한 애니메이션이 없고, 레이아웃은 기존 설정 시트 폭을 그대로 따른다. 좁은 폭 화면은 직접 열어 보지 않았다.
- 실제 화면 확인: 개발 앱에서 클라이언트가 없을 때의 안내, 입력과 `저장됨`, `Drive 연결` 버튼 등장, 연결된 동안의 잠금 안내와 비활성 버튼, 재시작 뒤 저장한 클라이언트 유지, 삭제 뒤 입력칸 복귀를 화면 캡처로 확인했다(스모크 증거 표 참고).
- 화면 QA(`aside-web-qa`): Not Run. TARGET으로 쓸 로컬 URL이 계획과 이슈에 없고, 이 앱은 Tauri 창이라 평범한 브라우저에서는 IPC가 없어 검증할 수 없다. 대신 위 앱 창 확인으로 대체했다.

## 확인하지 못한 것

- 내장 클라이언트로 만든 옛 Drive 연결이 시작 때 해제되는 경로를 앱 창으로 보는 것: 이 컴퓨터에 그런 연결이 없다. use case 테스트가 고정한다.
- 프로덕션으로 게시한 프로젝트의 클라이언트에서 동의 화면에 추가 확인이 요구되는지: 스모크는 테스트 상태(`galpi-511214`) 프로젝트로 했다. README의 `프로덕션 게시` 안내는 `drive.file`이 민감하지 않은 범위라는 문서 근거에 의존한다.
- 설정 시트 `만드는 방법` 요약과 좁은 폭 레이아웃은 화면에서 읽지 않았다(DOM 테스트와 G22가 문구를 확인한다).
- Windows 실제 화면. 개발 컴퓨터는 macOS뿐이다.

## 구현 중 계획에 없던 변경

- `Secret` enum의 `#[expect(clippy::enum_variant_names, …)]`.
- `status()`가 클라이언트 없는 옛 연결을 해제하는 동작과 갱신 경로의 같은 처리(내장 클라이언트 제거에 따른 필수 결과).
