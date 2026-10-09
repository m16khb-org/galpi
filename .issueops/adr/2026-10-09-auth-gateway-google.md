---
name: 2026-10-09-auth-gateway-google
description: Accepted decision record with rationale, alternatives, and consequences.
---

# 앱 사용에 auth-gateway Google 로그인을 요구하고 호스트가 기능 명령을 막는다

- Date: 2026-10-09
- Kind: `adr`
- Source: issueops-docs io-21fd3b1e1f06 (issue #10); src-tauri/src/application/gateway.rs, src-tauri/src/adapters/outbound/gateway/mod.rs, src-tauri/src/adapters/outbound/browser_sign_in/; cargo test --all-targets gateway (24 passed)
- Summary: Galpi는 auth-gateway의 데스크톱 로그인(RFC 8252: 시스템 브라우저, 루프백 리다이렉트, PKCE)으로 Google 로그인을 마쳐야 쓸 수 있다. Rust Application이 기능 명령 5개(prepare·refine_transcript·transcribe·import_transcript·start_recording)의 첫 줄에서 AUTH_REQUIRED로 막고, 프런트는 inert 셸 앞의 로그인 화면만 보여 준다.
- Context: galpi#10 사용자 결정: auth-gateway에 데스크톱 로그인 흐름을 추가(m16khb-org/auth-gateway#8)하고, 로그인한 모든 Google 계정을 허용하며, 저장된 세션이 있으면 오프라인에서도 쓴다. 기존 ChatGPT 로그인 어댑터가 브라우저·루프백·PKCE를 이미 갖고 있었다.
- Decision: 1) application/gateway.rs AppAccessGate가 메모리 접근 상태를 소유하고, 시작 때 refresh 1회로 판정한다: 성공은 회전 토큰 저장 후 열기, gateway의 400 invalid_grant만 세션 삭제 후 닫기, 그 밖의 실패(연결·5xx·429·형식 오류)는 저장 세션으로 오프라인 열기. 2) 로그아웃은 메모리 상태를 먼저 닫고 POST /auth/native/logout(이 기기 세션만)을 시도한 뒤 결과와 관계없이 로컬 세션을 지운다. 3) 세션은 새 Secret::GatewaySession(Windows Credential Manager com.m16khb.galpi:gateway-session, macOS 0600 settings.json)에, 이메일은 settings.json gatewayEmail에 둔다. 4) ChatGPT와 gateway 로그인이 함께 쓰는 브라우저·루프백·PKCE는 adapters/outbound/browser_sign_in/로 옮기고 오류 코드는 SignInCodes로 호출자가 넘긴다. 5) gateway 주소는 https://auth.m16khb.dev이며 디버그 빌드에서만 GALPI_AUTH_GATEWAY_URL로 바꾼다.
- Consequences: 업데이트 뒤 첫 실행은 로그인 전까지 기능을 쓸 수 없고, 저장 세션이 없는 오프라인 첫 실행은 쓸 수 없다. 새 기능 명령을 추가하면 Application 메서드 첫 줄에 self.access.require()?를 넣어야 한다. 진행 중 작업을 끝내는 명령(stop/cancel)과 설정·진단은 막지 않는다. macOS에서는 refresh token이 서명 전까지 0600 평문 파일에 남는다.
