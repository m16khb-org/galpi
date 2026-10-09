# Gates: 10

- [x] G1: 공유 모듈 이동 뒤 ChatGPT 로그인 테스트가 그대로 통과한다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets chatgpt
  EXPECT: /test result: ok\. [1-9]\d* passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-3a6bea8a51165c32)
- [x] G2: 접근 게이트 application 테스트 (a)~(i)
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets gateway
  EXPECT: /test result: ok\. [1-9]\d* passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-3a6bea8a51165c32)
- [x] G3: gateway 어댑터·설정 store 테스트
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets adapters::outbound
  EXPECT: /test result: ok\. [1-9]\d* passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-3a6bea8a51165c32)
- [x] G4: Rust 서식
  CHECK: python3 -c 'import subprocess,sys;r=subprocess.run(["cargo", "fmt", "--manifest-path", "src-tauri/Cargo.toml", "--check"]);print("FMT_OK" if r.returncode==0 else "FMT_FAIL");sys.exit(r.returncode)'
  EXPECT: FMT_OK
  EVIDENCE: FMT_OK
- [x] G4b: Clippy
  CHECK: python3 -c 'import subprocess,sys;r=subprocess.run(["cargo", "clippy", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings"]);print("CLIPPY_OK" if r.returncode==0 else "CLIPPY_FAIL");sys.exit(r.returncode)'
  EXPECT: CLIPPY_OK
  EVIDENCE: Compiling galpi v0.1.0 (/Users/m16khb/Workspace/galpi.worktrees/10-require-gateway-google-login/src-tauri) | Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.09s
- [x] G4c: Rust 전체 테스트
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets
  EXPECT: /test result: ok\. [1-9]\d* passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-3a6bea8a51165c32)
- [x] G5: 아키텍처 검사·Biome·tsc
  CHECK: python3 -c 'import subprocess,sys;r=subprocess.run(["bun", "run", "check"]);print("CHECK_OK" if r.returncode==0 else "CHECK_FAIL");sys.exit(r.returncode)'
  EXPECT: CHECK_OK
  EVIDENCE: 138 138 │     } | 139 139 │
- [x] G5b: 프런트 reducer·DOM·Zod 테스트
  CHECK: bun test
  EXPECT: /(?m)^\s*0 fail$/
  EVIDENCE: 549 expect() calls | Ran 201 tests across 27 files. [853.00ms]
- [x] G6: 문서 플랫폼 문구 검사
  CHECK: python3 scripts/verify-docs-platform.py
  EXPECT: PLATFORM_DOCS_OK
  EVIDENCE: PLATFORM_DOCS_OK
- [x] G7: 명령 28·포트 15와 문서 숫자 일치
  CHECK: python3 -c 'import re,sys,pathlib as P;t=P.Path("src-tauri/src/adapters/inbound/tauri.rs").read_text().count("#[tauri::command]");q=len(re.findall(r"^pub trait",P.Path("src-tauri/src/application/ports.rs").read_text(),re.M));pats=["24 frontend","24 command","24 IPC","24 Tauri","24 커맨드","24개","24 `#[tauri","13 Rust traits","13 ports","13개 포트","13개 trait","포트 13개"];fs=["AGENTS.md","src-tauri/AGENTS.md","docs/ARCHITECTURE.md",".issueops/conventions/overview.md",".issueops/architecture/overview.md",".issueops/OPEN_API_SPEC.md"];left=[(f,x) for f in fs for x in pats if x in P.Path(f).read_text()];ok=t==28 and q==15 and not left;print("COUNTS_OK" if ok else ("COUNTS_FAIL",t,q,left));sys.exit(0 if ok else 1)'
  EXPECT: COUNTS_OK
  EVIDENCE: COUNTS_OK
- [x] G8: IPC로 나가는 AppAccess JSON에 토큰이 없다
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets app_access_serializes_only_state_and_email
  EXPECT: /test result: ok\. 1 passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-3a6bea8a51165c32)
