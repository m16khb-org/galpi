---
name: 2026-10-09-remove-hugging-face-token-via-community-1-mirror
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Hugging Face 토큰 기능을 없애고 화자분리 모델은 community-1 미러에서 받는다

- Date: 2026-10-09
- Kind: `adr`
- Context: `pyannote/speaker-diarization-community-1`은 gated 저장소라서 첫 모델 내려받기에 Hugging Face 계정, 이용 조건 동의, Read 전용 토큰이 필요했다. 그래서 설정 시트에 토큰 입력·발급 안내·이용 조건 페이지 버튼이 있었고, 호스트는 토큰을 설정 파일(macOS) 또는 Credential Manager(Windows)에 보관해 준비 때 `HF_TOKEN`으로 워커에 넘겼다. `pyannote-community/speaker-diarization-community-1`은 같은 가중치의 비게이트 CC-BY-4.0 미러다(바이트 동일).
- Decision: (1) 화자분리 모델은 미러에서 받는다. 호스트의 준비 완료 판정은 `models--pyannote-community--speaker-diarization-community-1` 캐시 디렉터리를 본다. (2) 토큰 기능을 끝까지 걷어낸다: Tauri 명령 `hugging_face_token_stored`·`save_hugging_face_token`, `SettingsPort`의 토큰 메서드, `SetupRequest.hugging_face_token`, `Secret::HuggingFaceToken`, `HF_TOKEN` 환경 전달, 오프라인 캐시 판정의 토큰 조건, 프런트엔드의 토큰 입력·발급 안내·모델 이용 조건 버튼(`BackendPort`의 토큰 메서드와 `openModelAccessPage`)과 `opener` 권한 범위의 `huggingface.co`. (3) 이전 빌드가 남긴 것만 정리한다. `settings.json`의 `huggingFaceToken`·`huggingFaceTokenStored`는 읽기만 하고 쓰지 않는다(`LocalSettings`는 `deny_unknown_fields`가 없어 파일은 그대로 읽힌다). 설정을 처음 읽을 때 이 키가 있으면 파일을 키 없이 다시 쓰고, `huggingFaceTokenStored`가 참이면 `SecretStore::forget_retired_token`으로 Windows 자격 증명 `com.m16khb.galpi:hugging-face-token`(과 조각 항목)을 한 번 최선 노력으로 지운다. 지우기가 실패해도 설정 읽기는 막지 않는다.
- Alternatives rejected:
  - 토큰 입력을 남기되 선택 사항으로 둔다: 미러가 비게이트라 쓸 곳이 없고, 비밀 값을 보관하는 코드(자격 증명 저장소, 설정 파일 평문)만 남는다.
  - 기존 `settings.json`을 마이그레이션 없이 둔다: macOS에서는 평문 토큰이 파일에 계속 남고, Windows에서는 Credential Manager 항목이 영원히 고아로 남는다.
  - 자격 증명을 매번 시작 때 지운다: 설정 파일의 플래그를 지우고 나면 필요 없는 Win32 호출을 반복한다. 플래그가 있을 때 한 번만 지운다.
  - 미러 대신 원본 저장소를 번들하거나 재배포한다: 용량이 크고 라이선스·갱신 책임이 앱에 생긴다.
- Consequences: 첫 준비에 계정이 필요 없다. 미러는 원본과 별개의 저장소라 사라지거나 내용이 바뀌면 준비가 실패하며, 이때는 원본으로 되돌릴 수 없다(토큰 경로를 지웠다). 모델은 CC-BY-4.0이므로 출처 표기를 유지해야 한다. 기존 설치는 이전 모델 캐시 디렉터리 `models--pyannote--speaker-diarization-community-1`을 더는 인정하지 않아 준비를 한 번 다시 거친다. 이전 빌드로 되돌리면 저장된 토큰은 이미 지워져 있다. IPC 명령은 24개에서 22개가 된다.
- Evidence: `src-tauri/src/adapters/outbound/settings.rs`(`retire_hugging_face_token`과 테스트 `a_settings_file_from_the_token_era_loads_and_is_scrubbed_once`), `src-tauri/src/adapters/outbound/secrets/credential_manager.rs`(`forget_retired_token`), `src-tauri/src/adapters/outbound/environment.rs`(`PYANNOTE_MODEL_DIR`), `src-tauri/src/adapters/outbound/model_cache.rs`, `src/ui/settings-dialog.ts`. 이전 규칙의 기록: [2026-10-09-gated-token-implicit-token](../cautions/2026-10-09-gated-token-implicit-token.md).
