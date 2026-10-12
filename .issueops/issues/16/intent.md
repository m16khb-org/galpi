# 요청자 의도 계약

- lifecycle: io-d5c9aed664a2
- issue: https://github.com/m16khb-org/galpi/issues/16
- intent_class: standard

## 원문 요청
구글드라이브 로그인 기능은 필요해보이는데 서비스 auth-gateway 로그인이 필요할 이유가 있어? / (선택지 A: 로그인을 Drive 연결 시점으로 미룸, B: gateway 로그인 제거하고 gateway 대행 엔드포인트의 bearer 검사 제거, C: 비밀번호를 소스에 포함해 gateway 의존 제거 중) C로 로 진행 / (보안 비밀번호를 어떻게 받을지 질문 답) 갈피로그인되어있으니까 응답을 확인해서 넣어

## 해석
Galpi에서 auth-gateway 의존을 완전히 없앤다. (1) 앱 전체 Google 로그인 게이트(AppAccessGate, AUTH_REQUIRED, 로그인 화면, gateway 로그인·로그아웃 명령, gateway 세션 저장)를 제거해 앱을 열면 바로 녹음·전사·가져오기·AI 증강·엔진 준비를 쓸 수 있게 한다. (2) Google Drive의 코드 교환과 토큰 갱신을 auth-gateway POST /drive/native/token 대행 대신 앱이 Google 토큰 엔드포인트(https://oauth2.googleapis.com/token)에 직접 한다. 이를 위해 galpi-511214 프로젝트 Galpi Desktop OAuth 클라이언트에 새로 추가한 보안 비밀번호(끝 4자리 YPUH, 브라우저에 로그인된 Cloud 콘솔에서 발급)를 소스 상수로 넣는다. PKCE와 drive.file 범위, appProperties 동기화, Picker 흐름은 그대로 둔다. 범위는 이슈·계획·구현·검증·커밋·push·draft PR·execution complete이며 merge는 범위 밖이다.

## 성공 기준
- 1) 저장된 세션 없이 앱을 열면 로그인 화면 없이 기능 화면이 바로 열리고, 녹음·전사·가져오기·AI 증강·엔진 준비·Drive 명령이 로그인 없이 동작한다. 코드에 AUTH_REQUIRED와 GATEWAY_ 접두어 오류 코드, auth-gateway 주소가 남지 않는다(grep).
- 2) Drive 연결은 브라우저 동의, 루프백 콜백, Google 토큰 엔드포인트 직접 교환(client id, 앱 상수 비밀번호, code, code verifier, redirect uri), 토큰 저장 순서로 끝나고 갱신도 Google에 직접 한다. 400 invalid_grant만 Drive 연결을 지우고(DRIVE_RECONNECT_REQUIRED), 그 밖의 실패는 DRIVE_UNAVAILABLE로 끝낸다(가짜 서버 테스트).
- 3) 실제 Google로 Drive 연결, Picker 가져오기, 결과 업로드를 직접 교환 경로로 확인한다(스모크 증거).
- 4) gateway를 전제로 쓴 문서(README 두 벌, INSTALL, DESIGN, AGENTS, docs/ARCHITECTURE, .issueops 개요)를 고치고, 2026-10-09와 2026-10-11 ADR을 대체하는 ADR을 남긴다.
- 5) bun run check, bun test, cargo fmt --check, cargo clippy -D warnings, cargo test --all-targets가 통과한다.

## 비목표
- (없음)

## 제약
- (없음)

## 모호함
- (없음)

## 읽는 규칙
이 문서는 요청자 의도 계약이다. 원격 이슈 본문은 구현 계약이다. 두 문서가 충돌하면 구현을 시작하지 말고 충돌한 줄을 인용해 blocker로 보고한다.
