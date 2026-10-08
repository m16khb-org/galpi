# Gates: 4

- [x] G1: 인가 요청 계약·최초 등록·재로그인 콜백, state 불일치·access_denied 때 코드 교환 0회(완료 기준 1·7)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'adapters::outbound::chatgpt']];K=['cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=36
- [x] G2: 설정 시트의 ChatGPT로 계속하기·<이메일>로 로그인됨·모델 목록·로그아웃·동의 거부/한도 문구(완료 기준 1·3·6·7 UI)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['bun', 'test', 'src/ui/chatgpt-settings.dom.test.ts', 'src/application/chatgpt-machine.test.ts', 'src/domain/chatgpt.test.ts', 'src/domain/backend.test.ts', 'src/adapters/tauri-backend.test.ts']];K=['bun'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=43
- [x] G4: 로그인 상태 재시작 유지(비밀 읽기 0회)·IPC 직렬화 키 집합에 토큰 없음·설정 파일 토큰 부재(완료 기준 2)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'settings::chatgpt'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'domain::chatgpt']];K=['cargo', 'cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=9,7
- [x] G6: 모델 목록 visibility=list 필터·서버 순서, 만료 토큰 목록 조회 전 갱신, 선택 slug가 RefinementJob.model로 전달(완료 기준 3)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'application::tests::chatgpt'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'expired_access_token_refreshes_before_listing_models'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'adapters::outbound::chatgpt::tests::models']];K=['cargo', 'cargo', 'cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=26,1,2
- [x] G7: Responses 전송이면 GALPI_ASSISTANT_TRANSPORT=responses이고 base URL·effort 없음, ChatCompletions는 기존 키 불변(완료 기준 4)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'adapters::outbound::environment']];K=['cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=3
- [x] G8: Responses 스트림 전송·맵리듀스 동일 전송·response.completed 필수·한도 코드 매핑·재시도 없음(완료 기준 4·7)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['env', 'PYTHONPATH=.', 'python3', '-m', 'unittest', 'worker.tests.test_responses_stream', '-v']];K=['py'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=14
- [x] G10: 만료 토큰 자동 갱신·회전된 refresh token 교체 저장·동시 정제 시 갱신 1회·일시 오류 시 삭제 없음(완료 기준 5)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'expired_access_token'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'rotated_refresh_token']];K=['cargo', 'cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=3,1
- [x] G12: 로그아웃 시 토큰·client_id 삭제, API 키 모드 복귀, host id 유지, 폐기 미확인에도 삭제(완료 기준 6)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'sign_out'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'settings::chatgpt']];K=['cargo', 'cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=5,9
- [x] G14: 워커 안정 코드가 AppError로 전달·refine 1회·동의 거부 시 교환 0회·한도 시 사용량 관리 노출(완료 기준 7)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'adapters::outbound::process'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'application::tests::chatgpt'], ['bun', 'test', 'src/ui/chatgpt-settings.dom.test.ts', 'src/ui/controller.test.ts']];K=['cargo', 'cargo', 'bun'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=7,26,25
- [x] G16: API 키 모드 기존 테스트 4개가 수정 없이 통과(완료 기준 8)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'refinement_sends_saved_background_and_publishes_minutes'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'refinement_is_rejected_before_a_'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'out_of_the_settings_file'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'never_reaches_the_keychain_again']];K=['cargo', 'cargo', 'cargo', 'cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=1,1,1,1
- [x] G17: 아키텍처 펜스·Biome·tsc·전체 Bun 테스트 통과
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['bun', 'run', 'check'], ['bun', 'test']];K=[None, 'bun'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=1,151
- [x] G18: cargo fmt·clippy -D warnings·전체 Rust 테스트 통과
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'fmt', '--manifest-path', 'src-tauri/Cargo.toml', '--check'], ['cargo', 'clippy', '--manifest-path', 'src-tauri/Cargo.toml', '--all-targets', '--', '-D', 'warnings'], ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--all-targets']];K=[None, None, 'cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=1,1,151
- [x] G19: ruff check·format·전체 워커 unittest 통과
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['uvx', 'ruff', 'check', 'worker'], ['uvx', 'ruff', 'format', '--check', 'worker'], ['env', 'PYTHONPATH=.', 'python3', '-m', 'unittest', 'discover', '-s', 'worker/tests', '-t', '.', '-v']];K=[None, None, 'py'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=1,1,105
- [x] G20: InMemorySecrets가 쓰기 횟수를 실제로 세고 같은 ChatGPT 토큰은 다시 쓰지 않음
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'repeated_identical_chatgpt_tokens_are_not_rewritten']];K=['cargo'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: ALL_PASS counts=1
  EVIDENCE: ALL_PASS counts=1
- [x] G21: 맵 단계 첫 실패 뒤 새 청크 요청 없음(Responses·API 키 모드 모두 요청 수 ≤ 3, 첫 오류 코드 유지)
  CHECK: python3 -c "import subprocess as s,sys,re;C=[['env', 'PYTHONPATH=.', 'python3', '-m', 'unittest', 'worker.tests.test_responses_stream', '-k', 'map_failure', '-v']];K=['py'];P={'cargo':'test result: ok. ([0-9]+) passed','bun':'(?m)^ *([0-9]+) pass$','py':'Ran ([0-9]+) test'};R=[s.run(c,capture_output=True,text=True) for c in C];N=[sum(map(int,re.findall(P[k],r.stdout+r.stderr))) if k else 1 for k,r in zip(K,R)];F=[' '.join(c[:4]) for c,r,n in zip(C,R,N) if r.returncode or n==0];print('FAILED: '+'; '.join(F) if F else 'ALL_PASS counts='+','.join(map(str,N)));sys.exit(1 if F else 0)"
  EXPECT: /^ALL_PASS/m
  EVIDENCE: ALL_PASS counts=2
