# galpi#16 스모크 (2026-10-12, 실제 Google·Drive, auth-gateway 없음, 내장 클라이언트 없음)

- 범위: 내장 Google 클라이언트를 지운 최종 코드. Drive는 사용자가 저장한 클라이언트로만 연결한다.
- 앱 창: 이 컴퓨터에서 `bun run dev`(`cargo tauri dev`)로 이 브랜치 코드를 띄워 설정 시트와 Drive 연결을 직접 눌렀다. 앱 설정(`settings.json`)에는 클라이언트가 없는 상태에서 시작했다.
- 스모크용 클라이언트: 같은 Cloud 프로젝트 `galpi-511214`에 데스크톱 클라이언트 `Galpi client smoke 2 (galpi#16)`를 새로 만들어(README의 만드는 방법과 같은 절차: 클라이언트 만들기 → 데스크톱 앱) 쓰고, 끝난 뒤 콘솔에서 삭제했다. 값은 대화·문서·로그에 쓰지 않았고 임시 파일도 지웠다. Drive API와 Google Picker API는 이 프로젝트에 이미 사용 설정돼 있었다.
- Drive 하네스: 운영 어댑터(`DriveOAuthAdapter`, `DriveRestAdapter`)와 운영 use case(`DriveAccounts`)를 그대로 쓰는 임시 하네스(`cargo test --lib adapters::outbound::drive::smoke -- --ignored --nocapture`, 실행 뒤 삭제). 앱이 저장한 클라이언트를 설정 파일에서 읽어(값은 출력하지 않음) 썼다. 계정 선택, `drive.file` 동의, Picker 선택은 Aside로 조작했다. 저장소는 하네스 메모리 저장소다.
- 토큰 엔드포인트: 하네스가 운영 설정(`Tuning::production()`)의 값을 출력했다. 교환과 갱신 요청이 `https://oauth2.googleapis.com/token`으로 간다. 앱 코드에는 auth-gateway 주소와 내장 클라이언트 값이 남아 있지 않다(G8, G21).
- 정리: 스모크가 만든 Drive 폴더(`Galpi`)는 휴지통으로 옮겼다(HTTP 200). 토큰·code·보안 비밀번호 값은 기록하지 않았다. 하네스 원본 로그는 `/tmp`에 있었고 이 표에 옮겼다.

| 시각(UTC) | 단계 | 결과 | 비고 |
|---|---|---|---|
| ~06:52 (앱 창) | 로그인 화면 없음 | 통과 | 저장된 세션 없이 작업 화면(`새 회의 전사`)이 바로 열림. 계정 칩·로그아웃 없음, `준비 완료 · Qwen3-ASR-1.7B` |
| ~06:52 (앱 창) | 클라이언트 없음 안내 | 통과 | 설정 시트 `Google Drive` 탭에 `Drive를 연결하려면 먼저 Google 클라이언트를 저장해야 합니다.`가 보이고 `Drive 연결` 버튼이 없음. `Google 클라이언트` 구역에 ID·보안 비밀번호 입력칸, `저장`, `Google 클라이언트가 아직 없습니다.` |
| ~06:53 (앱 창) | 클라이언트 저장 | 통과 | 두 칸에 입력해 `저장`: 입력칸이 사라지고 `저장됨`과 `클라이언트 삭제`, `만드는 방법`이 보임. 같은 자리에 `Drive 연결` 버튼이 나타남. 설정 파일에 ID와 비밀번호가 저장됨 |
| ~06:53 (앱 창) | Drive 연결 | 통과 | 앱의 `Drive 연결` → 시스템 브라우저의 동의 URL이 저장한 클라이언트 ID로 열림 → 동의 → 앱이 `m16khb@gmail.com 계정의 Drive에 연결했습니다` 표시(설정 파일에 계정·세션 저장) |
| ~06:54 (앱 창) | 연결 중 변경 거부 | 통과 | 연결된 동안 `클라이언트 삭제`가 비활성이고 `Drive 연결을 해제한 뒤에 바꿀 수 있습니다.` 안내가 보임. 앱의 `연결 해제`는 클라이언트를 지우지 않음(설정 파일에 ID·비밀번호 유지) |
| 06:55:01 → 06:55:17 | Drive 연결 (하네스) | 통과 | 저장한 클라이언트로 동의 → `oauth2.googleapis.com/token` 직접 교환 → `Connected` |
| 06:55:23, 06:55:27 | 결과 업로드 | 통과 | sync `Srt=Uploaded`, `SpeakerText=Uploaded`, 녹음 업로드 `Uploaded` |
| 06:55:29 | 토큰 갱신 | 통과 | 저장 토큰 만료 후 sync에서 새 access token 발급, refresh token 유지, `Unchanged` (Google 직접) |
| 06:55:29 → 06:56:00 | Picker 가져오기 | 통과 | 두 번째 동의(`trigger_onepick`, `login_hint`) → `galpi-smoke-16.wav` 46바이트 |
| 06:56:01 | 연결 해제와 폐기 확인 | 통과 | `Disconnected`, 이전 refresh token 갱신은 `Rejected`. Galpi 폴더 휴지통 이동 HTTP 200 |
| ~06:57 (앱 창) | 클라이언트 삭제 | 통과 | 앱을 다시 실행해도 `Google 클라이언트를 쓰는 중입니다 · <클라이언트 ID>`와 `클라이언트 삭제`가 유지됨 → `클라이언트 삭제` → 입력칸과 `저장`이 다시 보이고 `Google 클라이언트가 아직 없습니다.`, 설정 파일에서 ID·비밀번호가 모두 사라짐(`driveClientSecretStored: false`), `Drive 연결` 버튼은 다시 숨음 |

## 관찰과 남은 확인

- 설정 시트는 합성 마우스·키보드 입력으로 조작했다. 앞 창이 `galpi`인지 확인한 뒤에만 입력했고, 앞 창이 다른 앱(Slack, 터미널)이면 입력하지 않고 다시 앞으로 가져왔다.
- 새 클라이언트의 동의 URL은 Chrome 주소창이 아니라 열린 직후 원본 인가 URL을 가로채 Aside에서 열었다(주소창의 URL은 로그인으로 리디렉션된 형태라 Google이 400을 돌려준다). 앱의 URL 생성 문제가 아니다.
- 내장 클라이언트로 만든 옛 연결이 시작 때 해제되는 경로는 이 컴퓨터에 그런 연결이 없어 앱 창으로 보지 못했다. use case 테스트(`drive_client_required_old_connection_is_dropped_at_status`, `drive_client_required_renewal_without_client_reconnects`)가 고정한다.
- `연결 중 변경 거부`는 연결된 상태를 앱 창으로 확인했다. 동의를 기다리는 동안의 거부와 Picker 대기 중 클라이언트 변경은 테스트(`drive_client_locked_*`)가 고정한다.
- 설정 시트 `만드는 방법` 요약과 좁은 폭 레이아웃은 화면에서 읽지 않았다(DOM 테스트와 G22가 문구를 확인한다).
- 프로덕션으로 게시한 프로젝트의 클라이언트에서 동의 화면에 추가 확인이 요구되는지는 확인하지 못했다(스모크 프로젝트는 테스트 게시 상태).
- Windows 화면은 확인하지 못했다.
