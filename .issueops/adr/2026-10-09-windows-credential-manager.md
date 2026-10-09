---
name: 2026-10-09-windows-credential-manager
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Windows Credential Manager에 긴 비밀을 조각으로 저장한다

- Date: 2026-10-09
- Kind: `adr`
- Source: issue #3 브랜치에 origin/main(da46360) 병합
- Summary: main(#8)의 ChatGPT 토큰 레코드는 Credential Manager 한 항목의 2,560바이트 한도(CRED_MAX_CREDENTIAL_BLOB_SIZE)를 넘는다. 그래서 한도를 넘는 비밀은 세대별 조각(`<target>#<generation>.<i>`)과, 원래 target에 두는 manifest로 나눠 저장한다.
- Context: 이슈 #3은 HF 토큰과 assistant 키를 Credential Manager 한 항목(UTF-16LE)에 저장했고, 한도를 넘으면 CREDENTIAL_WRITE_FAILED를 냈다. main 병합으로 Secret::ChatGptTokens(access/refresh/id JWT를 담은 JSON)가 같은 SecretStore로 들어오면서 Windows에서 저장이 실패하게 됐다. 이 토큰은 갱신할 때마다 다시 쓰이므로 쓰기가 중간에 끊겨도 일관성이 유지돼야 한다.
- Decision:
  - 2,560바이트 이하는 기존 단일 항목 형식을 그대로 쓴다. 이미 저장된 값과 호환된다.
  - 한도를 넘으면 새 세대(uuid-v7)의 조각을 모두 쓴 뒤 manifest를 커밋한다. manifest에는 조각 수, 세대, 바이트 길이, SHA-256을 담고, 유효한 UTF-16 값이 될 수 없는 lone surrogate로 시작한다.
  - 커밋 뒤에는 `<target>#*`를 CredEnumerateW로 열거해, 현재 manifest가 가리키지 않는 조각을 지운다.
  - 삭제할 때는 manifest를 먼저 지우고, 같은 방식으로 남은 조각을 모두 지운다.
  - 읽을 때는 모든 조각이 있고 길이와 digest가 맞아야 한다. 조각이 빠지거나 세대가 섞이거나 손상되면 CREDENTIAL_READ_FAILED를 낸다.
  - 순수 로직은 secrets/credential.rs에 두고 mac에서 테스트한다. FFI는 credential_manager.rs에 둔다.
- Consequences:
  - ChatGPT 로그인 토큰이 Windows에서도 저장된다.
  - 쓰기가 끊기면 이전 값이 그대로 남는다.
  - 쓰기 직후의 정리가 실패하면 쓰이지 않는 조각이 다음 쓰기나 삭제 때까지 남을 수 있다. 쓰기는 이미 커밋됐으므로 이 정리 실패는 오류로 올리지 않는다.
  - 계획 §3.2의 '1,280 코드 유닛 초과 시 CREDENTIAL_WRITE_FAILED' 규칙은 이 결정으로 대체된다.
  - 기각한 대안:
    - 비밀마다 DPAPI 파일: 새 저장 경로와 권한 관리가 필요하다.
    - UTF-8 인코딩만으로 한도를 피하기: JWT 레코드는 UTF-8로도 2,560바이트를 넘을 수 있다.
    - 조각을 제자리에 덮어쓰기: 쓰기가 끊기면 두 세대가 섞인 값을 오류 없이 읽을 수 있다.
    - ChatGPT 토큰만 설정 파일에 평문 저장: Windows 비밀 저장 계약을 어긴다.
