# 계획: 회의 소리(시스템 출력)까지 녹음 — galpi#14

- lifecycle: `io-cba4c0ac8466`
- issue: https://github.com/m16khb-org/galpi/issues/14
- branch: `14-system-audio-recording` (base `main` @ `7af8bc8655bb46e2df2b3acef35fc692bd0a3e20`)
- 사용자 요청 범위: IssueOps 전체 사이클로 진행하고, 구현은 Sonnet 5.5 세션에 인계한다. 종료점은 draft PR 발행과 `execution complete`다. 머지·정리는 범위 밖이다.
- 세션: 워크트리 준비 뒤 환경별 자동 세션 인계(준비 세션이 Herdr 안이므로 Herdr `new-session`). 새 세션은 omp, 모델 `anthropic/claude-sonnet-5-5`. 인계는 승인 범위를 넓히지 않는다.

## 목표와 비목표

목표: 녹음 카드에서 `마이크만` / `마이크 + 회의 소리`를 고르고, 후자는 기본 마이크 입력과 기본 출력 장치의 시스템 소리를 함께 녹음해 회의 폴더에 WAV 하나(`<이름>.wav`)로 남긴다. 이후 전사·화자분리·Drive 업로드·회의 폴더 이름 규칙은 그대로다.

비목표: 앱별 소리 선택, 녹음 중 실시간 믹싱, 에코 제거, 장치 선택 메뉴, Linux, 화자분리를 트랙별로 나누는 개선.

## 적용되는 결정과 주의사항

- `AGENTS.md` ANTI-PATTERNS "Never block the CPAL callback; it feeds a bounded queue with nonblocking sends" → 시스템 출력 스트림도 기존 `capture::enqueue`(try_send, 버퍼 재사용)를 그대로 써야 한다. 합치기·리샘플링은 콜백이 아니라 정지 뒤 blocking 스레드에서 한다.
- `AGENTS.md` "Preserve process-group cancellation, bounded output lines, and `.wav.part` cleanup" → 두 트랙과 최종 파일 모두 `.part`로 쓰고, 정지 실패·버리기·시작 실패 때 모든 `.part`와 빈 회의 폴더를 지운다.
- `AGENTS.md` "Rust domain and application layers stay framework-free" / `docs/ARCHITECTURE.md` 포트 소유 규칙 → 녹음 소스 값 객체는 `domain`, 장치 해석(cpal)은 `adapters/outbound/recording`에만 둔다.
- `docs/ARCHITECTURE.md:66-70` 플랫폼 분기 규칙 → outbound 코드의 플랫폼 분기는 `Os::current()` 값으로 하고, `#[cfg]`는 다른 플랫폼에서 컴파일할 수 없는 코드에만 쓴다. 무음 유지 스트림과 macOS 입력 겸용 장치 거부는 cpal 공통 API라 `Os` 분기다.
- `AGENTS.md` "Worker protocol, Rust parser, frontend event schema, and job reducer form one change set" → 이번 변경은 worker 프로토콜을 바꾸지 않는다(합치기를 호스트에서 하므로). worker 파일은 건드리지 않는다.
- `.issueops/cautions/2026-10-09-new-stored-credential-needs-four-match-arms-and-is-empty.md` → 새 설정 필드(`recording_source`)를 `LocalSettings`에 추가하면 `is_empty()`에도 넣어야 한다(빠뜨리면 다른 설정이 없는 설치에서 settings.json이 지워진다).
- `.issueops/cautions/2026-10-11-setbusy-kind-renderjob.md` → 해당 없음(녹음은 job panel을 쓰지 않음). 대조만 했다.
- `.issueops/cautions/2026-08-23-tauri-frontend-renders-in-a-plain-browser-but-is-ipc-dead-su.md` → 새 IPC 경로는 브라우저 미리보기에서 Not Run으로 분류하고, 실제 녹음은 설치한 Windows 앱에서 확인한다.
- ADR `2026-10-09-adopt-seed-css-tokens-with-the-action-button-recipe-only` → 소스 선택 UI는 기존 `segmented-control` 마크업과 SEED 토큰으로 만든다. 새 레시피·hex·px 토큰을 추가하지 않는다.
- `DESIGN.md` §Settings Autosave "selects, switches … persist immediately" → 소스 선택은 바꾸는 즉시 저장한다.
- `.issueops/testing/overview.md` "Manual Windows gates" → 마이크 녹음 후 버리기 확인 항목이 있다. 새 소스에도 같은 수동 확인을 추가한다.
- `.issueops/CONSTITUTION.md` → secret 없음. 진행 시간 추정 문구를 만들지 않는다(합치는 동안 "녹음 파일을 마무리합니다"만 표시).
- 대조했으나 이 변경에 해당 없음: `.issueops/OPEN_API_SPEC.md`(HTTP API 없음), worker 관련 ADR, Drive ADR(업로드 흐름 불변).

