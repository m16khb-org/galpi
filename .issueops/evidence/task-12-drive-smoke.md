# galpi#12 Drive 스모크 (2026-10-10, 실제 Google·auth-gateway·Drive)

- 실행 방식: 운영 어댑터(`GatewayOAuthAdapter`, `DriveOAuthAdapter`, `DriveRestAdapter`)와 운영 use case(`AppAccessGate`, `DriveAccounts`)를 그대로 쓰는 임시 하네스(`cargo test --lib drive::smoke -- --ignored --nocapture`, 실행 후 삭제). 시스템 기본 브라우저(Aside)에서 실제 Google 동의·Picker를 조작했다.
- 앱 화면(`bun run dev`)으로 실행하지 않은 이유: 이 컴퓨터에는 Galpi 앱 데이터(엔진, 모델 약 3GB, gateway 세션)와 사용자 Hugging Face 토큰이 없어 전사 단계를 돌릴 수 없다. 화면 배선은 `src/ui/drive.dom.test.ts`(14개)와 Vite + 가짜 IPC 화면 확인으로 검증했다.
- 전제: auth-gateway `POST /drive/native/token` 배포 확인(bearer 없이 401 `missing bearer token`, 다른 경로는 404). auth-gateway#10 이슈·PR #11은 아직 열림 상태다.
- 저장소: 하네스 메모리 저장소. 사용자 설정 파일은 건드리지 않았다. 스모크가 만든 Drive 파일·폴더는 마지막에 휴지통으로 옮겼다.

| 시각(UTC) | 단계 | 결과 |
|---|---|---|
| 17:39:12 → 17:39:56 | gateway Google 로그인 | `SignedIn { email: m16khb@gmail.com, offline: false }` |
| 17:39:56 → 17:40:45 | Drive 연결(`drive.file` 동의, "확인되지 않은 앱" 경고 없음) → gateway 대행 교환 → about → 폴더 | `Connected`, `Galpi` 폴더 생성 |
| 17:40:46 | 폴더 ID 없이 다시 찾기 | 같은 `Galpi` 폴더(`galpiRoot` appProperties 조회) |
| 17:40:48 → 17:44:32 | Picker 가져오기(`trigger_onepick`, `login_hint`, `mimetypes`) | 고른 ID가 업로드한 파일과 일치, 32,044바이트 다운로드, 로컬 수정 시각 17:40:47Z = Drive `modifiedTime` |
| 17:44:38 | 동기화 1(처음) | `Srt=Uploaded, SpeakerText=Uploaded` |
| 17:44:39 | 업로드 직후 Drive `sha256Checksum` | 있음, `galpiSha256`과 일치 |
| 17:44:40 | 동기화 2(같은 내용) | `Srt=Unchanged, SpeakerText=Unchanged` |
| 17:44:44 | 동기화 3(새 로컬 출력 폴더 `smoke meeting 2`로 재전사, 텍스트 변경) | `Srt=Unchanged, SpeakerText=Updated`, 같은 Drive 회의 폴더, 같은 파일 ID |
| 17:44:47 | 동기화 4(Drive에서 직접 수정 + 로컬 수정) | `SpeakerText=Conflict`(덮어쓰지 않음) |
| 17:44:50 | 동기화 5(덮어쓰기) | `SpeakerText=Updated` |
| 17:44:53 | 동기화 6(증강 뒤 같은 회의) | `Minutes=Uploaded`, 폴더 링크 있음 |
| 17:44:54 → 17:44:56 | 정리 | 회의 폴더·스모크 녹음·`Galpi` 폴더 휴지통 이동(200) |
| 17:44:57 | 연결 해제 | `Disconnected`, 저장소 비움, 이전 refresh token으로 갱신 시 `Rejected`(Google 폐기 확인) |
| 17:44:58 | gateway 로그아웃 | 완료 |

## 관찰과 남은 확인

- Picker 동의 첫 화면에서 Google이 한 번 500을 돌려줬고 새로고침으로 진행됐다. 같은 파라미터 조합(기본, `login_hint`, `mimetypes`, 둘 다)을 별도 redirect로 다시 열었을 때는 500이 재현되지 않아 Google 쪽 일시 오류로 판단했다. 앱은 이 경우 사용자가 브라우저에서 새로고침하거나 취소 후 다시 시도하면 된다(대기 5분).
- 50MB 이상 녹음 다운로드 중 메모리 측정(계획의 성능 영향 절)은 하지 않았다. 스모크 파일은 32KB였다. 다운로드는 `Response::chunk()`를 파일에 바로 쓰는 구조이며 `downloads_land_in_a_fresh_folder_with_the_drive_modification_time` 어댑터 테스트가 `.part` 경유 저장을 고정한다.
- 다른 계정으로 고른 파일 거부(`DRIVE_ACCOUNT_MISMATCH`)와 실제 전사 뒤 자동 업로드는 실계정·엔진이 없어 application 테스트로만 확인했다.
