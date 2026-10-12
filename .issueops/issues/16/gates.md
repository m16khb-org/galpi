# Gates: 16

- [x] G1: Drive 토큰 어댑터가 Google 토큰 엔드포인트에 form으로 client id·보안 비밀번호·code·code verifier·redirect uri를 보내고, 갱신도 같은 엔드포인트에 보내며, 400 invalid_grant만 Rejected, 401·5xx·연결 실패는 Unavailable로 매핑한다
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 12.49s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib adapters::outbound::drive
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
- [x] G2: Drive use case가 로그인 없이 연결·가져오기·동기화·업로드하고 invalid_grant 갱신 실패만 연결을 지운다
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 1.16s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib application::tests::drive
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
- [x] G3: Rust 전체 테스트가 통과한다
  EVIDENCE: Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8) | Running unittests src/main.rs (src-tauri/target/debug/deps/galpi-3a6bea8a51165c32)
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --all-targets -- --quiet
  EXPECT: /test result: ok\. [1-9][0-9]* passed; 0 failed/
- [x] G4: Rust 린트가 경고 없이 통과한다
  EVIDENCE: CLIPPY_OK
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"cargo\",\"clippy\",\"--manifest-path\",\"src-tauri/Cargo.toml\",\"--all-targets\",\"--\",\"-D\",\"warnings\"],capture_output=True);print(\"CLIPPY_OK\" if r.returncode==0 else \"CLIPPY_FAIL\");sys.exit(r.returncode)"
  EXPECT: CLIPPY_OK
- [x] G5: Rust 형식이 맞다
  EVIDENCE: FMT_OK
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"cargo\",\"fmt\",\"--manifest-path\",\"src-tauri/Cargo.toml\",\"--check\"],capture_output=True);print(\"FMT_OK\" if r.returncode==0 else \"FMT_FAIL\");sys.exit(r.returncode)"
  EXPECT: FMT_OK