## 사실 확인(현재 동작)

- 녹음은 기본 입력 장치 하나만 연다: `src-tauri/src/adapters/outbound/recording/mod.rs:107-113`(`host.default_input_device()`).
- 한 녹음 = 회의 폴더 하나 + `.wav.part` → 정지 시 `rename`으로 `.wav`: `mod.rs:100-105`, `mod.rs:213-218`.
- 콜백은 `capture::enqueue`에서 `try_send`만 하고 막히지 않는다: `capture.rs:113-160`. 에러 코드는 `MICROPHONE_DISCONNECTED`/`MICROPHONE_ERROR`로 고정돼 있다: `capture.rs:88-99`.
- writer는 hound `WavWriter` 16-bit PCM, `RIFF_DATA_LIMIT` 검사: `writer.rs:10-12, 40-60, 219, 248`.
- `RecordingPort::start(recording_id, output_root)`: `application/ports.rs`, 호출은 `application/use_cases.rs`의 `start_recording(output_root)`, Tauri 명령 `adapters/inbound/tauri.rs:146-150`. 테스트에서 `app.start_recording("/tmp".to_owned())`를 4곳에서 부른다(`application/tests/recording.rs:47`, `tests/drive.rs:1497,1510`, `tests/gateway.rs:214`).
- 프런트 `BackendPort.startRecording(outputRoot)`: `src/domain/backend.ts:46`; 구현 `src/adapters/tauri-backend.ts:326`.
- 정지 중 문구는 `녹음 파일을 마무리합니다.`: `src/application/recording-machine.ts:60-62`.
- cpal 0.18.2: WASAPI는 `eRender` 장치에 입력 스트림을 열면 `AUDCLNT_STREAMFLAGS_LOOPBACK`(`cpal/src/host/wasapi/device.rs:854-856`). CoreAudio는 입력이 없는 장치면 `LoopbackDevice::from_device`로 프로세스 탭 aggregate를 만든다(`cpal/src/host/coreaudio/macos/device.rs:727-736`). README: 루프백은 macOS 14.6+.
- WASAPI 루프백은 재생 중인 소리가 없으면 데이터를 주지 않는다(Microsoft Loopback Recording 문서, NAudio 문서 "play silence through that device"). 그대로 두면 무음 구간이 빠져 시스템 트랙이 마이크 트랙보다 짧아지고 시간이 어긋난다.
- 호스트에는 ffmpeg가 없다. ffmpeg는 worker 가상환경(imageio-ffmpeg)에만 있다.

