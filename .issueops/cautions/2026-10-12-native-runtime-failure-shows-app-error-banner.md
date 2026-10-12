---
name: 2026-10-12-native-runtime-failure-shows-app-error-banner
description: Caution record for a solved false case or recurring risk.
---

# 네이티브 런타임 연결 실패는 다시 앱 오류 배너로 표시한다

- Date: 2026-10-12
- Kind: `caution`
- Source: issueops-docs io-d5c9aed664a2 (galpi#16); src/ui/controller.ts, src/ui/controller.test.ts
- Supersedes: [2026-10-09-qa-ipc](2026-10-09-qa-ipc.md)
- Summary: 로그인 화면이 없어졌으므로 controller.start()는 이벤트 구독이 실패하면 `view.showError("네이티브 런타임에 연결할 수 없습니다. 앱을 다시 실행해 주세요.")`로 앱 오류 배너(#app-error)에 알리고 반환한다. 구독에 성공하면 로그인 확인 없이 바로 `loadWorkspace()`를 부른다.
- Context: 2026-10-09 caution은 galpi#10에서 셸이 로그인 전에 inert였기 때문에 배너가 보이지 않아 로그인 화면(#access-status)에 재실행 안내를 띄운다고 적었다. galpi#16이 앱 접근 게이트와 inert 셸을 지워 그 전제가 사라졌다.
- Resolution: 시작 순서는 `bind()` → 이벤트 구독 → `loadWorkspace()`를 지킨다(2026-08-23 caution). 런타임 실패 경로는 controller.test.ts가 오류 배너 문구로 고정하고, 정상 시작은 로그인 없이 `diagnose`가 불리는 것으로 고정한다. 평범한 브라우저에서는 IPC가 없다는 2026-08-23 caution이 그대로 적용된다.
