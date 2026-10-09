---
name: 2026-10-09-windows-untrusted-junction-uv-python
description: Caution record for a solved false case or recurring risk.
---

# Windows가 일반 사용자 junction을 막으면 uv python install이 실패한다 (os error 448)

- Date: 2026-10-09
- Kind: `caution`
- Source: Windows 11 26H2(빌드 26300) 실기 엔진 준비 실패, `uv` 0.12.5 `crates/uv-fs/src/lib.rs` `create_junction`
- Summary: 이 OS는 일반 권한으로 만든 junction을 따라가지 못하게 막는다(`STATUS_UNTRUSTED_MOUNT_POINT`, os error 448). `uv python install`은 인터프리터 설치를 마친 뒤 minor 버전 junction(`cpython-3.12-windows-x86_64-none`)을 만들고 `metadata()`로 따라가 확인하는데, 이 확인이 막혀 명령 전체가 "Failed to create Python minor version link directory"로 끝난다. `--python 3.12`로 만든 venv도 이 junction을 거쳐 실패한다.
- Context: PowerShell, WMI로 띄운 프로세스, C 드라이브 어디에서 만든 junction이든 똑같이 막혔다. OneDrive는 실행 중이 아니었고 레지스트리·프로세스 완화 정책에도 관련 설정이 없었다. uv에는 이 링크를 끄는 옵션이나 환경 변수가 없고, upstream 이슈 astral-sh/uv#19616도 열려 있다.
- Resolution: `setup.rs`는 Python을 `PYTHON_VERSION`(3.12.14, 고정된 uv가 설치하는 패치)으로 설치하고 venv도 같은 패치로 만들어 junction을 거치지 않는다. Windows에서 `uv python install`이 `PROCESS_FAILED`로 끝나도 `cpython-3.12.14-windows-x86_64-none\python.exe`가 있으면 계속 진행하고, 인터프리터는 다음 `uv venv` 단계가 다시 검증한다. 취소(`CANCELLED`)와 인터프리터가 없는 실패는 그대로 오류다. uv 사이드카를 올리면 그 uv가 설치하는 3.12 패치로 `PYTHON_VERSION`과 `scripts/ci/windows-install-smoke.ps1`을 함께 바꾼다.
