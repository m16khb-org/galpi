---
name: 2026-10-12-drop-auth-gateway-drive-client-in-source
description: Accepted decision record with rationale, alternatives, and consequences.
---

# auth-gateway 의존을 없애고 Drive는 사용자가 저장한 Google 클라이언트로만 연결한다

- Date: 2026-10-12
- Kind: `adr`
- Source: issueops-docs io-d5c9aed664a2 (galpi#16); src-tauri/src/adapters/outbound/drive/mod.rs, src-tauri/src/adapters/outbound/drive/tokens.rs, src-tauri/src/adapters/outbound/settings/drive.rs, src-tauri/src/application/drive.rs
- Summary: Galpi는 더 이상 auth-gateway에 요청을 보내지 않는다. 앱 접근 게이트(로그인 화면, `AUTH_REQUIRED`, gateway 명령 4개, gateway 세션 저장)를 지우고, Drive 코드 교환과 갱신은 앱이 Google 토큰 엔드포인트(`https://oauth2.googleapis.com/token`)에 직접 보낸다. 소스와 저장소에는 OAuth 클라이언트 ID도 보안 비밀번호도 두지 않는다. 사용자가 자기 Google Cloud 프로젝트의 데스크톱 클라이언트를 설정 시트에서 저장해야만 Drive에 연결할 수 있고, 저장하지 않으면 `DRIVE_CLIENT_REQUIRED`다.
- Supersedes: [2026-10-09-auth-gateway-google](2026-10-09-auth-gateway-google.md)(앱 사용에 gateway 로그인을 요구함)와 [2026-10-11-google-drive-auth-gateway-appproperties](2026-10-11-google-drive-auth-gateway-appproperties.md)의 Decision 1·2(gateway가 토큰 교환을 대행하고 비밀번호를 서버에만 둠, "소스 상수 포함" 기각)를 모두 대체한다. 같은 ADR의 appProperties 무상태 동기화와 충돌 판정은 그대로 유효하다.
- Context: galpi#16. 로그인을 요구하는 기능 명령(녹음·전사·전사문 가져오기·AI 증강·엔진 준비)은 로컬에서만 동작하고 gateway에 아무것도 요청하지 않는다. gateway는 로그인한 모든 Google 계정을 허용했으므로 접근을 제한하는 효과가 없었다. Google은 설치형 앱의 클라이언트 보안 비밀번호를 기밀로 보지 않으므로(https://developers.google.com/identity/protocols/oauth2/native-app) 앱이 직접 교환할 수 있고, 인가 코드는 PKCE가 지킨다. 이 저장소는 바이너리를 배포하지 않고 소스 빌드로 배포한다. 처음에는 Drive 클라이언트의 ID와 보안 비밀번호를 소스 상수로 두려 했다. 그러나 GitHub push 보호(GH013)가 평문 상수도 base64 상수도 막았다(GitHub는 base64 값을 디코딩해 비밀로 판정한다). 그래서 이런 값을 저장소에 두면 push하는 사람마다 허용 절차가 필요하고, 허용은 3시간만 유효하다(경위는 [2026-10-12-push-protection-is-user-level-hidden-from-repo-api-base64-blocked-allow-expires](../cautions/2026-10-12-push-protection-is-user-level-hidden-from-repo-api-base64-blocked-allow-expires.md)). 또 모든 사용자가 쓰는 공개 클라이언트 하나는 Google이 비밀번호를 자동으로 폐기할 위험을 가늠할 수 없고, 모든 사용자의 Drive 접근을 한 프로젝트에 묶는다.
- Decision: 1) 앱 접근 게이트를 지운다. 창은 로그인 화면 없이 작업 화면으로 열리고, 런타임 연결 실패는 앱 오류 배너(`view.showError`)로 알린다. 2) Drive 교환·갱신은 Google 토큰 엔드포인트에 form으로 직접 보낸다. `DriveAuthPort`는 bearer 인자를 받지 않고, 동의 URL·교환·갱신은 호출하는 쪽이 넘긴 `DriveClient`로 만든다. 3) 소스와 저장소에는 어떤 OAuth 클라이언트 값도 두지 않는다. 사용자는 자기 Cloud 프로젝트의 데스크톱 클라이언트 ID와 보안 비밀번호를 설정 시트에 저장하고, 저장하지 않은 상태에서 `connect_drive`와 `import_from_drive`는 `DRIVE_CLIENT_REQUIRED`("Google 클라이언트를 먼저 저장해 주세요.")를 돌려준다. 테스트·문서·로그·이슈·PR 본문에는 가짜 값만 쓴다. 4) 유지하는 결정: Drive appProperties 무상태 동기화, `drive.file` 범위, PKCE, 400 `invalid_grant`만 Drive 연결 해제(`DRIVE_RECONNECT_REQUIRED`), 실패한 업로드나 갱신을 자동으로 다시 보내지 않음. 401 등 그 밖의 실패는 `DRIVE_UNAVAILABLE`이다. 5) 이전 빌드에 들어 있던 클라이언트로 맺은 Drive 연결은 갱신할 수 없으므로 앱이 시작할 때 끊는다. 사용자는 자기 클라이언트를 저장하고 다시 연결해야 한다.
- Alternatives rejected:
  - 로그인을 Drive 연결 시점으로 미루는 안(A): gateway 대행 교환과 Google 동의가 두 번 남아 사용자가 기각.
  - gateway 대행 엔드포인트의 bearer 검사만 제거하는 안(B): auth-gateway 저장소를 바꿔야 하고 서버 의존이 남아 사용자가 기각.
  - 평문 상수로 소스에 두는 안: GH013이 막아 push마다 사용자별 허용이 필요하고, 허용은 3시간 뒤 만료된다. 공개 비밀번호의 자동 폐기 위험과 한 프로젝트 의존도 남는다.
  - base64 상수로 두는 안: GitHub가 base64 값도 디코딩해 막으므로 우회가 되지 않고, 읽는 사람에게 아무것도 숨기지 못한 채 코드만 복잡해진다.
  - gateway를 유지해 서버에 비밀번호를 두는 안: 모든 로그인 계정을 허용해 접근 제한 효과가 없었고, 서버 의존과 로그인 화면이 남는다.
  - 빌드 때 CI 비밀로 클라이언트를 주입하는 안: 만들어진 바이너리에 값이 그대로 들어가므로 비밀이 사라지지 않고, 소스 빌드 사용자는 그 값을 받을 수 없다. 주입되지 않은 빌드에서 Drive를 끄는 안은 2026-10-11 ADR에서 이미 기각했다.
  - 시작 때 옛 gateway 세션을 지우는 정리 코드: 모든 기기에서 한 번만 필요한 코드가 영구히 남고, 그 토큰은 이제 앱이 부르지 않는 서버에만 쓸 수 있어 기각. 대신 README가 지워도 되는 항목으로 안내한다.
