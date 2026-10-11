---
name: 2026-10-09-windows-asr-on-cuda
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Windows CUDA 빌드에서 ASR도 CTranslate2 CUDA float16으로 실행

- Date: 2026-10-09
- Kind: `adr`
- Supersedes: [2026-10-09-windows-x64-os-cpu-cuda](2026-10-09-windows-x64-os-cpu-cuda.md) 결정 (3)의 "ASR은 계속 CTranslate2 CPU int8"
- Context: CUDA 빌드를 설치해도 ASR은 CPU int8로만 돌아 긴 회의에서 전사 단계가 수십 분 걸렸다. torch `2.8.0+cu128` 휠은 CTranslate2 4.8.2가 런타임에 불러오는 cuBLAS 12·cuDNN 9 DLL을 `torch/lib`에 함께 싣고, WhisperX가 CTranslate2보다 torch를 먼저 import하므로 별도 패키지 없이 DLL이 해석된다.
- Decision: 워커의 `runtime.select_asr_runtime`이 torch 장치가 `cuda`이고 `ctranslate2.get_cuda_device_count() > 0`이면 ASR을 `cuda`/`float16`, 그 외(macOS MPS 포함)는 `cpu`/`int8`로 정한다. CUDA ASR이 실패하면(VRAM 부족 등) 로그를 남기고 CPU int8로 한 번 다시 실행한다. 호스트·락·프로토콜은 바뀌지 않는다.
- Alternatives rejected:
  - `int8_float16`: VRAM은 더 적지만 RTX 3070 실측에서 `float16`의 최대 할당이 248 MB로 충분히 작았고, 정확도 손실 위험을 둘 이유가 없다.
  - nvidia-cublas/cudnn 휠을 락에 추가: torch 휠이 이미 같은 DLL을 싣고 있어 용량만 늘어난다.
  - 설정에 ASR 장치를 별도로 노출: 연산 장치 선택(CPU/CUDA) 하나로 충분하고, 선택지가 늘면 준비 상태 조합만 복잡해진다.
- Consequences: Windows 실기(RTX 3070, ComfyUI가 GPU 5 GB 점유 중)에서 120초 오디오 ASR이 CPU int8 40.1초 → CUDA float16 2.5초였고 첫 문장 텍스트가 같았다. 5분 클립 전체 파이프라인(`python -m galpi_worker transcribe`)은 84초에 끝났다. ASR 진행률 메시지는 `CTranslate2 CUDA float16`처럼 실제 런타임을 표시하고, ASR과 화자분리는 WhisperX 진행 콜백으로 단계 안에서 퍼센트가 오른다(화자분리는 문장별 목소리 확인에 넘기는 80% 아래에서 멈춘다). 문장 정렬은 `alignment.py`가 길이별로 나눠 돌리므로 진행 콜백을 쓰지 않는다. 준비 단계(`preparation.py`)는 모델 다운로드 목적이라 CPU int8 로드를 유지한다.
- Note: feat/cross-platform-upgrade(7b99816)에서 2026-10-11 main으로 옮겼다.
- Evidence: `worker/galpi_worker/runtime.py` (`select_asr_runtime`, `detect_asr_runtime`), `worker/galpi_worker/engine.py` (`run_asr`), `worker/tests/test_devices.py`.
