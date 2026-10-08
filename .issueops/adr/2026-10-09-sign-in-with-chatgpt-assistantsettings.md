---
name: 2026-10-09-sign-in-with-chatgpt-assistantsettings
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Sign in with ChatGPT: 호스트가 토큰을 갱신하고 계정 상태를 AssistantSettings와 분리한다

- Date: 2026-10-09
- Kind: `adr`
- Source: issueops-docs io-51d682a8c95b (issue #4)
- Summary: ChatGPT 로그인(SIWC 오픈소스·로컬 앱 흐름)은 Rust 호스트가 OAuth·토큰 갱신·저장을 소유하고, 워커에는 짧은 수명의 access token과 GALPI_ASSISTANT_TRANSPORT=responses만 넘기며, 계정 상태는 AssistantSettings가 아닌 별도 ChatGptSettings/ChatGptPreferences와 별도 IPC 커맨드로 둔다.
- Context: 이슈 #4는 API 키 대신 ChatGPT 요금제로 회의록을 정제하길 요구했다. refresh token은 갱신마다 회전하고 같은 세션의 갱신은 직렬화돼야 한다. 설정 시트는 AssistantSettings 문서 전체를 autosave로 되돌려 보내며, fdd8caf가 고친 '창이 오래된 호스트 값을 되돌려 지우는' 사고가 있었다. 현재 SecretStore는 e283101 이후 Keychain이 아니라 0600 settings.json이다.
- Decision: 1) 갱신은 호스트(application/chatgpt.rs ChatGptAccounts)가 정제 직전·모델 목록 조회 직전에 tokio Mutex로 직렬화해 수행하고 회전된 토큰을 즉시 저장한다. 만료 20분 전부터 갱신한다. 2) 워커에는 access token만 GALPI_ASSISTANT_API_KEY로, 전송 방식은 GALPI_ASSISTANT_TRANSPORT=responses로 넘기고 워커는 재시도하지 않는다. 3) authMode·model·welcomeAcknowledged(창 소유)와 계정 상태(호스트 소유)를 ChatGptPreferences/ChatGptSettings로 분리하고 AssistantSettings는 바꾸지 않는다. 4) ext_agent_host_id는 urn:uuid:<v4>로 settings.json에 영속하고 로그아웃 뒤에도 유지한다. 5) 토큰은 기존 SecretStore(Secret::ChatGptTokens)를 재사용하며 Keychain 전환은 Developer ID 서명 뒤로 미룬다. 6) 로그아웃은 이슈 완료 기준 6을 따라 서버 폐기를 시도한 뒤 결과와 무관하게 토큰과 발급 client_id를 지운다.
- Consequences: refresh token이 API 키와 같은 0600 평문 파일 수준으로 보호된다(README에 명시). 서명 후 LocalSettingsStore의 비밀 저장소 교체 한 줄로 Keychain으로 옮기면 갱신 때만 Keychain 쓰기가 생긴다. 로그아웃 정책을 공식 권고로 바꾸려면 clear_session이 registration을 남기도록 한 곳만 고치면 된다. 정제 중 20분 마진을 넘겨 만료되면 CHATGPT_AUTH_REJECTED로 끝나고 자동 재실행하지 않는다.
- Evidence:
  - src-tauri/src/application/chatgpt.rs, src-tauri/src/application/chatgpt/sign_in.rs
  - src-tauri/src/adapters/outbound/settings/chatgpt.rs
  - src-tauri/src/adapters/outbound/environment.rs assistant_environment(.., transport)
  - src-tauri/src/domain/chatgpt.rs ACCESS_TOKEN_REFRESH_MARGIN_SECONDS, AgentHostId
  - cargo test --lib application::tests::chatgpt (26 passed), settings::chatgpt (9 passed)
  - issue https://github.com/m16khb-org/galpi/issues/4, plan .issueops/issues/4/plan.md
- Alternatives / rejected options:
  - 워커가 refresh: 회전 토큰 경쟁, 30일 비밀이 호스트 밖으로 나감, 단명 프로세스에 저장소 없음
  - AssistantSettings에 authMode·계정 필드 추가: autosave 되돌림 사고 재현 위험, 기존 테스트 리터럴 전부 파손
  - ext_agent_host_id로 JWK thumbprint: 비밀키 저장이 필요한데 OpenAI가 소유를 검증하지 않아 이득 없음
  - reqwest 기본 rustls+aws-lc-rs: C 툴체인 빌드 의존, native-tls(Security.framework) 채택
  - 로그아웃 때 client_id 유지(공식 profiles-and-sessions 권고): 이슈 완료 기준 6과 충돌해 기각, 재로그인마다 새 동적 등록이 생길 수 있음
