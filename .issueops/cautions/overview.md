---
name: overview
description: Family module overview: known risks and incident lessons.
---

# Cautions — Overview

Canonical index: [CAUTIONS.md](../CAUTIONS.md)

- Generated docs are drafts; directly verify weak evidence.
- Do not commit secrets, credentials, local state, or generated artifacts.
- [작업 시작 시 setBusy(kind)를 renderJob보다 먼저 불러야 진행 카드 제목이 새 작업을 따른다](2026-10-11-setbusy-kind-renderjob.md)
- [게이트 CHECK 인자에 비밀처럼 보이는 문자열을 넣으면 명령 정책이 실행을 거부한다](2026-10-11-check.md)
- [데스크톱 Picker 동의 화면의 Google 500은 일시 오류이며 새로고침으로 이어진다](2026-10-11-picker-google-500.md)
- [Drive 파일 이름을 Path로 해석하면 Windows에서 결과가 달라진다](2026-10-11-drive-path-windows.md)
- [MPS 모델에 길이가 제각각인 입력을 넣으면 empty_cache로 풀리지 않는 메모리가 쌓인다](2026-10-11-mps-empty-cache.md)
- [네이티브 런타임 연결 실패는 다시 앱 오류 배너로 표시한다](2026-10-12-native-runtime-failure-shows-app-error-banner.md)
- [공개 저장소의 사용자 단위 push 보호는 저장소 설정 API에 보이지 않고, base64도 디코딩해 막으며, 허용은 3시간만 유효하다(그래서 저장소에 OAuth 값을 두지 않는다)](2026-10-12-push-protection-is-user-level-hidden-from-repo-api-base64-blocked-allow-expires.md)
