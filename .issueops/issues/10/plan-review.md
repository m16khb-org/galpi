# 계획 검토 기록

## 1차: 수정 요청

- 1차 리뷰(독립 reviewer 서브에이전트, anthropic/claude-opus-5-5 high)는 수정 요청을 냈다. 설정 파일 is_empty와 macOS 설정 파일 비밀 경로를 빠뜨려 재시작 후 세션이 사라질 수 있고, domain이 AppError에 의존하며, refresh 거부 판정이 invalid_grant보다 넓고, 접근 상태 확인·로그아웃 실패 전이가 없고, gateway가 Supabase 장애를 invalid_grant로 돌려준다는 5건과 문서 목록 누락 1건이다. 이 기록은 반영한 개정본에 묶여 있다.

## 2차: 수정 요청

- 2차 delta 리뷰(독립 reviewer 서브에이전트, anthropic/claude-opus-5-5 high)는 1차 결함 1·2·3·5가 해소됐다고 확인했다. 네이티브 구독 실패 경로에서 로그인 화면이 확인 중에 멈추는 문제와 G7 정규식이 한국어 포트 수 표기를 놓치는 문제 2건으로 수정을 요청했다.

## 3차: 수정 요청

- 3차 delta 리뷰(상향 모델 anthropic/claude-opus-5-5 xhigh, omp -p 빈 컨텍스트)는 2차 지적 두 건이 해소됐다고 확인했다. 런타임 연결 실패 상태의 로그인 버튼 표시 규칙이 계획 안에서 서로 다르고, G7이 .issueops/OPEN_API_SPEC.md의 IPC 명령 수를 놓친다는 2건으로 수정을 요청했다.

## 4차: 수정 요청

- 4차 delta 리뷰(상향 모델 anthropic/claude-opus-5-5 xhigh)는 3차 결함 두 건이 해소됐다고 확인했다. 로그인 버튼 규칙이 진행 중 상태(signing-in·signing-out)를 제외하지 않아 전이표와 어긋나고, 로그아웃이 진행 중인 로그인을 먼저 취소하지 않아 상태 경쟁이 생길 수 있다는 1건으로 수정을 요청했다.

## 5차: 수정 요청

생략: 리뷰 5라운드 상한에 도달했다. 마지막 결함은 리뷰어가 제시한 최소 수정 그대로 반영했다: cancel_sign_in은 ChatGPT(sign_in.rs:48-65)처럼 oneshot 신호만 보내고 상태·저장소를 바꾸지 않으며, 상태 변경·revoke·clear는 sign_out에만 둔다. 이를 고정하는 application 테스트 (i)(저장 세션이 있는 상태에서 로그인 취소 → revoke 0회, 세션 유지, CANCELLED)를 G2에 추가했다. 같은 개정에서 게이트를 gates-ledger 형식(argv 명령 하나, 셸 연결자 없음)으로 바꾸고 G7 래퍼를 구현 전 저장소에서 실행해 COUNTS_FAIL(24, 13)로 판정되는 것을 확인했다.

- 5차 delta 리뷰(상향 모델 anthropic/claude-opus-5-5 xhigh)는 4차 결함(버튼 허용 목록, 로그아웃 전 로그인 취소)이 해소됐다고 확인했다. 남은 결함은 cancel_sign_in이 저장 세션을 지우는 동작으로 정의된 모순 하나다.
