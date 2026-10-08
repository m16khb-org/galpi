# 요청자 의도 계약

- lifecycle: io-21fd3b1e1f06
- issue: https://github.com/m16khb-org/galpi/issues/3
- intent_class: standard

## 원문 요청
windows도 지원해야하고 codex sign in도 지원해야 하고 당근 디자인 시스템도 적용해야해 로 각각 이슈부터 만들자

## 해석
갈피를 Windows 10/11 x64에서 설치·실행·녹음·전사·회의록 정제까지 되는 데스크톱 앱으로 만든다. 현재 Rust 아웃바운드 어댑터(Keychain, IOKit sleep assertion, nix 프로세스 그룹, PATH/venv 경로), 사이드카 스테이징(aarch64 uv), DMG 패키징·CI, 워커 프리셋(Qwen3=MLX 전용, WhisperX=CPU/MPS)이 macOS ARM64에 고정돼 있어 플랫폼 레이어를 포트 뒤로 분리하고 Windows 구현을 추가한다. Windows에서는 WhisperX 프리셋(CPU 기본, CUDA 선택)만 제공하고 Qwen3/MLX 프리셋은 숨긴다. ARM64 Windows와 Intel macOS는 범위 밖이다.

## 성공 기준
- 1) `cargo build --target x86_64-pc-windows-msvc` 와 `bun run build`가 Windows 러너에서 NSIS 설치본을 만든다. 2) Windows에서 설치본 실행 후 환경 진단이 통과하고 WhisperX 프리셋으로 샘플 오디오 전사가 .srt/_화자별.txt/.aligned.v2.json을 만든다. 3) 녹음 시작/중지/취소가 동작하고 .wav.part 정리가 유지된다. 4) 전사 취소 시 워커 자식 프로세스 트리가 종료된다(Job Object). 5) HF 토큰·assistant API 키가 Windows Credential Manager에 저장되고 재시작 후 유지된다. 6) 기존 macOS CI 게이트가 그대로 통과한다. 7) CI 매트릭스에 windows 러너가 추가되고 Rust/TS/Python 게이트가 통과한다.

## 비목표
- (없음)

## 제약
- (없음)

## 모호함
- (없음)

## 읽는 규칙
이 문서는 요청자 의도 계약이다. 원격 이슈 본문은 구현 계약이다. 두 문서가 충돌하면 구현을 시작하지 말고 충돌한 줄을 인용해 blocker로 보고한다.