- Consequences: Drive는 Cloud 콘솔에서 클라이언트를 한 번 만들어 저장해야 쓸 수 있다(README `Google Drive 사용 준비`). 이전 빌드의 클라이언트로 맺은 Drive 연결은 시작할 때 끊기며, 같은 클라이언트가 없으므로 다시 연결하려면 사용자가 자기 클라이언트를 저장해야 한다. 저장소에는 OAuth 값이 없으므로 push 보호 허용 절차가 필요 없다. 동기화의 연속성은 Cloud 프로젝트마다 따로 성립한다(아래 프로젝트별 동기화 단절). 이전 버전이 남긴 Credential Manager의 `gateway-session` 항목과 설정 파일의 `gateway*` 키는 더 쓰지 않으며, 설정 키는 다음 설정 쓰기 때 사라진다. 이전 버전 앱이 새 설정 파일을 읽으면 gateway 세션이 없어 로그인 화면을 보인다(롤백 시 재로그인). 포트는 18개에서 16개로 줄고, IPC 명령은 40개에서 gateway 명령 4개를 지워 36개가 된 뒤 Google 클라이언트 명령 3개를 더해 39개가 된다. 시작 경로의 gateway 갱신 요청 한 번과 Drive 토큰 요청 앞의 gateway 갱신 왕복이 사라진다. 사용자가 클라이언트를 저장해 연결한 채 클라이언트 설정이 없는 이전 버전으로 롤백하면 이전 버전이 그 클라이언트를 몰라 갱신에 실패(`DRIVE_UNAVAILABLE`)하므로, 사용자는 연결을 해제하고 다시 연결해야 한다(README에 안내).
- Client handling: 사용자는 설정 시트의 `Google 클라이언트`에서 자기 데스크톱 클라이언트 ID와 보안 비밀번호를 저장한다(IPC `load_drive_client`·`save_drive_client`·`clear_drive_client`). 세 명령은 모두 `DriveClientStatus {configured, clientId}`를 돌려주며, `clear_drive_client`는 저장한 클라이언트를 지운다(대체할 클라이언트는 없다). 저장하면 동의 화면(연결과 Picker), 코드 교환, 토큰 갱신이 모두 그 클라이언트로 간다. 토큰 폐기는 토큰만 보내므로 클라이언트와 무관하다.
  - 잠금: Drive가 연결된 동안과 연결 동의를 기다리는 동안에는 저장도 삭제도 할 수 없다(`DRIVE_CLIENT_LOCKED`). refresh token은 발급한 클라이언트에서만 갱신되기 때문이다. 저장·삭제는 연결과 같은 연결 슬롯을 잡아 서로 겹치지 않는다. Picker 동의를 기다리는 동안 연결 해제, 클라이언트 저장, 재연결이 일어나면 옛 클라이언트가 발급한 토큰은 붙이지 않고 폐기한다. 연결 해제는 저장한 클라이언트를 지우지 않는다.
  - 형식: 클라이언트 ID는 `<숫자>-<영문 소문자·숫자>.apps.googleusercontent.com`, 보안 비밀번호는 공백 없는 256자 이하이며 접두어는 요구하지 않는다(오래된 클라이언트의 비밀번호에는 접두어가 없다). 어긋나면 `DRIVE_CLIENT_INVALID`이고 메시지는 어느 칸이 왜 틀렸는지 말한다.
  - 저장 위치: 클라이언트 ID는 `settings.json`의 `driveClientId`, 보안 비밀번호는 비밀 저장소(Windows Credential Manager `com.m16khb.galpi:drive-client-secret`, macOS는 설정 파일)에 둔다. 둘 다 있고 형식이 맞을 때만 저장된 것으로 본다. 저장은 ID 지우기 → 비밀번호 → ID, 삭제는 ID → 비밀번호 순서로 써서, 중간에 멈춰도 ID가 없는 상태가 되어 클라이언트 없음으로 읽힌다.
  - 비밀번호는 창으로 돌려보내지 않는다. 응답은 `{ configured, clientId }`뿐이고, 저장에 성공하면 입력칸을 비운다. `DriveClient`의 `Debug`는 비밀번호를 가리고 `Serialize`를 구현하지 않는다.
  - 프로젝트별 동기화 단절: 루트 폴더와 결과 파일은 `appProperties`로 찾는데, `appProperties`는 요청한 앱(프로젝트)에만 보이고 `drive.file`도 그 앱이 만들거나 연 파일에만 접근하게 한다. 그래서 다른 프로젝트의 클라이언트로 바꾸면 Drive에 새 `Galpi` 폴더가 생기고, 회의 폴더와 녹음은 다시 올라가며, 충돌과 최신 여부 판정은 이전 파일과 이어지지 않는다. 이전 클라이언트로 올린 파일은 보이지 않는다(Drive에서 지워지지는 않는다). 2026-10-11 ADR의 appProperties 무상태 동기화는 클라이언트(프로젝트)마다 성립한다. README와 설정 시트 안내에 이 사실을 적었다.
