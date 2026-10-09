---
name: 2026-10-09-chatgpt-responses
description: Caution record for a solved false case or recurring risk.
---

# ChatGPT Responses 전송: 제한 필드, 재전송 금지, 토큰 비노출, 맵 단계 중단

- Date: 2026-10-09
- Kind: `caution`
- Source: issueops-docs io-51d682a8c95b (issue #4)
- Summary: ChatGPT 요금제 Responses 호출은 preview 제한 필드를 보내면 안 되고, 한도·인증 오류 뒤 재전송하면 안 되며, 토큰·Authorization·인가 URL은 로그·오류·픽스처에 나오면 안 되고, 맵 단계 병렬 요청은 첫 실패 뒤 새 요청을 시작하면 안 된다.
- Context: 이슈 #4 구현 중 확인한 함정이다. SIWC preview는 max_output_tokens·temperature·reasoning·previous_response_id와 system role 입력을 받지 않고 response.completed를 받아야만 성공이다. 기존 extract_chunk_notes는 ThreadPoolExecutor 블록 종료 시 대기 중인 청크를 모두 실행해, 429 한도 초과 뒤에도 7개 청크 요청을 전부 보냈다(실측: 수정 전 테스트에서 요청 7 > 3). 또 테스트용 InMemorySecrets.write가 쓰기 카운터를 올리지 않아 '같은 값은 다시 쓰지 않는다' 단언이 0==0으로 공허했다. 새 워크트리에서는 sidecar 미스테이징 때문에 cargo test 빌드 스크립트가 'resource path binaries/uv-aarch64-apple-darwin doesn't exist'로 실패한다.
- Resolution: responses_stream.build_responses_body는 system 메시지를 instructions로 합치고 제한 필드를 보내지 않으며, response.completed 없는 스트림은 CHATGPT_STREAM_INTERRUPTED로 실패시킨다. 워커는 재시도하지 않고 안정 코드(CHATGPT_*)를 error 이벤트로 내며 호스트 process.rs가 이를 AppError로 올린다. refine.extract_chunk_notes는 공유 threading.Event로 첫 실패 뒤 새 요청을 막고(요청 수 ≤ MAP_MAX_WORKERS) 첫 실제 오류를 유지한다. ChatGptTokens의 Debug는 값을 가린다. InMemorySecrets.write는 writes를 증가시킨다. 새 워크트리에서는 cargo test 전에 bun run sidecar:stage를 실행한다.
- Evidence:
  - worker/galpi_worker/responses_stream.py
  - worker/galpi_worker/refine.py extract_chunk_notes
  - worker/tests/test_responses_stream.py -k map_failure (수정 전 2 failures: 7 not <= 3, 수정 후 2 OK)
  - src-tauri/src/adapters/outbound/secrets.rs InMemorySecrets::write
  - src-tauri/src/domain/chatgpt.rs debug_output_hides_token_values
  - src-tauri/src/adapters/outbound/process.rs is_assistant_error_code 분기
