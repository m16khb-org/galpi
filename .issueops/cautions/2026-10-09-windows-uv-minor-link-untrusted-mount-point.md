---
name: 2026-10-09-windows-uv-minor-link-untrusted-mount-point
description: Caution record for a solved false case or recurring risk.
---

# Windows: uv 최소 버전 junction이 신뢰할 수 없는 탑재 지점(os error 448)이 되면 엔진 venv가 깨진다

- Date: 2026-10-09
- Kind: `caution`
- Source: Windows 11 26H2(빌드 26300) 실기에서 로컬 엔진 준비가 `uv trampoline failed to spawn Python child process / entity not found (os error 2)`로 실패
- Summary: uv는 관리형 Python을 설치한 뒤 `python\cpython-3.12-windows-x86_64-none` junction을 만들고, `--python 3.12`로 만든 venv의 `pyvenv.cfg` `home`도 이 junction을 가리킨다. 표준 사용자가 만든 junction을 Windows가 신뢰할 수 없는 탑재 지점으로 취급하면 모든 프로세스가 그 경로를 통과하지 못한다(os error 448). 그 결과 venv의 trampoline `python.exe`는 os error 2로 실패하고, `uv python install`·`uv venv --python 3.12`·`uv python find 3.12`도 448로 실패한다. 같은 사용자가 `%TEMP%`에 새로 만든 junction도 통과되지 않았으므로 Galpi 앱 데이터 경로만의 문제가 아니다.
- Resolution: (1) Windows는 `Os::python_request()`로 정확한 패치(`3.12.14`)를 요청한다. 패치 디렉터리는 일반 폴더라서 venv `home`이 junction을 거치지 않는다. (2) uv는 설치를 모두 끝낸 뒤 마지막 단계에서 junction을 만들기 때문에, `setup::install_python`은 `uv python install`의 실패를 취소가 아닌 한 로그로만 남기고 계속한다. 바로 다음 `uv venv --python 3.12.14`가 실제 실패(다운로드 실패 포함)를 보고한다. (3) Windows 준비 마커에 `+py<요청>`을 넣어 junction 위에 만든 기존 venv를 한 번 다시 만들게 한다. `scripts/ci/windows-install-smoke.ps1`도 같은 요청을 쓴다.
- Avoid: Windows에서 `--python 3.12` 같은 minor 요청이나 `cpython-3.12-…` 경로에 의존하지 않는다. uv 사이드카를 올릴 때는 `3.12.14`를 그 uv가 내려받을 수 있는지 확인한다.
- Evidence: `src-tauri/src/adapters/outbound/platform.rs` (`Os::python_request`), `src-tauri/src/adapters/outbound/setup.rs` (`install_python`), `src-tauri/src/adapters/outbound/environment.rs` (`whisperx_marker`). 실기 재현: venv `python.exe` 실행 시 trampoline os error 2, `Get-ChildItem` junction 시 "untrusted mount point", `uv venv --python 3.12.14`는 성공하고 `home`이 패치 디렉터리를 가리킴.
