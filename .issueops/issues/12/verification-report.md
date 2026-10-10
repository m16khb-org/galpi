# galpi#12 구현 검증 보고

- Lifecycle: `io-21fd3b1e1f06`, generation 2, branch `12-google-drive-sync`, base `4c24a2479fc4bf294ffeecc81fd06ea7438894fe`
- 계획: `.issueops/issues/12/plan.md` (sha256 `50c8de3550ebb0db6083592620d5c73c88224cbb8f6e01d1469fb421a07fcacb`)
- 게이트 원장: `.issueops/issues/12/gates.md` (G1~G12 충족)

## 게이트 결과

| 게이트 | 명령 | 결과 |
|---|---|---|
| G1 도메인 | `cargo test --lib domain::drive` | 12개 통과(충돌 판정 5경우, 파일 ID 거부, 회의 키 한도, FNV-1a 벡터, 토큰 Debug 가림) |
| G2 gateway bearer | `cargo test --lib application::tests::gateway` | 16개 통과(새 bearer 5경우 포함) |
| G3 Drive 저장소 | `cargo test --lib settings::drive` | 5개 통과 |
| G4 Drive 어댑터 | `cargo test --lib adapters::outbound::drive` | 23개 통과 |
| G5 Drive use case | `cargo test --lib application::tests::drive` | 25개 통과 |
| G6 Rust 전체 | `cargo test --all-targets` | 295 통과, 1 무시(기존) |
| G7 clippy | `cargo clippy --all-targets -- -D warnings` | 경고 없음 |
| G8 fmt | `cargo fmt --check` | 통과 |
| G9 프런트 정적 | `bun run check` | 통과 |
| G10 프런트 테스트 | `bun test` | 233 통과 |
| G11 비밀번호 없음 | `git grep --untracked -E` (Google 클라이언트 비밀번호 모양: 접두어-20자 이상) | 일치 없음. 계획의 접두어 단독 검색은 추적 파일만 보고 계획 문서의 설명 문구에도 걸려, 미추적 파일까지 보고 실제 값 모양만 찾도록 바꿨다(가짜 값 탐침 파일로 탐지 확인 후 삭제) |
| G12 실제 스모크 | `.issueops/evidence/task-12-drive-smoke.md` | 실제 Google·auth-gateway·Drive로 연결→Picker→업로드→Unchanged→Updated→Conflict→덮어쓰기→회의록→해제 확인 |

## 계획과 다르게 구현한 점

1. `Artifacts.source_transcript` 추가: 전사문 가져오기의 회의 키(정경화한 원본 경로)를 application이 파일 시스템 없이 얻도록, 가져오기 어댑터가 이미 정경화한 입력 경로를 산출물에 실었다.
2. 회의 키 해시를 SHA-256 대신 FNV-1a 64로 했다: `docs/ARCHITECTURE.md` §1은 Rust domain 의존을 std·serde·thiserror·uuid로 제한한다. 업로드 내용의 SHA-256(`galpiSha256`, Drive `sha256Checksum` 비교)은 어댑터(`rest.rs`)가 계산한다.
3. `DriveFilesPort`에 `read_local`을 두었다: 충돌 판정에 필요한 로컬 체크섬을 application이 파일을 읽지 않고 얻기 위해서다. 5 MiB 초과 거부도 여기서 한다.
4. `download`의 저장 위치는 어댑터가 정한다(`AppPaths.cache/drive-imports/<uuid>/`): application은 `AppPaths`를 모른다.
5. multipart 경계 문자열은 `random_urlsafe` 대신 UUID v4를 쓴다(실패 경로가 없다).
6. 프런트엔드 `openDriveLink(url)`를 `BackendPort`에 추가했다: 동기화 행의 `Drive 폴더 열기`가 opener를 쓰려면 필요하다. `https://drive.google.com/`이 아닌 주소는 거부한다.
7. `AppController.begin()`에서 `setBusy(kind)`를 `renderJob()`보다 먼저 부른다: Drive 가져오기 시작 시 진행 카드 제목이 이전 작업 종류로 그려지는 결함을 DOM 테스트가 잡았다. 6차 계획 리뷰의 비차단 메모 2(다음 전사 시작 직후 제목이 잠시 틀림)도 같은 원인이라 함께 해소된다.
8. Drive 아이콘은 Phosphor의 Drive 로고 대신 일반 `cloud` 아이콘을 쓴다: ChatGPT 패널과 같이 브랜드 자산을 쓰지 않는다.
9. 계획이 `controller.test.ts`에 두려던 가져오기 흐름 검증은 `src/ui/drive.dom.test.ts`에 모았다(스타일 주입, 같은 단언).
10. gateway가 Drive 토큰 요청의 bearer를 401로 거절하면 호스트도 세션을 닫는다(`AppAccessGate::expire`). 창은 `AUTH_REQUIRED`를 받으면 로그인 화면으로 돌아가므로, 호스트가 `SignedIn`으로 남아 기능 명령을 계속 받는 불일치를 없앴다.
11. `bearer()`의 `GATEWAY_UNAVAILABLE` 문구는 Drive를 언급하지 않는 범용 문구("로그인 서버에 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요.")로 했다.

