---
name: 2026-10-09-new-stored-credential-needs-four-match-arms-and-is-empty
description: Caution record for a solved false case or recurring risk.
---

# 새 Secret 값을 추가하면 LocalSettingsStore의 네 match 분기와 is_empty를 함께 고친다

- Date: 2026-10-09
- Kind: `caution`
- Source: issueops-docs io-21fd3b1e1f06 (issue #10); src-tauri/src/adapters/outbound/settings.rs, src-tauri/src/adapters/outbound/settings/gateway.rs
- Summary: macOS SettingsFile 백엔드는 read가 항상 없음, write가 no-op이고 실제 값은 LocalSettingsStore가 LocalSettings 필드에 직접 쓴다. 새 Secret 변형은 secret()·store_secret()·note_secret_present()·secret_stored()의 match 분기와 LocalSettings 필드, is_empty()를 모두 갱신해야 재시작 뒤에도 남는다.
- Context: galpi#10 계획 1차 리뷰가 찾았다: is_empty()는 필드를 손으로 나열하므로 새 필드를 빠뜨리면 다른 설정이 없는 새 설치에서 로그인 직후 store_settings가 settings.json을 지워 세션이 사라진다. match는 컴파일러가 잡지만 is_empty는 잡지 못한다.
- Resolution: Secret::GatewaySession 추가 때 네 분기와 gateway_email·gateway_session·gateway_session_stored를 is_empty에 넣었고, settings/gateway.rs 테스트가 빈 설정에서 SettingsFile 백엔드로 저장한 세션이 새 store에서 다시 읽히는지와 clear 뒤 파일이 지워지는지를 고정한다.