미확인 가정(구현 중 판별):
- A1 macOS에서 기본 출력 장치가 입력도 가진 장치(일부 USB 헤드셋)면 cpal은 루프백 대신 그 장치의 입력을 연다(`device.rs:727`). 이 경우 감지해 `SYSTEM_AUDIO_UNAVAILABLE`로 알릴지는 Mac에서 확인해야 하며, 이 PC에서는 판별할 수 없다. 코드에서는 `supports_input()`이 참인 출력 장치를 루프백 대상으로 쓰지 않고 명확한 오류를 낸다.
- A2 macOS 탭 aggregate는 무음에도 프레임을 계속 준다. Mac 실측 전에는 단언하지 않는다(PR에 미확인으로 남김).
- A3 Windows 무음 유지용 출력 스트림이 다른 앱의 재생 음량이나 장치 독점에 영향을 주지 않는다(공유 모드 무음). G6에서 실측한다.

## 설계

### 1. 녹음 소스 값 객체(domain)
- `src-tauri/src/domain/recording.rs`(새 파일) `RecordingSource { Microphone, MicrophoneAndSystem }`, serde `camelCase` → `"microphone" | "microphoneAndSystem"`, 기본값 `Microphone`.
- TS `src/domain/recording.ts` 같은 유니언 타입과 기본값. Zod는 `tauri-backend.ts`에서 `z.enum`.

### 2. 계약
- `RecordingPort::start(recording_id, output_root, source)`; `Application::start_recording(output_root, source)`; Tauri `start_recording(output_root, source)`; TS `startRecording(outputRoot, source)`.
- 설정: `SettingsPort::load_recording_source()` / `save_recording_source(source)`, `LocalSettings.recording_source: Option<RecordingSource>`(`skip_serializing_if = "Option::is_none"`, `is_empty()`에 추가). Tauri 명령 `load_recording_source`, `save_recording_source` → 명령 38 → 40.
- `RecordingResult` 모양은 그대로(경로·sample_rate·channels·frames·dropped_frames·duration_seconds). 합친 결과면 sample_rate는 마이크 rate, channels는 1, frames는 합친 파일의 프레임 수, dropped_frames는 두 트랙의 버려진 프레임 합(각 트랙 rate 기준 프레임을 합친 rate로 환산하지 않고, 버려진 '샘플 프레임 수'의 합으로 정의하고 주석으로 명시).