## ai-slop-clean

읽기 전용 검토 에이전트가 diff에서 찾은 후보를 분류해 반영했다.

- 결함: 새 회의 동기화 중 이전 회의의 파일 줄과 동작하지 않는 `Drive 폴더 열기`가 남던 문제(`drive-view.ts`), 상태를 읽기 전 설정 시트에 동작하지 않는 `Drive 연결`이 보이던 문제(템플릿 `hidden`), gateway 401 뒤 호스트 세션 불일치(10번). 앞의 두 건은 DOM 회귀 테스트 2개로 고정했다.
- 중복: 동의→bearer→교환 흐름을 `DriveAccounts::consent`로, 401 복구 분기 5벌을 `recovered`/`after_refused_token`으로, `auth_required`를 gateway 모듈 하나로, `STATE_ERROR` 생성을 `state_error()`로 묶었다. `sha256_hex`는 크레이트의 기존 `format!("{:x}", …)` 관용구를 쓴다.
- 쓰지 않는 코드: `DriveFileId`의 `Hash`, `tokens_from`의 도달할 수 없는 `ok_or`, 추론되는 `LocalUpload` 표기, 내보낼 필요 없는 `DRIVE_CONFLICT_MESSAGE` export, 이름만 바꾸던 `DriveImported` 타입, 동기화 루프의 도달할 수 없는 오류 코드 목록(`DRIVE_UNAVAILABLE`만 남기고 테스트 추가).
- 약한 산출물·근거 없는 주장: 모듈 문서의 "모든 저장소 쓰기" 범위, `rest.rs`의 "Drive id 로컬 기록 불필요" 문장, refresh token을 "보장"한다는 주석, 테스트 구획 주석, 동작과 다른 테스트 이름을 고쳤다. REST 어댑터의 `DRIVE_UNAVAILABLE` 문구에서 로그인 서버 언급을 뺐다.
- 함께 고친 결함 가능성: 다운로드 첫 응답에도 60초 제한(`CHUNK_TIMEOUT`)을 걸었다.
- 의도적으로 유지: `DriveImport.jobId`와 파일별 `webViewLink`(계획의 IPC 계약), 비슷한 설정을 가진 두 취소 테스트(Picker 대기와 다운로드라는 서로 다른 지점), 테스트 모듈마다 있는 `lock`/`failure` 도우미(기존 관례).

정리 전후 측정(같은 스크립트, 변경·새 파일의 `.rs`/`.ts`/`.css` 추가 줄, 셸 휴리스틱):

