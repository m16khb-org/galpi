# 요청자 의도 계약

- lifecycle: io-21fd3b1e1f06
- issue: https://github.com/m16khb-org/galpi/issues/12
- intent_class: standard

## 원문 요청
google 로그인을 필수로하고잇는데 google drive에 녹음본을 올리고 거기있는 녹음본을 기반으로 전사 -> 화자분리 -> ai 증강을 하게 할수있어? / 구글드라이드 사용하기위해 온보딩절차를 만들고 그절차에 사용자 구글드라이브에 갈피용 폴더를 만들고 거기를 사용하게 하는거어때? / 갈피 앱과 웹에서는 google drive에 파일을 올리고 받고 수정할 수 있어야해 / 갈피웹은 아직 고려하지마 추후 만들 예정이야 / (수정 대상 답) 전사 결과 갱신 / (결과 업로드 답) 자동 업로드 / (Drive 앱으로 올린 녹음 처리 답) 이건 더 조사를 해봐 → 조사 결과 데스크톱 Google Picker로 가져오기 제안 / Cloud 프로젝트와 OAuth 클라이언트 ID 이거는 갈피전용을 쓰는 거야?? aside mcp를 활용해서 네가 만들어봐 / (이슈 생성 제안: 온보딩, 갈피 폴더, Picker로 가져오기, 결과 자동 업로드와 갱신, 충돌 감지) 로 이슈만들고 진행해줘 base는 main 브랜치로 / 앱 게시는 네가 확인해서 게시해줘 테스트유저말고 모두 쓸수있게해야해 / (클라이언트 보안 비밀번호 질문) 시크릿을상수로넣어도돼?? / 아니다 소스에 넣는거보다 auth gateway에서 받는게 좋겠어

## 해석
Galpi 데스크톱 앱에 Google Drive 연동을 추가한다. 로그인(auth-gateway) 뒤 건너뛸 수 있는 Drive 연결 온보딩에서 Galpi 전용 Cloud 프로젝트(galpi-511214)의 데스크톱 OAuth 클라이언트로 drive.file 동의를 받아 auth-gateway 대행으로 코드를 교환하고, refresh token을 OS 비밀 저장소에 두고, 내 Drive에 Galpi 폴더를 만든다(있으면 재사용, 지워졌으면 다시 만든다). 'Drive에서 가져오기'는 데스크톱 Google Picker(trigger_onepick)를 시스템 브라우저로 열어 녹음 파일을 고르게 하고, 연결 계정과 같은 계정인지 확인한 뒤 파일을 기기로 내려받아 기존 전사·화자 분리·AI 증강 흐름에 넣는다. Drive가 연결되어 있으면 전사·AI 증강 결과 파일을 Galpi 폴더에 자동으로 올리고, 같은 회의를 다시 처리하면 새 파일을 만들지 않고 기존 Drive 파일을 갱신한다. 갱신 직전에 Drive 파일이 마지막 동기화 이후 바뀌었으면 덮어쓰지 않고 사용자에게 알린다. Galpi 웹은 범위 밖이다. OAuth 앱은 테스트 사용자 제한 없이 모두 쓸 수 있도록 프로덕션으로 게시한다(콘솔 설정과 m16khb.dev 정책 페이지, 코드 밖 작업).

