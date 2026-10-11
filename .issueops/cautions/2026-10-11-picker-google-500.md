---
name: 2026-10-11-picker-google-500
description: Caution record for a solved false case or recurring risk.
---

# 데스크톱 Picker 동의 화면의 Google 500은 일시 오류이며 새로고침으로 이어진다

- Date: 2026-10-11
- Kind: `caution`
- Source: issueops-docs io-21fd3b1e1f06 (galpi#12)
- Summary: Drive 연결 동의 직후 Picker(trigger_onepick) 동의 URL이 accounts.google.com/signin/oauth/v3/consent에서 500을 한 번 돌려줬고, 같은 탭을 새로고침하자 drive.google.com/picker/oauth/consent로 정상 진행됐다.
- Context: galpi#12 실제 스모크(2026-10-10 17:40Z). 같은 파라미터 조합(기본, login_hint, mimetypes, 둘 다)을 다른 redirect로 다시 열었을 때는 500이 재현되지 않았다. 루프백 대기는 5분이라 새로고침해도 같은 시도로 콜백이 도착한다.
- Resolution: URL 파라미터를 바꾸거나 자동 재시도를 넣지 않는다. 스모크·QA에서 500을 보면 같은 탭을 새로고침하고, 사용자에게는 대기 문구('창이 보이지 않으면 취소 후 다시 시도')가 같은 해결을 안내한다. 재현되면 파라미터를 하나씩 빼서 별도 redirect로 원인을 좁힌다.
- Evidence:
  - .issueops/evidence/task-12-drive-smoke.md
  - src-tauri/src/adapters/outbound/drive/authorize.rs consent_url