### 3. 네이티브 녹음기(adapters/outbound/recording)
- `ActiveRecording`에 `system: Option<SystemTrack>` 추가. `SystemTrack { stream, writer, partial_path(<이름>.system.wav.part), sample_rate, source_channels, dropped_samples, keepalive: Option<Stream> }`. 실패 상태는 마이크 트랙과 공유한다.
- 시스템 트랙은 **writer 스레드에서 mono로 줄여 저장한다**(WavSpec `channels = 1`). 출력 장치의 mix format은 5.1·7.1(6·8채널)일 수 있어(cpal `wasapi/device.rs:754-768`의 `GetMixFormat`), 채널 그대로 쓰면 8채널 48 kHz에서 약 93분 만에 `RIFF_DATA_LIMIT`(`writer.rs:12, 212-219`)에 걸리고, 공유 실패 뒤 프런트가 `cancelRecording`을 불러(`recording-controller.ts:117-141`) 마이크 트랙까지 지워진다. mono로 줄이면 시스템 트랙 한도가 48 kHz 기준 약 12.4시간으로 마이크 mono 트랙과 같아진다. 구현: `writer::spawn`에 `downmix_from: Option<u16>`(입력 채널 수)을 주면 writer가 받은 인터리브 청크를 프레임 평균으로 mono화해 쓴다. 콜백 경로(`capture::enqueue`)는 그대로 인터리브 샘플을 보낸다(콜백에서 계산을 늘리지 않는다). `dropped_before`·`trailing_dropped`는 샘플 수로 오므로 writer에서 `/ downmix_from`으로 프레임 수로 바꿔 무음을 채우고, `WriterSummary.dropped_samples`도 저장 단위(mono 샘플 = 프레임)로 보고한다. 기존 `Microphone` 경로는 `downmix_from: None`으로 동작 불변.
- `MicrophoneAndSystem`이면 마이크 트랙은 `<이름>.mic.wav.part`, 시스템 트랙은 `<이름>.system.wav.part`에 쓴다. `Microphone`이면 지금처럼 `<이름>.wav.part` 하나(동작 불변).
- 시스템 장치: `host.default_output_device()`, 없으면 `SYSTEM_AUDIO_UNAVAILABLE`("소리를 받을 출력 장치를 찾지 못했습니다."). `Os::current() == Os::MacOs`이고 그 장치가 `supports_input()`이면 같은 코드로 거부(A1). 설정은 `default_output_config()`.
- `capture::build`에 `CaptureErrors { disconnected: &'static str, error: &'static str }`를 받게 해 시스템 트랙은 `SYSTEM_AUDIO_DISCONNECTED`/`SYSTEM_AUDIO_ERROR`를 쓴다. 콜백 경로(`enqueue`)는 재사용, 변경 없음. 실패 상태(`SharedFailure`)는 두 스트림이 하나를 공유해 어느 쪽이 끊겨도 같은 `recording-event`로 알린다.
- Windows 무음 유지: `Os::current() == Os::Windows`일 때 같은 출력 장치에 무음을 쓰는 출력 스트림(`build_output_stream`, 콜백은 버퍼를 0으로 채우기만 함)을 시스템 스트림보다 먼저 시작하고 녹음 끝까지 유지한다. macOS에서는 만들지 않는다(A2). 분기는 `#[cfg]`가 아니라 `adapters/outbound/platform.rs`의 `Os` 값으로 한다: `docs/ARCHITECTURE.md:66-70`은 두 플랫폼에서 모두 컴파일되는 코드를 `Os`로 나누고 `#[cfg]`는 다른 플랫폼에서 컴파일할 수 없는 코드(Win32 FFI 등)에만 쓰도록 정한다. `build_output_stream`·`supports_input()`은 cpal 공통 API라 이 예외가 아니다. 이렇게 하면 Windows에서 돌리는 G4가 macOS 분기까지 컴파일·clippy로 검사한다.
- 시작 실패(장치 없음, 14.6 미만 등 cpal 오류, play 실패)는 `SYSTEM_AUDIO_UNAVAILABLE`로 바꾸고 메시지에 "macOS 14.6 이상이 필요하거나 시스템 오디오 녹음 권한이 없을 수 있습니다. 마이크만 녹음은 계속 쓸 수 있습니다." 같은 행동 안내를 넣는다. 이미 연 마이크 스트림·writer·`.part`·폴더를 모두 정리한다.
- 정지: 두 스트림을 drop(keepalive 포함) → 두 writer finish → 실패 확인 → `mix::mix_tracks(mic_part, system_part, final_part)` → `final_part`를 `<이름>.wav`로 rename → 중간 `.part` 두 개 삭제. 어느 단계에서 실패해도 남은 `.part`를 모두 지우고 오류를 돌려준다.
- 버리기: 두 writer cancel, 모든 `.part`와 폴더 삭제.

### 4. 합치기(`recording/mix.rs`, 새 파일, 순수 함수 + 파일 I/O)
- 입력: hound `WavReader` 둘. 출력: mono 16-bit, sample rate = 마이크 rate.
- 스트리밍: 4096 프레임 단위로 읽고 쓴다. 전체를 메모리에 올리지 않는다.
- 각 트랙을 채널 평균으로 mono화(시스템 트랙은 이미 mono로 저장돼 있고, 마이크 트랙은 장치 채널 그대로라 여기서 줄인다). 시스템 트랙은 마이크 rate로 선형 보간 리샘플링(스트리밍 상태: 소수 위치와 직전 샘플). 음성 전사 용도로 충분하고, worker가 어차피 16 kHz로 다시 변환한다.
- 합: `i32` 합을 `i16` 범위로 clamp(두 소리는 대개 겹치지 않으므로 평균으로 6 dB 깎지 않는다).
- 길이: 긴 쪽 기준, 짧은 쪽은 0으로 채운다. 시작 시점 차이(두 스트림 시작 간격 수 ms)는 보정하지 않는다.
- `RIFF_DATA_LIMIT` 초과 시 `WAV_TOO_LARGE`(기존 코드 재사용).
- 반환: 합친 프레임 수.