## 성공 기준
- 로그인하지 않은 상태에서 새 Drive Tauri 명령은 AUTH_REQUIRED로 거부된다(Rust application 테스트).
- Drive 연결: 시스템 브라우저 drive.file 동의 → 루프백·PKCE 코드 교환 → refresh token을 OS 비밀 저장소에 저장 → Galpi 폴더 생성 또는 재사용 → 연결 계정 이메일 표시(어댑터 가짜 서버 테스트 + 실제 Google 스모크).
- Drive 연결 해제: 토큰 폐기를 시도하고 결과와 관계없이 로컬 토큰과 폴더 기록을 지운다(application 테스트).
- Drive에서 가져오기: Picker 콜백의 picked_file_ids와 code를 처리하고, 다른 계정으로 고르면 거부하며, 파일을 임시 경로로 스트리밍 다운로드한 뒤 기존 전사 흐름이 그 로컬 경로로 실행된다. 취소하면 부분 파일이 남지 않는다(테스트 + 실제 스모크).
- Drive가 연결되어 있으면 전사 완료와 AI 증강 완료 때 결과 파일이 Galpi 폴더 아래에 올라가고, 같은 회의를 다시 처리하면 같은 Drive 파일 ID가 갱신된다(가짜 서버 테스트 + 실제 스모크).
- 갱신 직전 Drive 파일 버전이 마지막 동기화 값과 다르면 덮어쓰지 않고 충돌을 사용자에게 알린다(application 테스트).
- Drive 업로드 실패는 로컬 결과 생성을 실패로 만들지 않고 별도 상태로 표시된다.
- 앱 소스에는 클라이언트 보안 비밀번호가 없고, 코드 교환과 토큰 갱신은 auth-gateway의 POST /drive/native/token(gateway 세션 bearer)으로만 한다. invalid_grant일 때만 Drive 연결을 끊고, gateway 미도달은 Drive 작업 실패로 표시한다(가짜 서버 테스트).
- bun run check, bun test, cargo fmt/clippy/test 게이트가 통과한다.
- OAuth 동의 화면 게시 상태가 프로덕션이고, 테스트 사용자가 아닌 계정도 동의할 수 있다(콘솔 확인).

## 비목표
- Galpi 웹(추후 별도 작업).
- drive.readonly 등 제한 범위 사용, Drive 폴더 전체 자동 동기화.
- 로컬 녹음 원본 오디오의 자동 업로드.
- Drive에서 고른 원본 파일의 이동·이름 변경·삭제.
- 앱 안의 전사 본문 편집기.

## 제약
- Google Cloud 프로젝트 galpi-511214, 데스크톱 OAuth 클라이언트 Galpi Desktop, 범위는 drive.file 하나.
- 데스크톱 Picker 요청에는 drive.file만 넣을 수 있고 prompt=consent, trigger_onepick=true가 필수다.
- 클라이언트 보안 비밀번호는 auth-gateway 서버 설정에만 둔다(선행 이슈 m16khb-org/auth-gateway#10). 앱은 client ID만 상수로 갖는다.
- 새 기능 명령은 Application 메서드 첫 줄에 self.access.require()?를 둔다(ADR 2026-10-09-auth-gateway-google).
- 전사 입력은 로컬 경로(TranscriptionRequest.input_path)로 유지하고 워커·프로토콜은 바꾸지 않는다.
- Drive refresh token은 기존 비밀 저장소 경로(Windows Credential Manager, macOS 0600 설정 파일)를 쓴다.

## 모호함
- resolved: 수정 대상은 전사 결과 갱신(사용자 답).
- resolved: 결과는 자동 업로드(사용자 답).
- resolved: Drive 앱 등 외부로 올린 녹음은 데스크톱 Google Picker로 가져온다(공식 문서 조사 + 실제 Picker 스모크로 picked_file_ids 확인).
- resolved: Cloud 프로젝트는 Galpi 전용 galpi-511214(사용자 답, 생성 완료).
- resolved: OAuth 앱은 프로덕션 게시(사용자 지시), 정책 페이지는 m16khb.dev portfolio-router(사용자 선택).
- deferred: 충돌 시 처리 방식(충돌 사본 업로드 또는 사용자 선택)과 Drive 안 회의별 폴더 이름 규칙은 계획 단계에서 정한다.
- deferred: 온보딩 refresh token이 이후 별도 Picker 흐름에서 고른 파일에도 접근하는지 구현 중 실측한다.
- resolved: 클라이언트 보안 비밀번호는 auth-gateway가 교환·갱신을 대행(사용자 결정, auth-gateway#10).

## 읽는 규칙
이 문서는 요청자 의도 계약이다. 원격 이슈 본문은 구현 계약이다. 두 문서가 충돌하면 구현을 시작하지 말고 충돌한 줄을 인용해 blocker로 보고한다.
