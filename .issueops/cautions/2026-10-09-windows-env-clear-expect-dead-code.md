---
name: 2026-10-09-windows-env-clear-expect-dead-code
description: Caution record for a solved false case or recurring risk.
---

# Windows 포팅 함정: env_clear 환경, \\?\ 경로, 조건부 expect(dead_code)

- Date: 2026-10-09
- Kind: `caution`
- Source: issue #3 구현 중 Windows 타깃 clippy에서 SettingsFile dead_code로 실제 실패
- Summary: Windows 타깃에서만 드러나는 실패 세 가지와 예방법.
- Context: 워커는 env_clear 후 명시 환경으로 뜬다. Windows canonicalize는 \\?\ 확장 경로를 돌려준다. 이 크레이트는 -D warnings와 #[expect(dead_code)]를 쓴다.
- Resolution: (1) Windows 워커 환경은 platform::worker_environment 한 곳에서 SYSTEMROOT·WINDIR·TEMP·USERPROFILE 등을 호스트에서 복사한다. SYSTEMROOT가 없으면 Python이 기동하지 못한다. CI 사본(scripts/ci/windows-install-smoke.ps1)과 함께 고친다. (2) 경로 정경화는 paths::canonical(dunce::simplified)만 쓰고 포함 검사는 양쪽을 같은 헬퍼로 만든다. (3) 한 플랫폼에서만 쓰이는 항목은 cfg_attr(all(windows, not(test)), expect(dead_code, …))처럼 조건을 정확히 건다. 무조건 expect는 다른 플랫폼에서 unfulfilled_lint_expectations로 -D warnings 실패가 된다. mac에서 Windows lint를 미리 보려면 RC_x86_64_pc_windows_msvc에 가짜 llvm-rc를 지정해 cargo clippy --target x86_64-pc-windows-msvc를 돌린다(링크는 안 함, 게이트 아님).
