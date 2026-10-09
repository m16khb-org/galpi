---
name: 2026-10-09-linux-x86-64-os-cpu-cuda
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Linux x86_64를 세 번째 지원 플랫폼으로 추가

- Date: 2026-10-09
- Kind: `adr`
- Context: macOS(Apple Silicon)와 Windows x64에 이어 Linux x86_64(glibc, Ubuntu 24.04 기준) 사용자도 같은 앱을 쓰게 한다. Windows 이식이 정한 규칙([2026-10-09-windows-x64-os-cpu-cuda](2026-10-09-windows-x64-os-cpu-cuda.md))을 그대로 따른다.
- Decision:
  1. `Os::Linux`를 추가한다. 플랫폼 차이는 `adapters/outbound/platform.rs`의 `Os` 값과 outbound 어댑터에만 둔다. `Os::current()`의 `cfg!`가 유일한 분기이며 Windows, Linux, 그 외(macOS) 세 갈래다.
  2. 엔진은 `WhisperX`만 제공한다(Qwen3/MLX 없음). 연산 장치는 `[Cpu, Cuda]`이고, 락은 `requirements-linux-cpu.lock`(`+cpu` torch는 PyTorch CPU 인덱스, `--index-strategy unsafe-best-match`)과 `requirements-linux-cuda.lock`(PyPI torch 2.8.0 = cu128 + `nvidia-*` 휠, 추가 인자 없음)이다. 준비 마커는 `<버전>+linux-<cpu|cuda>-<락 지문>+py3.12`이며 지문은 `build.rs`가 계산한다. Python 요청은 `3.12`로 두는데, 정확한 패치를 고정한 이유(Windows junction)가 Linux에는 없다.
  3. 기본 연산 장치: `cuda_driver_present(os, system_root, filesystem_root)`가 Linux에서 `libcuda.so.1`(multiarch, `usr/lib64`, WSL의 `usr/lib/wsl/lib` 등)이나 `/proc/driver/nvidia/version`을 찾으면 CUDA, 아니면 CPU다. 루트를 주입받는 순수 함수라 Windows 툴체인에서도 테스트한다.
  4. 워커 환경은 macOS 형태(HOME, PATH, TMPDIR)이되 로케일은 어디나 있는 `C.UTF-8`이다. 작업 디렉터리는 `/`.
  5. 녹음 중 절전 방지는 `systemd-inhibit --what=idle:sleep … cat` 자식 프로세스로 한다(`recording/power/linux.rs`). 의존성과 `unsafe`가 없고, 파이프된 stdin을 `cat`이 읽으므로 앱이 비정상 종료돼도 락이 풀린다. 시작할 수 없으면 기존 경로대로 로그만 남기고 녹음을 계속한다.
  6. 비밀 값은 macOS와 같이 `0600` `settings.json`(`~/.local/share/com.m16khb.galpi`)에 둔다.
  7. 빌드: `uv` 0.12.5 `x86_64-unknown-linux-gnu` 사이드카(SHA-256 고정), `bun run build`는 `.deb`와 AppImage를 만들고, CI에 `ubuntu-24.04` 잡을 둔다. 락 재현성 검사는 `scripts/verify-lock.py <windows-cpu|windows-cuda|linux-cpu|linux-cuda>`로 일반화했다.
- Alternatives rejected:
  - Secret Service/libsecret(keyring) 저장: 헤드리스·최소 데스크톱 세션에는 키링이 없을 수 있고 D-Bus 의존이 늘어난다. 서명 이후 macOS Keychain과 함께 다시 검토한다.
  - `logind` D-Bus를 직접 호출하는 절전 방지(`zbus` 등): 새 의존성과 비동기 런타임 결합이 필요하다. `systemd-inhibit`은 같은 락을 의존성 없이 건다.
  - `sleep infinity` 자식: 앱이 `Drop` 없이 죽으면 고아가 되어 락이 영구히 남는다. 파이프 EOF로 끝나는 `cat`은 그렇지 않다.
  - 로케일 `ko_KR.UTF-8` 유지: Linux에는 생성되지 않았을 수 있고, 없으면 일부 도구가 경고한다.
  - `nvidia-smi`로 탐지: 외부 프로세스 의존이 늘고 WSL에서는 경로가 다르다.
- Consequences: Linux 빌드에는 Tauri 시스템 패키지(`libwebkit2gtk-4.1-dev`, `libasound2-dev` 등)가 필요하다. `systemd-inhibit`이 없는 환경(컨테이너, 비 systemd 배포판)에서는 절전 방지 없이 녹음한다. `Os::Linux`가 만드는 값은 모두 Windows 툴체인 단위 테스트가 덮지만, `recording/power/linux.rs`와 Linux 네이티브 컴파일은 Linux 호스트에서만 검증된다.
- Evidence: `src-tauri/src/adapters/outbound/platform.rs`, `platform/tests.rs`, `environment.rs`, `recording/power/linux.rs`, `scripts/sidecar-targets.ts`, `scripts/build.ts`, `scripts/verify-lock.py`, `.github/workflows/ci.yml`.