### 5. 프런트엔드
- 녹음 카드 `#recorder` 안, 녹음 버튼 아래에 `segmented-control` 하나(`radiogroup`, 라벨 `녹음 소스`): `마이크만` / `마이크 + 회의 소리`. 녹음 중·정지 중에는 disabled. 바꾸면 `saveRecordingSource` 즉시 저장, 실패 시 이전 값으로 되돌리고 `#app-error`에 알림.
- `마이크 + 회의 소리` 선택 시 안내 한 줄(`role="note"`가 아닌 일반 helper `<p>`, aria-describedby로 연결): "스피커 대신 헤드폰을 쓰면 상대 목소리가 두 번 녹음되지 않습니다. 회의 앱의 알림음 등 컴퓨터 소리도 함께 녹음됩니다."
- 녹음 버튼 부제(`시스템 마이크 · 16-bit PCM WAV`)는 소스에 맞춰 `시스템 마이크 · 16-bit PCM WAV` / `마이크와 회의 소리 · 16-bit PCM WAV`.
- 녹음 중 라벨 부제 `마이크 입력을 저장하고 있습니다.`도 소스에 맞게 바꾼다.
- 시작 시 `load_recording_source` 결과를 컨트롤에 반영(로그인 뒤 기존 설정 로드 흐름에 합류).
- `RecordingController.start(outputRoot)` → 현재 선택한 소스를 넘긴다.

### 6. 플랫폼 파일
- `src-tauri/Info.plist`에 `NSAudioCaptureUsageDescription`: "갈피가 화상회의 상대방의 목소리를 함께 녹음하려면 컴퓨터에서 재생되는 소리를 녹음할 권한이 필요합니다."

### 7. 문서
- `DESIGN.md` Recorder(또는 녹음 관련 절)에 소스 선택·안내·상태 문구, `README.md`/`README.en.md` 녹음 절(“현재는 선택한 마이크 입력만 녹음” 문구 교체, Windows·macOS 조건), `AGENTS.md`·`docs/ARCHITECTURE.md` 명령 수 40, `.issueops/testing/overview.md` 수동 Windows 게이트에 시스템 소리 녹음 항목, ADR 한 건(정지 뒤 호스트 합치기와 Windows 무음 유지 스트림, 기각한 대안).

## 재사용하는 기존 구현

- `recording/capture.rs` `build`/`enqueue`/`take_buffer`: 시스템 스트림도 같은 콜백 경로를 쓴다. 바꾸는 것은 에러 코드 주입뿐이다.
- `recording/writer.rs` `spawn`/`finish`/`cancel`/`take_recycled`: 시스템 트랙 writer로 한 번 더 쓴다. 바꾸는 것은 `downmix_from: Option<u16>` 옵션 하나(시스템 트랙만 mono로 저장)이고, 큐·재활용·RIFF 한도·flush 로직은 그대로다.
- `recording/cleanup.rs` `cancel_and_remove`/`remove_partial`/`remove_if_exists`/`with_cleanup`: 두 트랙과 최종 파일 정리에 재사용한다. 여러 경로를 받도록 확장이 필요하면 기존 함수를 늘리지 않고 호출을 반복한다.
- `recording/failure.rs` `SharedFailure`/`take_failure`: 두 스트림이 공유한다.
- `paths.rs` `recording_folder_name`: 회의 폴더·파일 이름 규칙 그대로.
- hound(이미 의존성): 합치기의 reader/writer. 리샘플링 crate(rubato 등)는 추가하지 않는다. 선형 보간 수십 줄이면 충분하고 의존성 추가는 `.issueops/TECH_STACK.md` 갱신 부담이 생긴다.
- 설정 저장은 `compute_device`의 `load_compute_device`/`save_compute_device` 패턴(`settings.rs:405`)을 그대로 따른다.
- 프런트 `segmented-control` 마크업·스타일과 `onComputeDeviceChange`(`app-view.ts:162`) 패턴.
- 새로 만드는 것: `domain/recording.rs`(값 객체 자리가 없음), `recording/mix.rs`(합치기는 기존에 없음).

