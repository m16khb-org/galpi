# Gates: 5

- [x] G1: @seed-design/css가 정확히 3.0.2로 package.json dependencies와 bun.lock에 고정됨
  CHECK: bun -e 'const p=require("./package.json");const l=require("node:fs").readFileSync("bun.lock","utf8");console.log(p.dependencies["@seed-design/css"]==="3.0.2"&&l.includes("@seed-design/css@3.0.2")?"seed-pinned":"seed-missing")'
  EXPECT: seed-pinned
  EVIDENCE: seed-pinned
- [x] G2: styles.css가 base.css와 action-button.css를 불러오고 index.html이 light-only 모드를 고정함
  CHECK: bun -e 'const fs=require("node:fs");const c=fs.readFileSync("src/styles.css","utf8").split("\n");const h=fs.readFileSync("index.html","utf8");const n=["@import \"@seed-design/css/base.css\";","@import \"@seed-design/css/recipes/action-button.css\";"].filter((s)=>c.includes(s)).length;const m=h.split("data-seed-color-mode=\"light-only\"").length-1;console.log(`imports ${n} mode ${m}`)'
  EXPECT: imports 2 mode 1
  EVIDENCE: imports 2 mode 1
- [x] G3: styles.css에 hex·rgb·hsl 리터럴과 갈피 고유 토큰 정의가 0개이고 custom property 선언은 --seed-* 뿐
  CHECK: bun -e 'const c=require("node:fs").readFileSync("src/styles.css","utf8");const cnt=(re)=>(c.match(re)??[]).length;const lit=cnt(/#[0-9a-fA-F]{3,8}\b/g)+cnt(/rgba?\(/g)+cnt(/hsla?\(/g);const own=cnt(/^\s*--(?!seed-)[a-z0-9-]+\s*:/gm);console.log(`literals ${lit} own-tokens ${own}`)'
  EXPECT: literals 0 own-tokens 0
  EVIDENCE: literals 0 own-tokens 0
- [x] G4: styles.css 계약 테스트(토큰만·리터럴 없음·hidden·workspace 4행·reduced-motion·keep-all·AA 재지정 위치)가 통과함
  CHECK: bun test src/styles.test.ts
  EXPECT: /^\s*0 fail$/m
  EVIDENCE: 20 expect() calls | Ran 11 tests across 1 file. [6.00ms]
- [x] G5: app-template.ts의 id·name·role·data-*·aria-*·type·for 속성 집합이 base와 같고 src/ui 기존 테스트에서 삭제된 줄이 0
  CHECK: bun -e 'const {execSync}=require("node:child_process");const {readFileSync}=require("node:fs");const names=["id","name","role","type","for"];const f=(s)=>(s.match(/[a-z-]+="[^"]*"/g)??[]).filter((a)=>{const k=a.split("=")[0];return k.startsWith("data-")?true:k.startsWith("aria-")?true:names.includes(k)}).sort().join("\n");const a=execSync("git show 456b500cd843577e381d56842465656e3b596e4a:src/ui/app-template.ts").toString();const same=f(a)===f(readFileSync("src/ui/app-template.ts","utf8"));const del=execSync("git diff --numstat 456b500cd843577e381d56842465656e3b596e4a -- src/ui").toString().trim().split("\n").filter((l)=>l.endsWith(".test.ts")).reduce((s,l)=>s+Number(l.split("\t")[1]),0);console.log(`template-attrs ${same?"same":"DIFF"} test-deletions ${del}`)'
  EXPECT: template-attrs same test-deletions 0
  EVIDENCE: template-attrs same test-deletions 0
- [x] G6: 버튼이 action-button recipe 클래스로 렌더되고 setPrimary 상태 전환과 삭제 버튼 빌더가 recipe variant를 따름
  CHECK: bun test src/ui/seed.test.ts src/ui/app-view.dom.test.ts src/ui/controller.test.ts
  EXPECT: /^\s*0 fail$/m
  EVIDENCE: 125 expect() calls | Ran 31 tests across 3 files. [243.00ms]
- [x] G7: 전체 TypeScript 테스트가 통과함
  CHECK: bun test
  EXPECT: /^\s*0 fail$/m
  EVIDENCE: 289 expect() calls | Ran 120 tests across 19 files. [428.00ms]
- [x] G8: 루트 DESIGN.md의 --seed-* 이름 집합이 styles.css와 같고 DESIGN.md에 warm·pure black 서술이 없음
  CHECK: bun test src/styles.test.ts -t DESIGN.md
  EXPECT: /^\s*0 fail$/m
  EVIDENCE: 2 expect() calls | Ran 2 tests across 1 file. [4.00ms]
