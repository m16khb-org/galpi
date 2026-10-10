---
name: 2026-10-11-google-drive-auth-gateway-appproperties
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Google Drive 연동은 auth-gateway가 토큰을 교환하고 appProperties로 무상태 동기화한다

- Date: 2026-10-11
- Kind: `adr`
- Source: issueops-docs io-21fd3b1e1f06 (galpi#12)
- Summary: Galpi는 drive.file 범위의 데스크톱 OAuth client ID만 갖고, 코드 교환과 갱신은 auth-gateway의 POST /drive/native/token이 Galpi 세션 bearer로 대행한다. 회의 폴더와 결과 파일은 Drive appProperties(galpiRoot, galpiMeeting, galpiKind, galpiSha256)와 sha256Checksum으로 다시 찾고 충돌을 판정하므로 로컬에 Drive 파일 기록을 두지 않는다.
- Context: galpi#12. 데스크톱 OAuth 클라이언트는 보안 비밀번호 없이 교환하면 400 client_secret is missing을 받는다. 바이너리를 배포하지 않고 소스 빌드로 배포하는 저장소라 비밀번호를 소스에 둘 수 없다(사용자 결정). 같은 오디오를 다시 전사하면 출력 폴더가 `{stem} 2`로 바뀌어 출력 폴더 기반 기록은 끊긴다.
- Decision: 1) 클라이언트 보안 비밀번호는 auth-gateway 서버 설정에만 두고, 앱은 AppAccessGate::bearer()로 얻은 gateway access token을 bearer로 붙여 /drive/native/token에 교환·갱신을 맡긴다. bearer는 브라우저 동의가 끝난 뒤 얻는다. 2) 400 invalid_grant만 Drive 연결을 지우고(DRIVE_RECONNECT_REQUIRED), gateway 401은 호스트 세션도 닫는다(AppAccessGate::expire). 그 밖의 실패는 DRIVE_UNAVAILABLE로 끝내고 재시도하지 않는다. 3) 회의 키는 Drive에서 가져온 녹음이면 `drive:<fileId>`, 아니면 정경화한 원본 경로의 FNV-1a 해시(`local:<16hex>`)다. domain은 sha2를 쓰지 않는다. 4) 충돌 판정은 순수 함수 decide(remote, local_sha256, overwrite): Drive sha256Checksum == galpiSha256이면 아무도 손대지 않은 파일이다. 5) 업로드는 전사 명령과 분리된 sync_meeting_to_drive IPC로 하고 한 파일의 실패는 그 파일 상태로만 보고한다.
- Consequences: auth-gateway#10의 /drive/native/token이 배포되지 않으면 Drive 연결이 DRIVE_UNAVAILABLE로 실패할 뿐 다른 기능은 영향이 없다. Drive에서 고른 파일 외의 사용자 파일에는 접근하지 않는다. sha256Checksum이 없는 파일은 충돌로 취급되므로, 실제 스모크에서 업로드 직후 checksum이 있음을 확인했다(.issueops/evidence/task-12-drive-smoke.md). 포트가 3개(DriveAuthPort, DriveFilesPort, DriveSessionStore) 늘어 18개, IPC 명령이 7개 늘어 35개다.
- Evidence:
  - src-tauri/src/domain/drive.rs (decide, MeetingKey, stable_hash)
  - src-tauri/src/application/drive.rs (DriveAccounts::consent, token_failure)
  - src-tauri/src/application/gateway.rs (bearer, expire)
  - src-tauri/src/adapters/outbound/drive/tokens.rs
  - .issueops/evidence/task-12-drive-smoke.md
- Alternatives / rejected options:
  - 클라이언트 보안 비밀번호를 소스 상수로 포함: 사용자가 gateway 대행으로 재결정해 기각.
  - 빌드 때 비밀번호 주입, 주입 안 된 빌드는 Drive 비활성: 소스 빌드 사용자가 기능을 쓸 수 없어 기각.
  - 로컬 사이드카 파일에 Drive 파일 ID·리비전 저장: 재전사 때 새 출력 폴더가 생겨 기록이 끊겨 기각.
  - drive.readonly로 Galpi 폴더 전체 동기화: 제한 범위 심사가 필요해 기각.
  - 앱 안 웹 Picker(iframe): CSP frame-src 'none'과 WebKit 서드파티 쿠키 차단으로 기각, 시스템 브라우저 데스크톱 Picker 채택.
  - 전사 명령 안에서 업로드: 업로드 실패가 전사 결과를 막아 기각.
