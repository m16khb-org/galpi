# Gates: 3

- [x] G2: A1 사이드카 타깃 테이블·Windows uv 해시·빌드 분기·Rust/TS 상수 일치
  CHECK: bun test scripts/sidecar-targets.test.ts scripts/build.test.ts
  EXPECT: /^\s*0 fail$/m
  EVIDENCE: 21 expect() calls | Ran 8 tests across 2 files. [572.00ms]
- [x] G3a: A1/A2 Windows CPU 락이 같은 인자로 재생성해도 바이트 동일
  CHECK: python3 scripts/verify-windows-lock.py cpu
  EXPECT: LOCK_REPRODUCIBLE cpu
  EVIDENCE: LOCK_REPRODUCIBLE cpu
- [x] G3b: A2 Windows CUDA 락 재현
  CHECK: python3 scripts/verify-windows-lock.py cuda
  EXPECT: LOCK_REPRODUCIBLE cuda
  EVIDENCE: LOCK_REPRODUCIBLE cuda
- [x] G4: A2 CUDA 락이 torch==2.8.0+cu128 고정
  CHECK: grep -n ^torch==2.8.0+cu128 worker/requirements-windows-cuda.lock
  EXPECT: /^\d+:torch==2\.8\.0\+cu128 /m
  EVIDENCE: 2487:torch==2.8.0+cu128 \
- [x] G7a: A3 녹음 취소가 .wav.part와 빈 폴더를 남기지 않음
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets cancelling_a_recording_leaves_no_partial_file_or_empty_folder
  EXPECT: /test result: ok\. 1 passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-c9433e84552b46b3) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-7851c21d36129273)
- [x] G9a: A5 비밀 저장소 순수부(credential_target, 블롭 인코딩·한도) 통과
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets credential
  EXPECT: /test result: ok\. [1-9]\d* passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-c9433e84552b46b3) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-7851c21d36129273)
- [x] G9b: A5 재시작 후 저장됨 유지와 평문 비저장 settings 테스트 통과
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets settings::
  EXPECT: /test result: ok\. [1-9]\d* passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-c9433e84552b46b3) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-7851c21d36129273)
- [x] G10a: A6 Windows에서 Qwen3 숨김, CPU/CUDA 선택, 드라이버 없으면 CUDA 비활성
  CHECK: bun test src/ui/app-view.dom.test.ts src/ui/controller.test.ts src/adapters/tauri-backend.test.ts src/ui/settings-autosave.dom.test.ts
  EXPECT: /^\s*0 fail$/m
  EVIDENCE: 169 expect() calls | Ran 45 tests across 4 files. [779.00ms]
- [x] G10b: A6 설정 어댑터가 Os::Windows에서 Qwen3 거부·WhisperX 기본
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets os_windows_
  EXPECT: /test result: ok\. [1-9]\d* passed/
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-c9433e84552b46b3) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-7851c21d36129273)
- [x] G11: A7 macOS check:all 통과
  CHECK: python3 -c "import subprocess,sys; r=subprocess.run(['bun','run','check:all'],capture_output=True); print('CHECK_ALL_PASS' if r.returncode==0 else 'CHECK_ALL_FAIL'); sys.exit(r.returncode)"
  EXPECT: CHECK_ALL_PASS
  EVIDENCE: CHECK_ALL_PASS
- [x] G12b: A7 워크플로에 임시 continue-on-error 없음
  CHECK: python3 -c "import pathlib; hits=[p for p in ['.github/workflows/ci.yml','.github/workflows/release.yml'] if 'continue-on-error' in pathlib.Path(p).read_text()]; print('NO_CONTINUE_ON_ERROR' if not hits else 'FOUND '+','.join(hits))"
  EXPECT: NO_CONTINUE_ON_ERROR
  EVIDENCE: NO_CONTINUE_ON_ERROR
- [x] G13: A8 지원 플랫폼 서술에 Windows 포함, 낡은 단정 문구 없음
  CHECK: python3 scripts/verify-docs-platform.py
  EXPECT: PLATFORM_DOCS_OK
  EVIDENCE: PLATFORM_DOCS_OK
- [x] G14a: A7 워크플로 문법 유효
  CHECK: python3 -c "import subprocess,sys; r=subprocess.run(['uvx','--from','actionlint-py','actionlint','.github/workflows/ci.yml','.github/workflows/release.yml'],capture_output=True,text=True); print('ACTIONLINT_OK' if r.returncode==0 and not r.stdout.strip() else 'ACTIONLINT_FAIL '+r.stdout[:300]); sys.exit(r.returncode)"
  EXPECT: ACTIONLINT_OK
  EVIDENCE: ACTIONLINT_OK
- [x] G15: 플랫폼 분기가 composition.rs와 outbound에만 있음
  CHECK: python3 scripts/verify-platform-fence.py
  EXPECT: PLATFORM_FENCE_OK
  EVIDENCE: PLATFORM_FENCE_OK
- [x] G16: 프런트 번들이 chrome120 하한으로 빌드됨
  CHECK: bun run vite:build
  EXPECT: /built in/
  EVIDENCE: ✓ built in 186ms | $ tsc --noEmit && vite build
