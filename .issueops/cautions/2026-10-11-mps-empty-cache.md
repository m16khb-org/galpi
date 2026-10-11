---
name: 2026-10-11-mps-empty-cache
description: Caution record for a solved false case or recurring risk.
---

# MPS 모델에 길이가 제각각인 입력을 넣으면 empty_cache로 풀리지 않는 메모리가 쌓인다

- Date: 2026-10-11
- Kind: `caution`
- Source: autoresearch/ai-ai-ai-20261009 (8eeea43, cb024c5, 6896d62)
- Summary: MPS는 입력 길이마다 계산 그래프와 버퍼를 따로 만들고, torch.mps.empty_cache()는 이 메모리를 돌려주지 못한다. WhisperX 문장 정렬이 문장마다 다른 길이로 wav2vec2를 돌리자 33분 회의에서 약 14GB(other allocations)가 쌓였고, 정렬과 그 뒤 화자분리가 메모리 부족으로 CPU로 넘어가 40분 회의 처리에 1시간 이상 걸렸다.
- Context: 16GB Mac에서 40분 회의를 WhisperX 프리셋으로 전사할 때 정렬 뒤 MPS 풀에 약 12.4GB가 남아 화자분리가 CPU로 폴백했다. 먼저 runtime.release_torch_cache()(gc + mps/cuda empty_cache)를 정렬 뒤와 CPU 재시도 전에 부르게 했다(8eeea43, 같은 프로세스에서 12.43GB → 6.98GB). 그래도 33분 회의(D23_017)에서는 정렬 자체가 메모리 부족으로 실패했다. 정렬을 50문장씩 나누고 캐시를 비우는 방식도 그래프 메모리를 풀지 못했다.
- Resolution: 입력 길이의 종류를 몇 개로 고정한다. 문장 정렬은 alignment.PaddedAligner가 구간 음성을 1초 단위로 채우고 attention mask를 준 뒤 output_frames로 실제 프레임만 남긴다(cb024c5). 단어 시각은 CPU·MPS 모두 원래와 같았고(700/700, 다른 회의 512/512), D23_017은 MPS 정렬 74초, 정렬 뒤 드라이버 메모리 2.08GB, MPS 화자분리 158초로 끝났다. 줄 목소리 임베딩도 voice.WIDTH_SECONDS의 다섯 가지 고정 길이로 묶는다(6896d62). 단 임베딩 모델은 0 채움이나 풀링 마스크가 결과를 바꾸므로(코사인 약 0.6) 자기 소리를 반복해 채운다. 새 MPS 모델 호출을 추가할 때는 입력 길이를 버킷으로 고정하고, 채움 방식이 출력을 바꾸지 않는지 CPU 원래 결과와 비교한다. 상세 수치는 .issueops/adr/2026-10-11-speaker-relabel-by-line-voice.md에 있다.
