---
name: 2026-10-12-push-protection-is-user-level-hidden-from-repo-api-base64-blocked-allow-expires
description: Caution record for a solved false case or recurring risk.
---

# 공개 저장소의 사용자 단위 push 보호는 저장소 설정 API에 보이지 않고, base64도 디코딩해 막으며, 허용은 3시간만 유효하다

- Date: 2026-10-12
- Kind: `caution`
- Source: issueops-docs io-d5c9aed664a2 (galpi#16)
- Summary: 저장소 설정 API가 push 보호 `disabled`를 돌려줬는데도 push가 GH013으로 막혔다. 공개 저장소에는 사용자 단위 push 보호가 기본으로 켜져 있고 그 상태는 저장소 설정 API에 나타나지 않는다. GitHub는 base64로 인코딩한 값도 디코딩해 막았고, 사용자가 허용 링크로 통과시켜도 허용은 3시간만 유효하다.
- Context: galpi#16. Drive 데스크톱 클라이언트의 ID와 보안 비밀번호를 소스에 두는 결정을 하면서 1차 인계 자료는 "push 보호가 꺼져 있다"고 적었다. 그 근거는 `gh api repos/m16khb-org/galpi`의 `secret_scanning_push_protection: disabled`였는데, 이 값은 저장소 설정일 뿐이다. 실제 push는 평문 상수 두 개와 base64 상수 두 개 모두에서 GH013으로 막혔다.
- Resolution: 1) 저장소 API의 `disabled`를 push가 통과한다는 근거로 쓰지 않는다. 사용자 단위 보호는 공개 저장소에서 기본으로 켜져 있다. 2) 인코딩으로 우회하지 않는다. base64는 읽는 사람에게 아무것도 숨기지 못하고 코드만 복잡하게 한다. 3) 비밀로 판정되는 값은 저장소에 두지 않는다. 허용 링크로 통과시켜도 허용은 3시간만 유효하고 push하는 사람마다 필요하므로 해법이 되지 않는다.
- Outcome: 같은 이슈에서 Drive 클라이언트를 소스에 두는 결정을 접고 사용자가 자기 클라이언트를 저장하게 바꿨다([ADR](../adr/2026-10-12-drop-auth-gateway-drive-client-in-source.md)). 저장소에는 OAuth 값이 없으므로 허용 절차가 필요 없다.
- Evidence:
  - .issueops/adr/2026-10-12-drop-auth-gateway-drive-client-in-source.md
  - https://docs.github.com/en/code-security/secret-scanning/working-with-secret-scanning-and-push-protection/push-protection-for-users
  - https://docs.github.com/en/code-security/secret-scanning/working-with-secret-scanning-and-push-protection/working-with-push-protection-from-the-command-line