## 성능 영향

- 콜백(hot path): 변경 없음. 시스템 스트림이 하나 더 생겨 writer 스레드 하나와 bounded queue 하나가 늘어난다(`QUEUE_SECONDS = 30` 기준 메모리: 48 kHz 8채널 i16 30초 ≈ 23 MB가 최악, 스테레오는 ≈ 5.8 MB). mono 다운믹스는 writer 스레드에서 청크당 O(프레임×채널)로 한다.
- 정지 시 합치기: O(n) 스트리밍 한 번, 메모리는 청크 크기(수십 KB)로 일정. 측정: 10분 길이 합성 트랙(마이크 48 kHz mono, 시스템 48 kHz mono 저장본)으로 release 빌드에서 합치기 시간을 재고, 60분을 선형 외삽한 값을 G5 EVIDENCE에 남긴다. 목표는 60분 녹음 기준 수 초대(정지 버튼 뒤 대기). 이 시간 동안 UI는 기존 `stopping` 상태(문구 "녹음 파일을 마무리합니다.")를 유지한다.
- 디스크: 녹음 중 중간 파일 두 개(마이크 + 시스템 mono)와 합친 파일이 정지 직후 잠깐 함께 존재한다(60분 48 kHz mono 기준 약 0.35 GB씩, 합계 약 1 GB). 합친 뒤 중간 파일은 지운다.
- `마이크만` 경로는 성능·파일 수 모두 불변.

## 하위 호환성과 side effect

- 설정 파일: `recordingSource` 키 추가(없으면 `microphone`). `LocalSettings`는 `#[serde(default, rename_all = "camelCase")]`이고 `deny_unknown_fields`가 없어(`settings.rs:285-287`, grep 결과 없음) 구버전도 이 키를 무시하고 읽는다.
- IPC: `start_recording`에 `source` 인자가 추가된다. 프런트·호스트는 한 번에 배포되므로 구버전 혼용은 없다. Zod 스키마 변경 없음(RecordingStatus/Result 모양 불변). 새 명령 2개.
- 녹음 결과 파일: `마이크만`은 경로·포맷 불변. `마이크 + 회의 소리`는 최종 파일이 mono이고 sample rate는 마이크 rate.
- Drive 업로드·전사·`MeetingKey::local` 규칙 불변(최종 경로가 같은 `<이름>.wav`).
- macOS 권한: 처음 시스템 소리 녹음 때 OS 권한 창이 뜬다. 거부하면 탭이 무음을 줄 수 있다(미확인, PR에 기록).
- Windows: 녹음 중 같은 출력 장치에 무음 출력 스트림이 하나 열린다(공유 모드). 끝나면 닫힌다.
- 롤백: 커밋 revert로 되돌릴 수 있다. 저장된 `recordingSource` 키는 구버전이 무시한다.
- LLM 프롬프트 변경 없음.

## 게이트(G1..Gn)