- Client handling alternatives rejected:
  - 연결 중에도 클라이언트 변경을 허용하는 안: refresh token은 발급한 클라이언트에서만 갱신되므로 바꾼 뒤에는 갱신이 실패해 연결이 끊긴다. 잠금으로 처음부터 막는다.
  - 저장한 보안 비밀번호를 창에 다시 보여 주는 안: 창에 비밀번호가 남을 이유가 없고 기존 비밀 입력칸 관례(바꾸려면 지운다)와도 어긋난다. 저장 뒤 두 입력칸을 비우고 `저장됨`만 보인다.
  - 클라이언트 ID까지 비밀 저장소에 두는 안: ID는 비밀이 아니고, 화면에 보여 줘야 하며, ID 없음을 "저장 안 됨"의 기준으로 삼아 저장 중단 상태를 안전하게 다루는 데 쓴다. 그래서 ID는 `settings.json`에 둔다.
- Evidence:
  - src-tauri/src/adapters/outbound/drive/mod.rs (`GOOGLE_TOKEN_URL`, 호출자가 넘기는 `DriveClient`)
  - src-tauri/src/adapters/outbound/drive/tokens.rs
  - src-tauri/src/application/ports.rs (`DriveAuthPort`)
  - src-tauri/src/application/drive.rs (`DRIVE_CLIENT_REQUIRED`)
  - https://developers.google.com/identity/protocols/oauth2/native-app
  - .issueops/issues/16/plan.md
  - src-tauri/src/adapters/outbound/settings/drive.rs (`load_client`, `save_client`, `clear_client`)
  - https://docs.github.com/en/code-security/secret-scanning/working-with-secret-scanning-and-push-protection/push-protection-for-users
  - https://developers.google.com/workspace/drive/api/reference/rest/v3/files
