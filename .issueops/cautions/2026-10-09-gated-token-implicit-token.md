---
name: 2026-10-09-gated-token-implicit-token
description: Caution record for a solved false case or recurring risk.
---

# gated 모델은 token=을 명시해야 한다: 워커는 implicit token이 꺼져 있다

- Date: 2026-10-09
- Kind: `caution`
- Source: issue #3 Windows 실기 1차 G6 FAIL
- Summary: 워커 환경의 HF_HUB_DISABLE_IMPLICIT_TOKEN=1 때문에 HF_TOKEN 환경 변수는 자동으로 쓰이지 않는다. gated 모델(pyannote/speaker-diarization-community-1)을 내려받는 모든 호출은 token=os.environ.get('HF_TOKEN')을 직접 넘겨야 한다.
- Context: WhisperX 준비 경로의 DiarizationPipeline이 token을 넘기지 않았다. macOS에서는 기본 Qwen3 준비가 토큰과 함께 pyannote를 먼저 캐시해 드러나지 않았고, Qwen3가 없는 Windows 실기(2026-10-09)에서 401 GatedRepo로 준비가 실패했다.
- Resolution: 준비(prepare) 경로의 gated 다운로드는 모두 token=을 명시한다. 전사 경로는 토큰 없이 캐시만 쓴다(huggingface_hub 0.36.2는 gated 401에서 캐시로 대체). 새 gated 모델이나 새 준비 경로를 추가하면 Qwen3를 거치지 않은 깨끗한 캐시에서 준비를 확인한다.
