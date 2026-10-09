---
name: 2026-10-09-windows-x64-os-cpu-cuda
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Windows x64 지원: 플랫폼 분기는 Os 값으로, CPU 기본·CUDA 선택

- Date: 2026-10-09
- Kind: `adr`
- Source: issue #3, .issueops/issues/3/plan.md
- Summary: Windows 10/11 x64를 macOS Apple Silicon과 함께 지원한다. 플랫폼 차이는 outbound 어댑터와 composition.rs에만 두고, adapters/outbound/platform.rs의 Os 값으로 분기한다.
- Context: 고정 지점(전원 IOKit, nix 프로세스 그룹, 경로·PATH, uv 사이드카, ARM64 락, DMG)이 outbound에 모여 있었고 domain/application에는 플랫폼 코드가 없었다. Windows에는 MLX(Qwen3)가 없고 CUDA torch 휠은 CPU 휠의 약 14배(3.46 GB vs 241 MB)다.
- Decision: (1) cfg!(windows)는 Os::current() 한 곳, #[cfg]는 Win32 FFI 3파일(process/guard/windows.rs, recording/power/windows.rs, secrets/credential_manager.rs)과 nix 전용 파일에만 둔다. check-architecture.ts가 domain·application·inbound에서 cfg(windows)·cfg(unix)·target_os·cfg!( 를 금지한다. (2) Windows 기본 엔진은 WhisperX, Qwen3는 숨기고 저장값은 WhisperX로 정정한다. (3) Windows 락은 CPU(PyPI torch 2.8.0)와 CUDA(torch 2.8.0+cu128) 두 개이고 기본은 CPU, CUDA는 설정에서 고를 때만 설치한다. ASR은 계속 CTranslate2 CPU int8. (4) Windows 절전 억제는 SetThreadExecutionState가 아니라 PowerCreateRequest/PowerSetRequest(스레드 비결속). (5) 취소는 Job Object(KILL_ON_JOB_CLOSE)이며 정중한 단계도 즉시 TerminateJobObject. (6) 비밀은 Windows Credential Manager, macOS는 서명 전까지 0600 설정 파일 유지.
- Consequences: macOS 동작·준비 마커·설정 파일 형식은 불변(새 설정 키는 미설정 시 직렬화하지 않아 롤백 호환). Windows 전용 코드는 Windows CI와 실기 수동 게이트로만 실행 검증된다. 기각: keyring 크레이트(의존 증가·TargetName 제어 약화), 도메인 EnginePreset의 cfg 기본값(분기 규칙 위반), CUDA 단일 락(전원 3.46 GB 강제), 어댑터 쪼개기·레지스트리 포트화(헥사고날 ADR의 기각안).