| 지표 | 정리 전 | 정리 후 |
|---|---|---|
| SNR(신호/전체 비어 있지 않은 줄) | 0.920 (5,638 / 6,131) | 0.916 (5,698 / 6,219) |
| 같은 파일 7줄 이상 동일 블록 | 4 | 2(두 취소 테스트, 유지) |
| 테스트 제외 250줄 초과 새 파일 | 4 | 4(`rest.rs` 749, `application/drive.rs` 533, `domain/drive.rs` 440, `settings/drive.rs` 278; 셋은 인라인 테스트 포함) |

SNR이 0.004 낮아진 것은 회귀 테스트 2개·application 테스트 2개와 설명 주석이 늘었기 때문이다. 기준선(0.60)과 목표(0.75)를 모두 넘는다.

## 비차단 메모 반영

- 메모 1: 가져오기 성공 시 `view.setAudio(path)`를 `renderDriveImported(name)` 앞에서 부른다(`controller.ts` DriveHooks.onImported).
- 메모 2: 위 7번으로 해소. `setBusy(null)`에서 `jobKind`를 비우지 않았다(취소 제목 보존).

## UI 판단

- 디자인 시스템: 새 스타일은 기존 SEED 토큰만 쓴다(`src/styles.test.ts` 통과, 새 hex·px·커스텀 속성 없음). 버튼은 `actionButtonClass` recipe, 상태는 색과 문구를 함께 쓴다.
- 접근성: 상태 문구는 모두 `role="status"` live region, 설정 시트가 배경을 inert로 만들기 때문에 취소 버튼 id를 카드·설정으로 나눴다. 가져오기 버튼은 다른 시작 동작과 같이 작업·녹음 중 비활성.
- 실제 화면: Vite 개발 서버 + 가짜 `__TAURI_INTERNALS__`로 온보딩 카드, 연결 대기(취소만 보임), 연결 후 칩·가져오기 버튼, 가져오기 진행 카드(`작업 취소`가 `elementsFromPoint` 최상단), 가져오기 완료(카드 숨김, 파일 이름 표시, "전사 준비됨"), 동기화 충돌 행(덮어쓰기 버튼), 설정 Drive 절(연결 해제 최상단)을 1440×1000에서 확인했다. 설정 도움말 문장 글꼴이 다른 도움말보다 큰 문제를 발견해 `#drive-settings-help`로 맞췄다.
- 반응형·모션: 새 요소는 기존 그리드·flex-wrap 규칙을 따르며 애니메이션을 추가하지 않았다.

## side effect

- 새 IPC 명령 7개, `settings.json`의 Drive 필드 6개(`serde(default)`), Windows Credential Manager `com.m16khb.galpi:drive-session`.
- 로컬: `AppPaths.cache/drive-imports/<uuid>/`(실패·취소 시 즉시 삭제, 앱 시작 시 전체 삭제).
- 원격(사용자 Drive): `Galpi` 폴더, 회의별 폴더, 결과 파일 최대 3개. 원본은 읽기만 한다.
- opener 허용 목록에 `https://drive.google.com/*`.
- 스모크가 만든 Drive 파일·폴더는 휴지통으로 옮겼고, Drive·gateway 토큰은 폐기했다.

## 성능

- 전사 핫패스(워커, JSONL 이벤트, CPAL 녹음)는 바꾸지 않았다. Drive 업로드는 전사 명령이 끝난 뒤 별도 IPC로 실행된다.
- 다운로드는 `Response::chunk()`를 `.part` 파일에 바로 쓴다. 첫 응답과 각 청크 사이 대기는 60초로 제한한다.
- 동기화 한 번의 Drive 왕복: 루트 확인 1~2회, 회의 폴더 찾기 1회(없으면 생성 1회), 파일마다 찾기 1회 + 생성·갱신 1회(변경 없으면 0회). 실제 스모크에서 결과 2~3개 동기화는 1~4초 걸렸다(증거 파일 시각).
- 토큰 갱신은 Drive 만료 5분 전, gateway 만료 60초 전부터만 일어난다.
- 측정하지 않은 것: 50MB 이상 다운로드 중 메모리 증가(아래 남은 확인).

