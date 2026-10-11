# Gates: 14

- [x] G1: 녹음 소스 값 객체·설정 저장·명령이 동작한다
  CHECK: powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:/Users/m16kh/galpi-gate.ps1 source
  EXPECT: /test result: ok\. [1-9][0-9]* passed/
  EVIDENCE: test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 348 filtered out; finished in 0.01s | test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- [x] G2: 합치기·다운믹스와 정리 경로가 정확하다
  CHECK: powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:/Users/m16kh/galpi-gate.ps1 recording
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
  EVIDENCE: test result: ok. 25 passed; 0 failed; 1 ignored; 0 measured; 327 filtered out; finished in 0.06s
- [x] G3: Os 분기가 cfg 없이 두 플랫폼에서 컴파일된다
  CHECK: python3 -c "import re; pats=[r'cfg[(]windows',r'cfg[(]unix',r'target_os',r'cfg![(]']; n=sum(len(re.findall(p, open('src-tauri/src/adapters/outbound/recording/'+f+'.rs').read())) for f in ['mod','capture','mix'] for p in pats); print('FENCE_CLEAN' if n==0 else 'FENCE_HIT %d' % n)"
  EXPECT: FENCE_CLEAN
  EVIDENCE: FENCE_CLEAN
- [x] G4: Rust fmt/clippy/test 전체 게이트
  CHECK: powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:/Users/m16kh/galpi-gate.ps1 all
  EXPECT: RUST_GATES_PASS
  EVIDENCE: RUST_GATES_PASS fmt=ok clippy=ok passed=351 failed=0
- [x] G7: 프런트 check와 테스트
  CHECK: python3 -c "import subprocess; a=subprocess.run(['bun','run','check'],capture_output=True).returncode; t=subprocess.run(['bun','test'],capture_output=True,text=True); print('FRONT_PASS' if a==0 and t.returncode==0 else 'FRONT_FAIL'); print([l for l in (t.stdout+t.stderr).splitlines() if l.strip().endswith(' pass') or l.strip().endswith(' fail')])"
  EXPECT: FRONT_PASS
  EVIDENCE: FRONT_PASS | [' 274 pass', ' 0 fail']
- [x] G8: 플랫폼 펜스·문서 펜스·plist 키
  CHECK: python3 -c "import subprocess; a=subprocess.run(['python3','scripts/verify-platform-fence.py']).returncode; b=subprocess.run(['python3','scripts/verify-docs-platform.py']).returncode; c=subprocess.run(['grep','-q','NSAudioCaptureUsageDescription','src-tauri/Info.plist']).returncode; print('DOCS_PASS' if a==0 and b==0 and c==0 else 'DOCS_FAIL')"
  EXPECT: DOCS_PASS
  EVIDENCE: PLATFORM_DOCS_OK | DOCS_PASS
