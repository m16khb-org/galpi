# 요청자 의도 계약

- lifecycle: io-51d682a8c95b
- issue: https://github.com/m16khb-org/galpi/issues/4
- intent_class: standard

## 원문 요청
windows도 지원해야하고 codex sign in도 지원해야 하고 당근 디자인 시스템도 적용해야해 로 각각 이슈부터 만들자 / (추가 지시) 잠깐 구글 로그인은 놔두고 앱 내에서 gpt모델을 사용하려면 chatgpt로 로그인을 해서 이용할 수 있게하는 chatgpt의 최신공개된 기능을 사용하라는건데

## 해석
OpenAI가 2026-09-29 공개한 Sign in with ChatGPT(SIWC)의 오픈소스·로컬 앱용 ChatGPT plan usage 흐름을 갈피에 넣는다. 설정 시트에 'ChatGPT로 계속하기' 버튼을 추가해 시스템 브라우저에서 OAuth(PKCE, loopback 127.0.0.1 콜백, dynamic_agent_client 최초 등록, agent_name_hint=Galpi, 영속 ext_agent_host_id)를 수행하고, 발급된 client_id·access/refresh/id token을 기존 keychain 어댑터에 저장한다. 회의록 정제 워커는 인증 모드가 chatgpt일 때 https://api.openai.com/v1/responses(store:false, stream:true, instructions 사용, system role 금지, max_output_tokens 금지)로 요청하고, 모델 목록은 GET /v1/models(visibility=list)에서 받아 선택 UI에 보여 준다. 기존 API 키 + base URL 모드는 그대로 유지하며 두 모드를 전환할 수 있다. 토큰 갱신(access 1h, refresh 30d rotating)은 Rust 호스트가 담당한다.

## 성공 기준
- 1) 설정 시트에서 'ChatGPT로 계속하기'를 누르면 시스템 브라우저가 auth.openai.com으로 열리고, 승인 후 앱이 '<email>로 로그인됨'을 표시한다. 2) 앱 재시작 후에도 로그인 상태가 유지되고 토큰 값은 IPC로 넘어가지 않는다. 3) 로그인 상태에서 모델 선택 목록이 계정의 /v1/models 결과로 채워진다. 4) 회의록 정제가 ChatGPT 모드에서 Responses API 스트림으로 완료되어 _회의록.md를 만든다(response.completed 수신). 5) access token 만료 뒤 정제를 실행하면 자동 refresh 후 성공한다. 6) 로그아웃하면 keychain 항목이 지워지고 API 키 모드로 돌아간다. 7) subscription_sharing_usage_limit_exceeded 등 구조화 오류가 한국어 상태 문구로 표시된다. 8) API 키 모드의 기존 테스트가 그대로 통과한다.

## 비목표
- (없음)

## 제약
- (없음)

## 모호함
- (없음)

## 읽는 규칙
이 문서는 요청자 의도 계약이다. 원격 이슈 본문은 구현 계약이다. 두 문서가 충돌하면 구현을 시작하지 말고 충돌한 줄을 인용해 blocker로 보고한다.