## 검증 단계 피드백 반영

구현 리뷰(pass, 비차단 4건)와 화면 QA(aside-web-qa, "Ready with minor fixes", 기능 결함 없음, F1~F20 Pass)의 지적 중 사용자에게 보이는 것을 고치고 다시 검증했다.

| 출처 | 지적 | 수정 | 회귀 확인 |
|---|---|---|---|
| 리뷰 1 | Picker 대기 중 다른 계정으로 다시 연결하면 A 토큰이 B 연결에 저장됨 | `pick()`이 저장 잠금 안에서 현재 연결의 `permission_id`까지 대조, 다르면 새 토큰 철회 후 `DRIVE_NOT_CONNECTED` | `reconnecting_another_account_during_a_picker_consent_keeps_its_tokens` |
| 리뷰 2 | 사용자가 직접 연결 해제한 뒤의 `DRIVE_NOT_CONNECTED`가 온보딩 카드를 오류로 다시 띄움 | 창이 Drive 연결 상태일 때만 `reconnectRequired` | `a sync ended by the user's own disconnect does not bring the card back` |
| 리뷰 3 | 브라우저에서 동의 거부·Picker 닫기가 실패 카드로 표시됨 | `CANCELLED`는 `cancelJob`으로 취소 카드 | `an import declined in the browser cancels the card instead of failing it` |
| 리뷰 4 | `.drive-status:empty { display: none }`이 live region을 접근성 트리에서 뺌 | 빈 줄은 여백만 0 | Chromium에서 빈 `#drive-onboarding-status`의 계산된 `display: block` 확인 |
| VQA-1 | 긴 Drive 이메일이 1024px 상단 바를 넘겨 설정 버튼이 화면 밖으로 밀림 | Drive 칩 최대 220px, 말줄임, 전체 주소는 `title` | Chromium 1024px, 73자 이메일: `scrollWidth` 1024, 설정 버튼 오른쪽 984 |
| VQA-3 | `Drive 연결` 등이 숨으면 포커스가 body로 떨어짐 | 숨은 Drive 버튼의 포커스를 대신 나타난 보이고 활성인 컨트롤로 이동(`FOCUS_HANDOFF`), 작업 중 시작 동작이 비활성이면 설정 버튼으로, 가져오기 버튼 표시를 Drive 렌더보다 먼저 갱신 | DOM 테스트 2개(연결→취소→가져오기 버튼, 전사 중 `나중에`→설정 버튼) + Chromium 키보드 Enter로 연결 순서 확인 |

수정분 delta 리뷰: pass. 지적 1건(작업 중 비활성 후보로 포커스가 떨어짐, 비차단)을 위 VQA-3 행의 설정 버튼 대체로 반영했다.

고치지 않은 낮은 우선순위 QA 지적(후속 후보): VQA-2 재연결 요구 때 동기화 행에 연결 버튼이 없음, VQA-4 1024px 온보딩 제목 줄바꿈, VQA-5 계정 칩과 Drive 칩 모양이 같음. 화면 QA의 Not Run: 실행 중 `prefers-reduced-motion` 전환(새 애니메이션 없음을 소스와 계산된 스타일로만 확인), 새로고침 뒤 유지, 네이티브 동작.

## 남은 확인

- 50MB 이상 다운로드 중 메모리 측정: 실행하지 않음(증거 파일 참고).
- 실제 앱 창(`bun run dev`)에서의 전 과정: 이 컴퓨터에 엔진·모델·HF 토큰이 없어 실행하지 않음.
- auth-gateway#10 PR #11은 아직 열려 있으나 엔드포인트는 배포되어 응답한다.