- [x] G9: 루트 DESIGN.md와 .issueops/DESIGN.md가 @seed-design/css를 값의 단일 소스로 가리킴
  CHECK: bun -e 'const fs=require("node:fs");const n=(p)=>(fs.readFileSync(p,"utf8").match(/@seed-design\/css/g)??[]).length;console.log(n("DESIGN.md")>0&&n(".issueops/DESIGN.md")>0?"docs-point-to-seed":"docs-missing-seed")'
  EXPECT: docs-point-to-seed
  EVIDENCE: docs-point-to-seed
- [x] G10: 아키텍처 검사·Biome·tsc 정적 검사가 통과함
  CHECK: bun -e 'const r=Bun.spawnSync(["bun","run","check"]);console.log(r.exitCode===0?"check-pass":"check-FAIL "+r.stderr.toString().slice(-2000))'
  EXPECT: check-pass
  EVIDENCE: check-pass
- [x] G11: vite 프로덕션 번들 CSS가 140 kB 이하
  CHECK: bun -e 'const r=Bun.spawnSync(["bun","run","vite:build"]);const fs=require("node:fs");const f=fs.readdirSync("dist/assets").filter((x)=>/^index-.*\.css$/.test(x));const kb=f.length===1?fs.statSync("dist/assets/"+f[0]).size/1000:-1;console.log(r.exitCode===0&&kb>0&&kb<=140?"css-size-ok "+kb.toFixed(1):"css-size-FAIL "+kb)'
  EXPECT: /^css-size-ok \d/
  EVIDENCE: css-size-ok 109.1
- [x] G12: 레이아웃 불변 선언(레일 248px 열, workspace 4행, keep-all, 파형 clip-path)이 남아 있음
  CHECK: bun -e 'const c=require("node:fs").readFileSync("src/styles.css","utf8");const k=["grid-template-columns: 248px minmax(0, 1fr)","grid-template-rows: auto auto minmax(0, 1fr) auto","word-break: keep-all","clip-path: inset(0 calc(100% - var(--progress)) 0 0)"];console.log(k.every((s)=>c.includes(s))?"layout-ok":"layout-MISSING")'
  EXPECT: layout-ok
  EVIDENCE: layout-ok
- [x] G13: AA 재지정 블록이 :root[data-seed-color-mode=light-only] 선택자로 정확히 한 번 있고 html[...] 선택자는 없음
  CHECK: bun -e 'const c=require("node:fs").readFileSync("src/styles.css","utf8");const r=(c.match(/^:root\[data-seed-color-mode="light-only"\] \{/gm)??[]).length;const h=(c.match(/html\[data-seed-color-mode/g)??[]).length;console.log(`root-block ${r} html-block ${h}`)'
  EXPECT: root-block 1 html-block 0
  EVIDENCE: root-block 1 html-block 0
- [x] G14: .text-button 규칙이 남고 위 간격이 --seed-dimension-x2_5로 유지됨
  CHECK: bun -e 'const c=require("node:fs").readFileSync("src/styles.css","utf8");const m=c.match(/^\.text-button \{[^}]*margin-top: var\(--seed-dimension-x2_5\)[^}]*\}/m);console.log(m?"text-button-gap-ok":"text-button-gap-MISSING")'
  EXPECT: text-button-gap-ok
  EVIDENCE: text-button-gap-ok
- [ ] G15: Tauri 앱 번들과 DMG 프로덕션 빌드가 성공함
  CHECK: bun -e 'const r=Bun.spawnSync(["bun","run","build"]);const out=r.stdout.toString()+r.stderr.toString();console.log(r.exitCode===0?"build-pass":"build-FAIL "+out.slice(-3000))'
  EXPECT: build-pass
  EVIDENCE: pending
ABANDON: G15 환경 실패(이 변경과 무관): cargo tauri build가 tauri.conf.json minimumSystemVersion 14.0으로 MACOSX_DEPLOYMENT_TARGET=14.0을 설정하면 macOS 27.0.1(ld-27037.1, rustc 1.97.0)에서 만든 proc-macro dylib을 dyld가 'mis-aligned LINKEDIT string pool'로 거부한다. 저장소 밖 빈 serde_derive 탐침 크레이트에서도 같은 환경변수로 재현되고 환경변수 없이는 빌드된다. 이 변경은 src-tauri와 tauri.conf.json을 건드리지 않았고, 같은 bun run build의 프런트엔드 단계(sidecar:stage, tsc, vite build)는 통과했다. Rust 빌드 환경 수정은 이슈 범위 밖.
