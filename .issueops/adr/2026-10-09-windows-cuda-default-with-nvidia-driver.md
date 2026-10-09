---
name: 2026-10-09-windows-cuda-default-with-nvidia-driver
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Windows: NVIDIA 드라이버가 있으면 CUDA가 기본 연산 장치

- Date: 2026-10-09
- Kind: `adr`
- Supersedes: [2026-10-09-windows-x64-os-cpu-cuda](2026-10-09-windows-x64-os-cpu-cuda.md)의 결정 (3) 중 "기본은 CPU, CUDA는 설정에서 고를 때만 설치"
- Superseded in part: Consequences의 "ASR은 계속 CTranslate2 CPU int8"은 [2026-10-09-windows-asr-on-cuda](2026-10-09-windows-asr-on-cuda.md)로 대체.
- Context: NVIDIA GPU가 있는 Windows 사용자가 설정에서 CUDA를 직접 고르지 않으면 CPU 락이 설치되어 정렬·화자분리가 CPU로만 돌았다. 사용자는 GPU가 있으면 CUDA가 자동으로 설정되기를 요청했다.
- Decision: 저장된 연산 장치가 없을 때의 기본값을 `Os::default_device(cuda_driver)`로 정한다. Windows이고 `<SystemRoot>\System32\nvcuda.dll`이 있으면 `Cuda`, 그 외에는 `Cpu`. 드라이버 탐지는 `LocalSettingsStore::new`에서 앱 시작 때 한 번 하고, 사용자가 저장한 장치(`computeDevice`)는 기본값보다 항상 우선한다.
- Alternatives rejected:
  - 탐지 결과를 `computeDevice`로 저장: 사용자의 명시적 선택과 자동 기본값을 구분할 수 없어 드라이버 제거·추가를 따라가지 못한다.
  - 저장된 `cpu`도 드라이버가 있으면 `cuda`로 덮어쓰기: 사용자가 일부러 고른 CPU를 무시한다.
  - `nvidia-smi`로 드라이버 버전(cu128은 R570+)까지 확인: 외부 프로세스 의존이 늘고, 오래된 드라이버는 워커의 `torch.cuda.is_available()` 실패 시 CPU로 동작하므로 기능은 유지된다.
- Consequences: 장치를 고른 적 없는 기존 Windows 설치는 업데이트 후 준비 마커가 `win-cuda`로 바뀌어 로컬 엔진 준비를 다시 요구하고 약 3.5 GB를 내려받는다. 이미 `computeDevice: "cpu"`가 저장된 설치는 그대로 CPU다. ASR은 계속 CTranslate2 CPU int8이며 CUDA는 정렬·화자분리만 가속한다. 앱 실행 중 드라이버를 설치하면 재시작 후 기본값에 반영된다.
- Evidence: `src-tauri/src/adapters/outbound/platform.rs` (`Os::default_device`), `src-tauri/src/adapters/outbound/settings.rs` (`load_compute_device`), 테스트 `cuda_is_the_default_device_only_where_offered_and_a_driver_is_installed`, `os_windows_with_an_nvidia_driver_defaults_to_cuda_until_cpu_is_chosen`.