- G1: 녹음 소스 값 객체·설정 저장·명령이 동작한다 | CHECK: `cargo test --manifest-path src-tauri/Cargo.toml --all-targets recording_source` (Windows) | EXPECT: 새 테스트(직렬화 이름, 기본값, settings 저장·재로드, `is_empty` 반영) 통과
- G2: 합치기와 다운믹스가 정확하다 | CHECK: `cargo test --manifest-path src-tauri/Cargo.toml --lib adapters::outbound::recording` (Windows) | EXPECT: 서로 다른 rate(48k/44.1k), mono/stereo 마이크, 길이 불일치(짧은 쪽 0 채움), clamp, 빈 시스템 트랙 케이스, writer의 8채널 → mono 다운믹스와 `dropped_before` 프레임 환산 케이스 통과
- G3: 정지·버리기·시작 실패에서 `.part`가 남지 않고 `Os` 분기가 두 플랫폼 모두 컴파일된다 | CHECK: `cargo test --manifest-path src-tauri/Cargo.toml --lib adapters::outbound::recording` (Windows), `grep -rn "cfg(windows)\|cfg(target_os" src-tauri/src/adapters/outbound/recording/mod.rs src-tauri/src/adapters/outbound/recording/capture.rs src-tauri/src/adapters/outbound/recording/mix.rs` | EXPECT: 정리 경로 테스트 통과(장치 없는 경로는 함수 단위로 검증), grep 결과 없음
- G4: 전체 Rust 게이트 | CHECK: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` (Windows) | EXPECT: 모두 통과
- G5: 합치기 성능 측정 | CHECK: 10분 합성 트랙 release 측정(일회성 측정 코드는 커밋하지 않음) | EXPECT: 측정값과 60분 외삽값 기록
- G6: Windows 실제 녹음 | CHECK: 설치한 앱에서 `마이크 + 회의 소리`로 1분 이상 녹음(중간 20초는 재생 없이 무음), 브라우저 음원 재생 + 마이크 말소리 | EXPECT: 최종 WAV 길이가 녹음 시간과 1초 이내로 같고, 재생 소리와 말소리가 모두 들리며, 그 파일 전사가 끝까지 된다. 녹음 중 다른 앱 재생 음량 변화 없음(A3)
- G7: 프런트 | CHECK: `bun run check`, `bun test` | EXPECT: 통과, 소스 토글 저장·실패 되돌림·녹음 중 disabled·startRecording 인자 DOM 테스트 포함
- G8: 문서·플랫폼 펜스 | CHECK: `python3 scripts/verify-platform-fence.py`, `python3 scripts/verify-docs-platform.py`, `grep -n NSAudioCaptureUsageDescription src-tauri/Info.plist` | EXPECT: 통과, plist 키 존재
- G9: 버리기 수동 확인 | CHECK: Windows 앱에서 `마이크 + 회의 소리` 녹음 후 버리기 | EXPECT: 회의 폴더와 `.part` 파일이 남지 않음

## 검증 명령(게이트 밖)

- worker는 바뀌지 않으므로 worker 테스트는 회귀 확인으로 한 번만: `PYTHONPATH=. python3 -m unittest discover -s worker/tests -t .`
- Rust는 WSL에서 빌드되지 않는다(`recording/power` SleepBlocker가 Linux 미지원). Rust 게이트는 Windows 체크아웃에서 돌린다: 워크트리 파일을 Windows 쪽 같은 브랜치 워크트리로 동기화하거나, Windows에서 원격 브랜치를 받아 실행한다. 어느 방법이든 실행한 커밋 SHA를 EVIDENCE에 남긴다.

## 작업 순서

1. domain 값 객체 + 설정 포트·어댑터 + 명령(테스트 먼저: 직렬화·기본값·저장 재로드).
2. `mix.rs`(테스트 먼저: 합성 WAV 케이스).
3. 녹음기 두 트랙·keepalive·정리 경로, `capture::build` 에러 코드 주입.
4. use case·Tauri 명령·composition 등록, 기존 테스트의 `start_recording` 호출 4곳 갱신.
5. 프런트 계약·Zod·controller·view·template·styles, DOM 테스트.
6. Info.plist, 문서, ADR.
7. Windows에서 G4·G6·G9, 성능 측정 G5.