- [x] G6: 프런트 아키텍처 펜스, Biome, tsc가 통과한다
  EVIDENCE: CHECK_OK
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run([\"bun\",\"run\",\"check\"],capture_output=True);print(\"CHECK_OK\" if r.returncode==0 else \"CHECK_FAIL\");sys.exit(r.returncode)"
  EXPECT: CHECK_OK
- [x] G7: 프런트 테스트가 통과한다(시작 시 로그인 없이 작업 화면을 불러오고, 런타임 연결 실패는 오류 배너로 보인다)
  EVIDENCE: 892 expect() calls | Ran 282 tests across 27 files. [4.43s]
  CHECK: bun test
  EXPECT: /^\s*0 fail$/m
- [x] G8: 소스에 gateway 흔적(AUTH_REQUIRED, GATEWAY_ 코드, gateway 주소, 대행 경로, 접근 게이트 심볼, gateway 명령 4개, 열거형 변형·필드·모듈 이름)이 남지 않는다. 옛 설정 파일 모양을 고정하는 `settings/drive.rs`의 `gatewayEmail` 리터럴은 패턴에 걸리지 않는다
  EVIDENCE: GATEWAY_GONE
  CHECK: python3 -c "import subprocess,sys;p='AUTH_REQUIRED|GATEWAY_|auth[.]m16khb|drive/native/token|AppAccess|app_access|[-_]gateway|gateway[-_:;]|Gateway';r=subprocess.run(['git','grep','--untracked','-nE',p,'--','src','src-tauri/src'],capture_output=True);print('GATEWAY_GONE' if r.returncode==1 else 'GATEWAY_LEFT');sys.exit(0 if r.returncode==1 else 1)"
  EXPECT: GATEWAY_GONE
- [x] G9: 실제 스모크의 필수 단계(로그인 화면 없음, 클라이언트 없음 안내, 클라이언트 저장, Drive 연결, 결과 업로드, 토큰 갱신, Picker 가져오기, 연결 중 변경 거부, 클라이언트 삭제)가 모두 통과로 기록되고 토큰 엔드포인트가 Google임이 적혀 있다
  EVIDENCE: SMOKE_OK
  CHECK: python3 -c "import pathlib,sys;t=pathlib.Path('.issueops/evidence/task-16-smoke.md').read_text().splitlines();need=['로그인 화면 없음','클라이언트 없음 안내','클라이언트 저장','Drive 연결','결과 업로드','토큰 갱신','Picker 가져오기','연결 중 변경 거부','클라이언트 삭제'];miss=[n for n in need if not any(n in l and '| 통과 |' in l for l in t)];ok=not miss and any('oauth2.googleapis.com/token' in l for l in t);print('SMOKE_OK' if ok else 'SMOKE_MISSING');sys.exit(0 if ok else 1)"
  EXPECT: SMOKE_OK
- [x] G10: gateway를 전제로 쓴 문서(README 두 벌, INSTALL, DESIGN, AGENTS, docs/ARCHITECTURE, outbound AGENTS, .issueops 아키텍처·테스트 개요)에 gateway 문장과 옛 포트 수(18)·명령 수(40, 37)가 남지 않는다. `auth-gateway` 토큰은 뒤에 하이픈이 오는 ADR 파일 이름을 통과시킨다. README의 잔여물 안내 문장은 이 패턴의 토큰을 쓰지 않는다(예: '이전 버전의 로그인 세션 항목 `gateway-session`')
  EVIDENCE: DOCS_CLEAN
  CHECK: python3 -c "import subprocess,sys;p='auth[.]m16khb|GALPI_AUTH_GATEWAY_URL|gateway-brokered|drive/native/token|GatewayAuthPort|GatewaySessionStore|AppAccessGate|access-controller|access-machine|access-screen|account-chip|AUTH_REQUIRED|auth-gateway([^-]|$)|18 ports|18개|40 frontend|40 커맨드|40개|40 command|37 .#.tauri::command';r=subprocess.run(['git','grep','-nE',p,'--','README.md','README.en.md','INSTALL.md','DESIGN.md','AGENTS.md','docs/ARCHITECTURE.md','.issueops/architecture/overview.md','.issueops/testing/overview.md','src-tauri/src/adapters/outbound/AGENTS.md'],capture_output=True);print('DOCS_CLEAN' if r.returncode==1 else 'DOCS_STALE');sys.exit(0 if r.returncode==1 else 1)"
  EXPECT: DOCS_CLEAN
- [x] G11: 두 옛 ADR(2026-10-09 auth-gateway, 2026-10-11 Drive gateway 대행)을 모두 가리키는 새 ADR이 있다
  EVIDENCE: ADR_OK
  CHECK: python3 -c "import pathlib,sys;old={'overview.md','2026-10-09-auth-gateway-google.md','2026-10-11-google-drive-auth-gateway-appproperties.md'};f=[p.name for p in pathlib.Path('.issueops/adr').glob('*.md') if p.name not in old and '2026-10-09-auth-gateway-google' in p.read_text() and '2026-10-11-google-drive-auth-gateway-appproperties' in p.read_text()];print('ADR_OK' if f else 'ADR_MISSING');sys.exit(0 if f else 1)"
  EXPECT: ADR_OK
- [x] G12: 형식 검증. 클라이언트 ID 형식 오류, 보안 비밀번호의 빈 값·공백, 앞뒤 공백 trim이 domain과 use case에서 `DRIVE_CLIENT_INVALID`로 판정된다
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.94s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib drive_client_invalid_
  EXPECT: /test result: ok\. ([3-9]|[1-9][0-9]+) passed; 0 failed/
- [x] G13: Drive가 연결된 동안의 저장(`drive_client_locked_save_while_connected`), 연결된 동안의 삭제(`drive_client_locked_clear_while_connected`), 연결 동의를 기다리는 동안의 저장(`drive_client_locked_save_while_consent`)과 삭제(`drive_client_locked_clear_while_consent`)가 `DRIVE_CLIENT_LOCKED`로 거부되고 저장소가 바뀌지 않으며, Picker 동의 대기 중 해제·저장·재연결이 일어나면 옛 클라이언트 토큰이 붙지 않는다(`drive_client_locked_picker_client_changed`)
  EVIDENCE: LOCKED_OK
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--lib','drive_client_locked_'],capture_output=True,text=True);o=r.stdout;need=['drive_client_locked_save_while_connected','drive_client_locked_clear_while_connected','drive_client_locked_save_while_consent','drive_client_locked_clear_while_consent','drive_client_locked_picker_client_changed'];miss=[n for n in need if not any(n in l and l.rstrip().endswith('ok') for l in o.splitlines())];ok=r.returncode==0 and not miss;print('LOCKED_OK' if ok else ('LOCKED_FAIL',miss));sys.exit(0 if ok else 1)"
  EXPECT: LOCKED_OK
- [x] G14: 저장한 클라이언트가 동의 URL의 `client_id`(`drive_client_used_consent_url`), 교환 form의 `client_id`·`client_secret`(`drive_client_used_exchange_form`), 갱신 form(`drive_client_used_refresh_form`)에 쓰이고, use case가 connect(`drive_client_used_connect_passes_stored`), Picker(`drive_client_used_picker_passes_stored`), 갱신(`drive_client_used_renewal_passes_stored`)에 저장한 클라이언트를 넘긴다
  EVIDENCE: USED_OK
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--lib','drive_client_used_'],capture_output=True,text=True);o=r.stdout;need=['drive_client_used_consent_url','drive_client_used_exchange_form','drive_client_used_refresh_form','drive_client_used_connect_passes_stored','drive_client_used_picker_passes_stored','drive_client_used_renewal_passes_stored'];miss=[n for n in need if not any(n in l and l.rstrip().endswith('ok') for l in o.splitlines())];ok=r.returncode==0 and not miss;print('USED_OK' if ok else ('USED_FAIL',miss));sys.exit(0 if ok else 1)"
  EXPECT: USED_OK
- [x] G15: IPC 응답 `DriveClientStatus`의 JSON 키가 `configured`, `clientId` 둘뿐이고, `DriveClient`의 `Debug`에 비밀번호가 없다
  EVIDENCE: Finished `test` profile [unoptimized + debuginfo] target(s) in 0.42s | Running unittests src/lib.rs (src-tauri/target/debug/deps/galpi_lib-8b8b8ffe811d62b8)
  CHECK: cargo test --manifest-path src-tauri/Cargo.toml --lib drive_client_hidden_
  EXPECT: /test result: ok\. ([2-9]|[1-9][0-9]+) passed; 0 failed/
- [x] G16: 저장한 클라이언트가 재시작 뒤에도 남고, 연결 해제(`clear`)는 그것을 지우지 않으며, 클라이언트만 있는 설정 파일이 지워지지 않고, 삭제 뒤 두 값이 모두 사라지며, 덮어쓰기 중간 상태(ID 없음 + 새 비밀번호)가 저장 안 됨으로 읽힌다(`drive_client_stored_overwrite_interrupted_reads_as_none`)
  EVIDENCE: STORED_OK
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--lib','drive_client_stored_'],capture_output=True,text=True);o=r.stdout;n=sum(1 for l in o.splitlines() if l.startswith('test ') and 'drive_client_stored_' in l and l.rstrip().endswith('ok'));ov=any('drive_client_stored_overwrite_interrupted_reads_as_none' in l and l.rstrip().endswith('ok') for l in o.splitlines());ok=r.returncode==0 and n>=4 and ov;print('STORED_OK' if ok else ('STORED_FAIL',n,ov));sys.exit(0 if ok else 1)"
  EXPECT: STORED_OK
- [x] G17: 새 비밀 변형이 `settings.rs`의 네 match 분기와 `is_empty`에 모두 들어 있다
  EVIDENCE: STORE_ARMS_OK
  CHECK: python3 -c "import sys,pathlib as P;S='Sec'+'ret';t=P.Path('src-tauri/src/adapters/outbound/settings.rs').read_text();n=t.count(S+'::DriveClient'+S);i=t.index('fn is_empty');b=t[i:i+2600];ok=n>=4 and all(k in b for k in ['drive_client_id','drive_client_'+S.lower()+'.','drive_client_'+S.lower()+'_stored']);print('STORE_ARMS_OK' if ok else ('STORE_ARMS_FAIL',n));sys.exit(0 if ok else 1)"
  EXPECT: STORE_ARMS_OK
- [x] G18: 프런트의 클라이언트 동작(연결 중 비활성과 잠금 안내, 저장 성공 뒤 입력칸 비움과 `저장됨`, 저장된 상태에서 입력칸·저장 숨김, 비밀번호 칸 속성, 형식 오류 문구와 틀린 칸의 `aria-invalid`·`aria-describedby`, 삭제 성공 뒤 포커스가 비워진 클라이언트 ID 칸으로 이동, 클라이언트가 없으면 연결·가져오기 대신 설정 진입, 옛 연결(연결 해제 + 클라이언트 없음) 처리, Zod가 비밀번호 필드가 붙은 응답을 거부)이 테스트 이름에 `Drive client`를 넣은 테스트로 고정된다
  EVIDENCE: BUN_CLIENT_OK
  CHECK: python3 -c "import re,subprocess,sys;r=subprocess.run(['bun','test','-t','Drive client'],capture_output=True,text=True);o=r.stdout+r.stderr;p=re.search(r'^ *([0-9]+) pass',o,re.M);f=re.search(r'^ *([0-9]+) fail',o,re.M);ok=r.returncode==0 and p and int(p.group(1))>=12 and f and f.group(1)=='0';print('BUN_CLIENT_OK' if ok else 'BUN_CLIENT_FAIL');sys.exit(0 if ok else 1)"
  EXPECT: BUN_CLIENT_OK
- [x] G19: 명령 39개·포트 16개가 코드와 문서에서 일치하고 옛 숫자 36이 남지 않는다
  EVIDENCE: COUNTS_OK
  CHECK: python3 -c "import re,sys,pathlib as P;t=P.Path('src-tauri/src/adapters/inbound/tauri.rs').read_text().count('#[tauri::command]');q=len(re.findall(r'^pub trait',P.Path('src-tauri/src/application/ports.rs').read_text(),re.M));old=['36 frontend','36 command','36 커맨드','36개','36 '+chr(96)+'#[tauri'];new=[('AGENTS.md','39 frontend'),('AGENTS.md','39 command paths'),('docs/ARCHITECTURE.md','39 커맨드'),('docs/ARCHITECTURE.md','39개'),('.issueops/architecture/overview.md','39 '+chr(96)+'#[tauri')];fs=['AGENTS.md','docs/ARCHITECTURE.md','.issueops/architecture/overview.md','src-tauri/AGENTS.md','.issueops/conventions/overview.md'];left=[(f,x) for f in fs for x in old if x in P.Path(f).read_text()];miss=[(f,x) for f,x in new if x not in P.Path(f).read_text()];ok=t==39 and q==16 and not left and not miss;print('COUNTS_OK' if ok else ('COUNTS_FAIL',t,q,left,miss));sys.exit(0 if ok else 1)"
  EXPECT: COUNTS_OK
- [x] G20: README 두 벌에 Google 클라이언트 만드는 방법(Drive API와 Picker API 사용 설정, 데스크톱 앱 유형, 테스트 게시 상태의 7일 만료, 설정 시트 위치)과 클라이언트를 바꾸면 새 Galpi 폴더가 생긴다는 주의가 있고, 옛 내장 클라이언트·선택 사항 문구는 남지 않는다
  EVIDENCE: GUIDE_OK
  CHECK: python3 -c "import sys,pathlib as P;k=P.Path('README.md').read_text();e=P.Path('README.en.md').read_text();ko=all(x in k for x in ['Google Drive 사용 준비','Google Picker API','Google Drive API','데스크톱 앱','7일','새 Galpi 폴더']) and not any(x in k for x in ['기본 클라이언트','내장 클라이언트','내 Google 클라이언트 사용']);en=all(x in e for x in ['Set up Google Drive','Google Picker API','Google Drive API','Desktop app','7 days','new Galpi folder']) and not any(x in e.lower() for x in ['built-in client','default client']);ok=ko and en;print('GUIDE_OK' if ok else ('GUIDE_FAIL',ko,en));sys.exit(0 if ok else 1)"
  EXPECT: GUIDE_OK
- [x] G21: 소스에 Google OAuth 클라이언트 값이 없다. 내장 클라이언트 상수가 사라졌고, 비밀번호 모양(GOCSPX- 접두어)과 실제 모양의 클라이언트 ID가 추적·미추적 파일 어디에도 없다(이슈 12 계획 문서의 옛 ID는 이 변경의 범위 밖이라 제외한다)
  EVIDENCE: NO_CLIENT_VALUE
  CHECK: python3 -c "import subprocess,sys;V='GOCSPX-[A-Za-z0-9_-]{16,}|[0-9]{9,}-[a-z0-9]{20,}[.]apps[.]googleusercontent';a=subprocess.run(['git','grep','--untracked','-nE',V,'--','.',':!.issueops/issues/12'],capture_output=True);b=subprocess.run(['git','grep','--untracked','-nE','DEFAULT_CLIENT|_B64','--','src','src-tauri/src'],capture_output=True);ok=a.returncode==1 and b.returncode==1;print('NO_CLIENT_VALUE' if ok else 'CLIENT_VALUE_LEFT');sys.exit(0 if ok else 1)"
  EXPECT: NO_CLIENT_VALUE
- [x] G22: 설정 시트의 안내 요약에 만드는 방법의 핵심(Drive API·Picker API 사용 설정, 데스크톱 앱, 프로덕션 게시)과 새 Galpi 폴더 주의가 있다
  EVIDENCE: SHEET_GUIDE_OK
  CHECK: python3 -c "import pathlib,sys;t=pathlib.Path('src/ui/app-template.ts').read_text();ok=all(x in t for x in ['Google Drive API','Google Picker API','데스크톱 앱','프로덕션','새 Galpi 폴더']);print('SHEET_GUIDE_OK' if ok else 'SHEET_GUIDE_FAIL');sys.exit(0 if ok else 1)"
  EXPECT: SHEET_GUIDE_OK
- [x] G23: 내장 클라이언트가 없다. 클라이언트를 저장하지 않으면 연결과 Picker가 Google을 부르기 전에 `DRIVE_CLIENT_REQUIRED`로 멈추고(`drive_client_required_connect_without_client`, `drive_client_required_import_without_client`), 내장 클라이언트로 만든 옛 연결은 시작 때 해제·폐기되며(`drive_client_required_old_connection_is_dropped_at_status`) 클라이언트 없이는 갱신하지 않는다(`drive_client_required_renewal_without_client_reconnects`)
  EVIDENCE: REQUIRED_OK
  CHECK: python3 -c "import subprocess,sys;r=subprocess.run(['cargo','test','--manifest-path','src-tauri/Cargo.toml','--lib','drive_client_required_'],capture_output=True,text=True);o=r.stdout;need=['drive_client_required_connect_without_client','drive_client_required_import_without_client','drive_client_required_old_connection_is_dropped_at_status','drive_client_required_renewal_without_client_reconnects'];miss=[n for n in need if not any(n in l and l.rstrip().endswith('ok') for l in o.splitlines())];ok=r.returncode==0 and not miss;print('REQUIRED_OK' if ok else ('REQUIRED_FAIL',miss));sys.exit(0 if ok else 1)"
  EXPECT: REQUIRED_OK
