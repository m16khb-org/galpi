---
name: 2026-10-11-drive-path-windows
description: Caution record for a solved false case or recurring risk.
---

# Drive 파일 이름을 Path로 해석하면 Windows에서 결과가 달라진다

- Date: 2026-10-11
- Kind: `caution`
- Source: issueops io-21fd3b1e1f06 generation 3 (CI run 38081588654)
- Summary: Drive 파일 이름 같은 외부 문자열을 std::path::Path로 나누면 OS마다 결과가 다르다. Windows는 a:b?.WAV의 a:를 드라이브 접두어로 보고 버려 b.WAV를 내고, macOS는 \를 일반 문자로 보아 C:\x\y.m4a를 C--x-y.m4a로 만든다.
- Context: issue #12 PR #13 CI의 rust (windows-latest)에서 drive::rest::tests::downloaded_names_are_sanitized_and_keep_the_extension이 실패했다. macOS 로컬과 macOS CI는 통과해 머지 미리보기의 checks_failing에서야 드러났다.
- Resolution: local_name(drive/rest.rs)은 Path 없이 문자열을 / 와 \ 로 나눠 빈 조각과 . 을 건너뛴 마지막 조각을 쓰고(..이면 meeting), 확장자는 Path::file_stem과 같은 규칙으로 직접 나눈다. 이름 정리는 paths::sanitized_stem을 재사용한다. 실제 로컬 파일 경로가 아닌 문자열(원격 이름, 사용자 입력)에는 Path를 쓰지 않고, 테스트에 C:\x\y.m4a, ..\..\boot.ini, C:, c:.wav 같은 Windows 구문을 넣어 macOS에서도 RED가 나게 한다.
