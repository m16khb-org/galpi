# 계획 검토 기록

## 1차: 수정 요청

- 리뷰 모델 anthropic/claude-opus-5-5, effort high. 시스템 트랙을 출력 장치의 mix format 채널 수 그대로 16-bit WAV에 쓰는 설계를 공격했습니다. 8채널 48 kHz 장치에서는 약 93분에 WAV_TOO_LARGE가 나고, 공유 실패와 프런트 cancelRecording 때문에 마이크 트랙까지 지워진다는 점을 writer.rs와 recording-controller.ts로 확인했습니다.
- 무음 유지 스트림과 macOS 입력 겸용 장치 거부를 #[cfg]로 나누는 계획을 공격했습니다. docs/ARCHITECTURE.md는 두 플랫폼에서 모두 컴파일되는 cpal 코드를 Os 값으로 분기하도록 정하므로 power.rs(Win32 FFI)를 선례로 든 것은 맞지 않고, Windows 전용 게이트에서 macOS 분기가 컴파일되지 않는 문제도 생깁니다.
- WASAPI 루프백 플래그, 무음 시 패킷 부재, 기본 장치 변경 시 StreamInvalidated, 설정 serde 호환, 명령 수 38, 정리 경로에 대한 계획의 주장은 cpal 0.18.2 소스와 저장소 코드에 대조해 근거와 일치했습니다.

## 2차: 통과

- 리뷰 모델 anthropic/claude-opus-5-5, effort high, delta 리뷰. 8채널 출력 장치에서 녹음 한도가 줄어 마이크 트랙까지 잃는 경로를 공격했고, writer에서 mono로 줄이고 버린 양을 프레임으로 환산하면서 한도가 마이크 mono 트랙과 같아져 해소됐습니다.
- macOS 입력 겸용 장치 거부와 Windows 무음 유지 스트림이 #[cfg] 없이 두 플랫폼에서 모두 컴파일되는지 확인했고, 둘 다 cpal 공통 DeviceTrait API를 Os::current()로 나누므로 아키텍처 규칙에 맞습니다.
- G3의 grep이 cfg!( 형태를 놓치는 점은 막지 않는 검증 보강 사항이라 판정을 바꾸지 않으며, 구현 검증에서 저장소 펜스와 같은 정규식을 쓰기로 했습니다.
