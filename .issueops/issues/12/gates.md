# Gates: 12

- [x] G1: Drive 도메인 규칙(충돌 판정 다섯 경우, 파일 ID 거부, 회의 키 한도, 토큰 Debug 가림)이 고정된다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib domain::drive
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 7.06s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
- [x] G2: 로그인 게이트 bearer()가 만료 전 재사용, 만료 구간 갱신과 회전 저장, 거부 시 세션 삭제, 미도달 시 세션 유지, 토큰 없음 시 AUTH_REQUIRED로 동작한다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib application::tests::gateway
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.18s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
- [x] G3: Drive 연결 정보와 토큰이 재시작 뒤에도 남고 OS 비밀 저장소 백엔드에서는 설정 파일에 토큰이 없다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib settings::drive
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.14s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
- [x] G4: Drive 인증·REST 어댑터가 가짜 서버 기준 gateway 대행 교환, 오류 매핑, multipart 업로드, 스트리밍 다운로드 계약을 지킨다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib adapters::outbound::drive
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.16s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
- [x] G5: Drive use case(로그인 게이트, 연결·해제, 가져오기 계정 확인과 취소, 동기화 생성·갱신·충돌·실패 격리)가 동작한다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib application::tests::drive
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.18s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
- [x] G6: Rust 전체 테스트가 실패 없이 통과한다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-3a6bea8a51165c32)
- [x] G7: Rust 린트가 경고 없이 통과한다
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"cargo\",\"clippy\",\"--manifest-path\",\"src-tauri/Cargo.toml\",\"--all-targets\",\"--\",\"-D\",\"warnings\"],capture_output=True);print(\"CLIPPY_OK\" if r.returncode==0 else \"CLIPPY_FAIL\");sys.exit(r.returncode)"
  EXPECT: CLIPPY_OK
  EVIDENCE: CLIPPY_OK
- [x] G8: Rust 형식이 맞다
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"cargo\",\"fmt\",\"--manifest-path\",\"src-tauri/Cargo.toml\",\"--check\"],capture_output=True);print(\"FMT_OK\" if r.returncode==0 else \"FMT_FAIL\");sys.exit(r.returncode)"
  EXPECT: FMT_OK
  EVIDENCE: FMT_OK
- [x] G9: 프런트 아키텍처 펜스, Biome, tsc가 통과한다
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"bun\",\"run\",\"check\"],capture_output=True);print(\"CHECK_OK\" if r.returncode==0 else \"CHECK_FAIL\");sys.exit(r.returncode)"
  EXPECT: CHECK_OK
  EVIDENCE: CHECK_OK
- [x] G10: 프런트 테스트가 실패 없이 통과한다
  CHECK: bun test
  EXPECT: /^\s*0 fail$/m
  EVIDENCE: 665 expect() calls | Ran 233 tests across 29 files. [2.11s]
- [x] G11: 소스에 클라이언트 보안 비밀번호가 없다
  CHECK: python3 -c "import subprocess,sys;p=chr(71)+'OCSPX-[A-Za-z0-9_-]{20,}';r=subprocess.run(['git','grep','--untracked','-nE',p],capture_output=True);print('SOURCE_CLEAN' if r.returncode==1 else 'SOURCE_LEAK');sys.exit(0 if r.returncode==1 else 1)"
  EXPECT: SOURCE_CLEAN
  EVIDENCE: SOURCE_CLEAN
- [x] G12: 실제 Drive 스모크(연결, 가져오기, 업로드, 재처리, 충돌, 해제) 결과가 증거 파일에 기록된다
  CHECK: python3 -c "import pathlib;print(pathlib.Path(\".issueops/evidence/task-12-drive-smoke.md\").read_text().splitlines()[0])"
  EXPECT: /Drive 스모크/
  EVIDENCE: # galpi#12 Drive 스모크 (2026-10-10, 실제 Google·auth-gateway·Drive)
- [x] G13: 내려받은 Drive 파일 이름이 OS 경로 해석 없이 정리되어 Windows와 macOS에서 같은 결과를 낸다(`/`·`\` 구분자, 드라이브 접두어, 예약 문자)
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 7.07s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib downloaded_names_are_sanitized_and_keep_the_extension
  EXPECT: /test result: ok\. 1 passed; 0 failed/
