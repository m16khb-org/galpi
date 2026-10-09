# 요청자 의도 계약

- lifecycle: io-21fd3b1e1f06
- issue: https://github.com/m16khb-org/galpi/issues/10
- intent_class: standard

## 원문 요청
galpi를 사용하려면 auth-gateway를 통해 google 로그인을 해야되도록 하려고해 로 진행하자 / (연동 방식 질문 답) 첫번째방법이 일반적으로 사용되는 방법이면 그대로 해줘 / (사용 허용 범위 답) 로그인한 모든 Google 계정 / (오프라인 동작 답) 저장된 세션이 있으면 오프라인에서도 사용

## 해석
Galpi(macOS·Windows 데스크톱)를 쓰려면 auth-gateway를 거친 Google 로그인이 필요하도록 한다. 로그인은 RFC 8252 방식(시스템 브라우저, 루프백 리다이렉트, PKCE)으로 하고, auth-gateway에 새로 추가될 데스크톱 로그인 흐름(일회용 코드 교환, 쿠키 없는 refresh·logout)을 사용한다. 로그인한 모든 Google 계정을 허용하며 역할·도메인 검사는 하지 않는다. refresh token은 OS 비밀 저장소에 보관하고, gateway에 접속할 수 없을 때는 저장된 세션으로 계속 사용하며, gateway가 세션을 명시적으로 거부할 때만 다시 로그인을 요구한다. 로그아웃은 gateway 폐기 요청 후 로컬 세션을 지운다. 기존 ChatGPT 로그인 구현(시스템 브라우저, 루프백, PKCE, 비밀 저장소)을 재사용한다. auth-gateway 쪽 변경은 m16khb-org/auth-gateway의 별도 사이클이 선행 조건이다.

## 성공 기준
- 1) 저장된 Galpi 세션이 없으면 앱은 로그인 화면만 보여 주고, 녹음·전사·가져오기·AI 증강·엔진 준비 Tauri 명령은 AUTH_REQUIRED 오류로 거부한다(Rust application 테스트). 2) Google로 로그인 → 시스템 브라우저 → gateway → 루프백 콜백 → 코드 교환 → 토큰을 OS 비밀 저장소에 저장 → 기능 화면 진입(어댑터 테스트 + 실제 gateway 로그인 smoke). 3) 시작 시 refresh: 성공하면 회전된 토큰 저장, gateway가 invalid_grant로 거부하면 세션 삭제 후 로그인 화면, 네트워크 오류·5xx면 저장된 세션으로 계속 사용(포트 테스트로 세 분기 고정). 4) 로그아웃은 gateway 폐기를 시도하고 로컬 세션을 지운 뒤 로그인 화면으로 돌아간다. 5) 로그인 진행 중 취소와 5분 타임아웃을 지원한다. 6) bun run check, bun test, cargo fmt/clippy/test, 워커 게이트 통과.

## 비목표
- (없음)

## 제약
- (없음)

## 모호함
- (없음)

## 읽는 규칙
이 문서는 요청자 의도 계약이다. 원격 이슈 본문은 구현 계약이다. 두 문서가 충돌하면 구현을 시작하지 말고 충돌한 줄을 인용해 blocker로 보고한다.
