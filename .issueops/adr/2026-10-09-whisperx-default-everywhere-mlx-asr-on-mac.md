---
name: 2026-10-09-whisperx-default-everywhere-mlx-asr-on-mac
description: Accepted decision record with rationale, alternatives, and consequences.
---

# WhisperX가 모든 플랫폼의 기본 엔진, Mac은 음성 인식만 MLX

- Date: 2026-10-09
- Kind: `adr`
- Context: 사용자는 세 플랫폼을 최신 WhisperX로 통일하되 Mac GPU(MLX) 속도는 유지하기를 원했다. 공식 WhisperX 3.8.6(최신 안정판, 3.8.7rc1은 사전 배포)은 CTranslate2로만 음성 인식을 하고, CTranslate2 4.8.2(최신)는 Metal 백엔드가 없다(실험 PR #2077은 소스 빌드 전용). WhisperX 상류의 MLX PR #1185는 닫혔다. `whisperx.align`은 `{text,start,end}` 구간만 받으므로 음성 인식 단계만 바꿀 수 있다. 독립 한국어 벤치마크(seastar105/oss-asr-korean-benchmark)에서 Whisper large-v3-turbo는 회의·강의·통화 데이터에서 Qwen3-ASR-1.7B보다 CER이 낮고 낭독 음성에서는 비슷했다. MLX turbo의 실시간 배율은 Qwen3 MLX와 비슷하다(M1 Max RTF ≈0.09).
- Decision: (1) 기본 엔진은 모든 플랫폼에서 WhisperX다. 음성 구간 검출(WhisperX Pyannote VAD), 30초 청크, wav2vec2 한국어 정렬, pyannote community-1 화자분리는 공식 WhisperX 코드를 그대로 쓴다. (2) 음성 인식 런타임만 `runtime.select_asr_runtime`이 고른다: `mlx-whisper`가 설치된 곳(macOS 락)은 MLX float16(`mlx-community/whisper-large-v3-turbo`, `worker/galpi_worker/mlx_asr.py`), torch와 CTranslate2가 모두 GPU를 보면 CUDA float16, 그 외 CPU int8. 가속 런타임 실패 시 CPU int8로 한 번 재시도한다. (3) Qwen3는 `mlx-qwen3-asr` 0.4.4로 올려 Mac 선택 프리셋으로 남긴다. 저장된 Qwen3 선택은 존중한다. (4) 화자분리는 pyannote.audio 4.0.7(최신)과 community-1을 유지한다. NVIDIA Nemotron-3-Diarization은 영어·중국어 벤치마크에서 더 낫지만 공개 16일째이고 한국어·3시간 녹음 검증이 없으며 화자 8명 상한이 있어 보류한다. (5) 네 락(macOS WhisperX, Windows CPU·CUDA, Qwen3)을 같은 `--exclude-newer` 기준 시점과 `-U`로 다시 만들어 공통 패키지 버전을 맞춘다. macOS 락은 `MACOSX_DEPLOYMENT_TARGET=14.0`으로 해석한다. 준비 마커는 이제 `requirements*.txt`가 아니라 해석된 락의 지문을 쓴다.
- Alternatives rejected:
  - 공식 WhisperX만 사용(Mac 음성 인식 CPU): 측정상 MLX보다 수 배 느리다.
  - `whispermlx` 포크: 유지보수자 1명, 월 744 다운로드, 상류와 버전 체계가 달라 통일 취지와 맞지 않는다.
  - `mlx-whisperx`: torch 2.11·transformers 5가 필요해 WhisperX 3.8.6의 `torch~=2.8.0`과 충돌한다.
  - WhisperKit·whisper.cpp: Swift 전용이거나 별도 파이프라인이라 번들 uv venv 구조와 맞지 않는다(whisper.cpp는 `mlx-whisper`가 멈출 때의 대안으로 남긴다).
  - torch 2.14로 업그레이드: WhisperX 3.8.6이 `torch~=2.8.0`을 고정한다.
- Consequences: Mac 음성 인식은 그리디 디코딩(빔 서치 없음)이고 청크 단위로 처리한다. 결과 구간 형태는 CTranslate2 경로와 같아서(VAD 청크당 한 구간) 이후 단계는 바뀌지 않는다. 기존 설치는 락 지문이 바뀌어 엔진 재설치를 한 번 거친다. MLX 경로의 실제 Mac 실행은 아직 검증하지 않았다(가짜 `mlx_whisper`와 실제 WhisperX VAD로 청크·옵션·진행률만 Windows에서 확인).
- Evidence: `worker/galpi_worker/runtime.py`, `worker/galpi_worker/mlx_asr.py`, `worker/galpi_worker/engine.py`, `worker/galpi_worker/preparation.py`, `worker/requirements*.txt`, `worker/tests/test_mlx_asr.py`, `worker/tests/test_devices.py`. Windows RTX 3070 실기: CUDA 락 신규 설치 후 5분 클립 110초, 토큰 없이 미러 다운로드.
