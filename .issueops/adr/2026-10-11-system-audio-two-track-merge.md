---
name: 2026-10-11-system-audio-two-track-merge
description: Accepted decision record with rationale, alternatives, and consequences.
---

# 마이크와 시스템 소리 트랙은 정지 뒤에 합치고, Windows 루프백은 무음 출력 스트림으로 살려 둔다

- Date: 2026-10-11
- Kind: `adr`
- Source: galpi#14
- Summary: `마이크 + 회의 소리`는 녹음 중 두 트랙(`<이름>.mic.wav.part`, `<이름>.system.wav.part`)을 따로 저장하고, 정지 뒤 blocking 스레드에서 마이크 샘플레이트의 mono 16-bit `<이름>.wav` 하나로 합친다. Windows는 기본 출력 장치에 무음 렌더 스트림을 녹음 내내 열어 둬 루프백이 무음 구간에도 데이터를 보내게 한다. (Merge the microphone and system-audio tracks after stop; keep Windows loopback alive with a silent render stream.)
- Context: 화상회의 상대방 목소리는 컴퓨터 출력 소리에만 있어 마이크만으로는 녹음되지 않는다. 두 입력은 장치 클럭과 샘플레이트가 다르다. WASAPI 루프백은 재생 중인 소리가 없으면 콜백이 오지 않아 트랙이 짧아진다. macOS는 Core Audio 프로세스 탭(14.6+, `NSAudioCaptureUsageDescription` 권한)을 cpal로 연다.
- Decision: 시스템 소리는 Windows에서 기본 출력 장치의 WASAPI 루프백(cpal 입력 스트림)으로 연다. 같은 장치에 무음 렌더 스트림을 녹음 동안 열어 둔다. 시스템 트랙은 writer 스레드 안에서 mono로 합산한다. 정지 뒤 합치기는 시스템 트랙을 선형 보간으로 리샘플하고, 합을 clamp하며, 짧은 트랙은 0으로 채우는 스트리밍 방식이라 메모리가 일정하다. 합친 뒤 두 트랙 파일을 지운다. 마이크만은 기존 `<이름>.wav.part` → `<이름>.wav` 그대로다. 실패 코드는 `SYSTEM_AUDIO_UNAVAILABLE`(마이크만으로는 된다고 안내), `SYSTEM_AUDIO_DISCONNECTED`, `SYSTEM_AUDIO_ERROR`, `WAV_MIX_FAILED`다. 플랫폼 분기는 `Os::current()` 값으로 한다. 워커 프로토콜, Zod 결과 스키마, Drive 업로드, 전사는 바뀌지 않는다.
- Alternatives rejected:
  - CPAL 콜백 안에서 실시간으로 섞기: 두 장치의 클럭·샘플레이트가 달라 맞추려면 콜백에서 버퍼링과 리샘플이 필요하고, 콜백을 막지 않는다는 규칙을 깬다.
  - 전사 직전에 워커가 ffmpeg로 합치기: Drive 업로드·재생에서 "녹음 파일 하나" 전제가 깨지고 워커 프로토콜이 바뀐다.
  - rubato 같은 리샘플링 crate 추가: 워커가 어차피 16 kHz로 다시 리샘플하는 말소리에는 선형 보간으로 충분하고, 의존성은 TECH_STACK 유지 비용을 만든다.
  - 무음 렌더 스트림 대신 WASAPI 콜백 타임스탬프로 무음 채우기: 장치마다 다른 cpal 타임스탬프 정확도에 기대게 된다.
- Consequences: 정지 뒤 합치는 시간이 추가되어 정지 문구 `녹음 파일을 마무리합니다.`가 합치기까지 맡는다(소요 시간 추정은 표시하지 않는다). 녹음 중 디스크에는 두 트랙이 있고 취소하면 둘 다 지운다. 한계: 에코 제거가 없어 스피커로 나온 상대 목소리를 마이크가 다시 녹음하면 두 번 들어가므로 헤드폰을 권한다. 앱별 캡처와 장치 선택은 없고 기본 장치만 쓴다. 두 스트림의 시작 시각 차이(수 ms)는 보정하지 않는다. macOS는 개발 PC에서 검증하지 못했다. Tauri 커맨드는 38개에서 40개(`load_recording_source`, `save_recording_source`)가 되고 `start_recording`에 `source` 인자가 생겼다.
- Evidence: `src-tauri/src/adapters/outbound/recording/system.rs`, `recording/mix.rs`, `src-tauri/Info.plist`, `.issueops/testing/overview.md`, galpi#14
