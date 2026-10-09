# 당근 SEED 디자인 시스템 토큰·컴포넌트 스타일 적용 (`@seed-design/css` 3.0.2)

- 라이프사이클 ID: `io-0c055863a1cf`
- 이슈: https://github.com/m16khb-org/galpi/issues/5
- 브랜치: `5-seed-design-system`
- 기준(base): `main@456b500cd843577e381d56842465656e3b596e4a`
- 사용자 요청 범위: "이슈 생성 후 세 사이클을 병렬로 전체 진행: 구현 → 정리 → 문서 → 검증 → 커밋·푸시 → draft PR 발행 → execution complete"
- 워크트리 준비 후 Herdr 새 세션으로 인계되며, 인계는 승인 범위를 넓히지 않는다
- 범위 원칙(사용자 발언): "디자인 시스템만 당근 걸 쓰는 거지 디자인을 바꾸겠다는 게 아니야" — 레이아웃·정보 구조·흐름·카피·아이콘·상태 모델·selector는 그대로 두고 값만 SEED에서 받는다.
- 조사 방법: `npm pack @seed-design/css@3.0.2`를 `/tmp/seedpkg`에 풀어 `base.css`, `recipes/*.css`, `package.json` exports를 직접 읽었고, 저장소 밖 `/tmp/seedvite` 스크래치에서 `vite build`(target `safari17`, `cssMinify: "lightningcss"`)와 happy-dom 파싱을 시험했다. 저장소 파일은 건드리지 않았다.

## 이슈 본문과 다른 사실

1. **recipe 이름**: 본문은 `text-field`, `select`, `toggle`, `bottom-sheet`를 든다. 3.0.2 패키지에는 `text-field.css`가 없고(`recipes/`에서 `text-field` 0건) 대응물은 `text-input`(root·value 슬롯), 선택은 `select-trigger`/`select`/`select-item`(React 팝업 셀렉트), 토글은 `switch`/`toggle-button`/`chip`이다. 데스크톱 모달은 `bottom-sheet`(모바일 시트)가 아니라 `dialog`다. 갈피에는 스위치형 토글이 없다(라디오 세그먼트 `.segmented-control`, 체크박스 칩 `.participant-chip`, 눈 아이콘 버튼 `.secret-visibility-button`뿐 — `src/ui/app-template.ts:111-115, 206-209, 219`, `src/ui/participant-picker.ts:52-76`).
2. **"436개 파일"**: `recipes/` 항목 수가 436이고 그중 `.css`는 87개다. 나머지는 `.layered.css`·`.mjs`·`.d.ts`·`.layered.mjs`다.
3. **`--seed-dimension-x0`~`x16`**: `x0`은 없고 `x0_5`, `x1`, `x1_5` … `x16`(반 단계 포함)이다. 글자 크기도 `t1`~`t12`가 아니라 `t1`~`t14`다(`base.css` 439~580행대).
4. **완료 기준 2와 3의 충돌**: 기준 2(텍스트 필드·셀렉트·세그먼트·토글·시트가 SEED recipe 클래스로 렌더)와 기준 3(`app-template.ts`의 DOM 구조 유지)은 동시에 만족할 수 없다. `text-input`은 `input`을 감싸는 wrapper `div`가 root이고 포커스·invalid가 `[data-focus]`/`[data-invalid]` JS 속성에 걸려 있으며, `segmented-control`은 `__indicator` 요소와 `--segment-index`/`--segment-count` JS 변수가 필요하고, `dialog`는 positioner/backdrop/content/header/title/footer 슬롯에 `word-break: break-all`(한글 `keep-all` 계약과 충돌)을 둔다. 이슈 범위 절("selector와 `data-*` 계약은 유지", "클래스 이름 변경")과 사용자 지시("레이아웃·정보 구조 불변")에 따라 **기준 3을 우선**하고, 마크업 추가가 필요 없는 **버튼(`action-button`)만 recipe 클래스로 렌더**, 나머지는 같은 SEED 토큰·recipe 값을 갈피 클래스에서 참조한다(설계 §2~§5).
5. **완료 기준 3의 "기존 `bun test` 변경 없이"**: `src/styles.test.ts:28`은 `.section-heading h2`에 `font-size: 21px` 리터럴을 요구한다. 21px은 SEED 글자 크기가 아니다(`t7`=20px, `t8`=22px). 기준 1·5(타이포 값은 SEED)와 모순되므로 이 단언 한 줄만 `var(--seed-font-size-t8)`로 갱신한다. `src/ui/**` DOM·컨트롤러 테스트는 한 줄도 바꾸지 않는다(게이트 G5가 삭제 줄 0을 확인).
6. **색 접근성**: 본문은 "베이지 → SEED 시맨틱 색 변화 수용"이라고 하지만 루트 `DESIGN.md:226`의 WCAG AA 4.5:1 제약은 그대로다. SEED 기본값으로는 `--seed-color-bg-brand-solid`(#ff6600) 위 흰 글씨가 2.94:1(현재 #b75b37 위 #fffefa는 4.56:1), `--seed-color-stroke-focus-ring`(blue-600)이 흰 배경에서 2.84:1(현재 5.7:1), `--seed-color-fg-neutral-subtle`이 3.42:1, `--seed-color-fg-brand`가 2.94:1이다(WCAG 상대 휘도로 직접 계산). 그래서 SEED 팔레트 안의 더 짙은 단계로 시맨틱 변수 4개만 재지정한다(설계 §1 "AA 재지정 블록").

## 목표와 비목표

### 목표

- `src/styles.css`의 `:root` 토큰 정의(hex 18개 `src/styles.css:12-29`, px 8개 `:30-37`, 그리고 `:8-9`의 `color`/`background` hex 2개)를 전부 제거하고, 색·간격·글자·반경·그림자·모션 값을 `@seed-design/css` 3.0.2의 `--seed-*` 변수에서 받는다. 갈피 고유 custom property 선언은 `--seed-*` 재지정 4+1개(설계 §1)를 제외하고 0개.
- 버튼 전 종(주·보조·위험·텍스트·아이콘·삭제·녹음 정지/버리기)을 `action-button` recipe 클래스로 렌더하고 `setPrimary` 상태 전환이 recipe variant를 따라가게 한다.
- 입력·셀렉트·텍스트에어리어·세그먼트·칩·설정 시트·카드·상태 행은 DOM을 그대로 두고 SEED 토큰으로 다시 칠하며, 흩어진 중복 규칙(`.settings-input`/`.settings-select`/`.settings-textarea`/`.secret-field input`/`.token-field input`/`.number-fields input`/`.participant-row input`/`.glossary-row input`/`.participant-description`)을 한 규칙 목록으로 합친다.
- 루트 `DESIGN.md` §2·§3·§4(및 §6·§7·§8의 값 표현)를 SEED 토큰 이름으로 다시 쓰고 `.issueops/DESIGN.md`가 SEED를 단일 소스로 가리키게 한다. 표와 `styles.css`의 일치를 `src/styles.test.ts`가 지킨다.
- `bun run check`, `bun test`, `bun run build` 통과.

### 비목표 (이슈 "하지 않는 것" 그대로)

- 화면 구성·정보 구조·흐름·카피·아이콘 세트(`@phosphor-icons/web`) 변경, `@seed-design/react`·Stackflow·Tailwind 도입, 다크 모드(`light-only` 고정), 파형 진행 룰 재설계(색만 교체).
- `src/application`, `src/domain`, `src/adapters`, Rust·Python·IPC·워커 JSONL·저장 설정 변경 없음.
- 서드파티 라이선스 고지 파일 신설(Apache-2.0 §4)은 이 이슈 범위 밖이다(위험 R6에서 결정).

## 적용되는 결정과 주의사항

확인한 문서: `.issueops/CONSTITUTION.md`, `.issueops/ARCHITECTURE.md` + `architecture/overview.md`, `.issueops/CONVENTIONS.md` + `conventions/overview.md`, `.issueops/CAUTIONS.md` + `cautions/overview.md` + `cautions/2026-08-23-dom-visibility-is-not-pixel-visibility-grid-row-collapse-hid.md` + `cautions/2026-08-23-tauri-frontend-renders-in-a-plain-browser-but-is-ipc-dead-su.md`, `.issueops/ADR.md` + `adr/overview.md`(결정 1건은 Hexagonal 배치라 CSS와 무관), `.issueops/TESTING.md` + `testing/overview.md`, `.issueops/TECH_STACK.md`, `.issueops/DESIGN.md`, 루트 `DESIGN.md`, `AGENTS.md`, `docs/ARCHITECTURE.md`. `src/ui` selector 계약은 `src/ui/app-view.ts`, `app-template.ts`, `*.dom.test.ts`를 직접 읽어 확인했다.

| 문서 | 항목 | 이 계획에 대한 제약 |
|---|---|---|
| `AGENTS.md` WHERE TO LOOK / UNIQUE STYLES | "UI markup/style contract: `app-template.ts`, `styles.css`, `DESIGN.md` — Keep all three aligned", "`DESIGN.md` is normative" | 세 파일을 한 변경 집합으로 묶고, `styles.css`와 `DESIGN.md` 표 일치를 테스트로 고정한다. |
| `.issueops/DESIGN.md` How changes are verified | 마크업/스타일/루트 DESIGN.md 삼각 정합, 색은 항상 텍스트와 짝, 40px 터치 타깃(커밋 6b39acf), `word-break: keep-all` | 상태 색+텍스트 쌍, `min-height` 40px(`--seed-dimension-x10`), `p`의 `keep-all` 규칙(`styles.css:60-64`)을 유지한다. |
| 루트 `DESIGN.md` §8 Constraints | WCAG 2.2 AA 4.5:1, 보이는 포커스 링, 터치 타깃 40×40 | SEED 기본값이 AA를 못 넘는 4개 시맨틱 변수만 SEED 팔레트 안에서 재지정하고 표에 기록한다. |
| 루트 `DESIGN.md` §6 | `prefers-reduced-motion`은 transform 제거, 상태 전환 즉시 | 갈피 reduced-motion 블록(`styles.css:1700-1709`)을 유지하되 recipe의 클래스 단위 `transition`이 이기므로 `!important`를 붙인다(설계 §6). |
| `.issueops/CONSTITUTION.md` Source of truth·Principles | 1순위는 최신 사용자 지시, 규범 문서는 `DESIGN.md`, 비밀값 금지 | 사용자 지시(리디자인 아님)가 SEED 시각값 채택보다 구조 불변을 우선한다. 토큰 값은 문서에 hex로 복제하지 않는다. |
| `.issueops/CONVENTIONS.md`·`AGENTS.md` CONVENTIONS | TS strict, named export, 더블쿼트, 세미콜론 없음, `ui/`는 `adapters/`·`@tauri-apps/*` import 금지 | 새 `src/ui/seed.ts`는 순수 문자열 헬퍼이며 fence(`scripts/check-architecture.ts` ui 규칙)를 건드리지 않는다. |
| `.issueops/TESTING.md` | `bun run check` + `bun test`; 테스트는 공개 계약(selector·reducer)로 관찰, 내부 구조 고정 금지 | CSS 값 자체를 단언하지 않고 "리터럴 없음/토큰만/표와 일치"라는 계약만 테스트한다. 기존 DOM 테스트 불변. |
| `.issueops/CAUTIONS` 2026-08-23 "DOM visibility is not pixel visibility" | 텍스트·`hidden` 단언만으로 가시성을 믿지 말 것, 그리드 행 붕괴 사례 | `.workspace`의 `grid-template-rows: auto auto minmax(0, 1fr) auto`(`styles.css:244`, `styles.test.ts:31-43`) 유지, 레이아웃 변경 후 수동 게이트에서 배너·시트가 화면에 보이는지 실측한다. |
| `.issueops/CAUTIONS` 2026-08-23 "Tauri frontend renders in a plain browser but is IPC-dead" | 순수 브라우저 QA에서 IPC 의존 동작은 Not Run | 수동 게이트는 `bun run dev`(Tauri 창)를 기본, 순수 브라우저(`bun run vite:dev`)는 정적 시각 항목만. |
| `docs/ARCHITECTURE.md` 변경 집합 §4 "새 UI 상태: 리듀서 + 테스트" | 뷰는 렌더만 | 이번 변경은 새 UI 상태가 아니다. `setPrimary`는 이미 있는 상태를 렌더할 뿐이다. |
| `.issueops/TECH_STACK.md` Frontend | 프런트 의존성 목록은 설정 파일에서 확인한 사실만 | `@seed-design/css` 3.0.2 한 줄을 문서 단계에서 추가한다. |

## 재사용하는 기존 구현

- **`@seed-design/css` 3.0.2** (`base.css`, `recipes/action-button.css`): 토큰과 버튼 recipe를 그대로 쓴다. 새 CSS 시스템을 만들지 않는다. `recipes/*.mjs` 런타임은 쓰지 않는다 — `.mjs`가 `import './action-button.css'` 부수효과를 수행하므로(`recipes/action-button.mjs:1`) 테스트·헬퍼 코드가 CSS 모듈 로딩에 의존하게 된다. 미확인 가정: bun 런타임이 `.css` import를 어떻게 처리하는지는 시험하지 않았고, 이 계획은 그 동작에 의존하지 않는다.
- **`src/ui/app-view.ts:560-563 setPrimary`**: `primary-button`/`secondary-button` 클래스 토글을 그대로 두고(`app-view.dom.test.ts:288 isPrimary`가 의존) recipe variant 교체 한 줄을 더한다.
- **`src/ui/dom.ts required()`와 기존 `element()` 캐시**: 변경 없음.
- **기존 DOM 테스트의 `styles.css` 주입 패턴**(`app-view.dom.test.ts:8-14`, `controller.test.ts:8,42`): happy-dom은 `@import`를 무시한다(스크래치에서 `@import "@seed-design/css/base.css"`가 든 파일로 232개 규칙 파싱·`[hidden]` 계산값 `none` 확인). 그대로 재사용.
- **`src/styles.test.ts`의 정규식 파싱 방식**: 새 계약 테스트도 같은 파일에 같은 방식(`Bun.file(new URL(...)).text()`)으로 추가한다.
- **신규 파일 정당화**: `src/ui/seed.ts`(약 25줄)만 새로 만든다. 이유: recipe 클래스 5개 조합(`seed-action-button` + `--variant_*` + `--size_*` + `--layout_*` + 복합 `--size_*-layout_*`, `recipes/shared.mjs createClassName`과 동일 규칙)을 템플릿 13곳과 동적 빌더 2곳에서 손으로 반복하면 오타가 CSS에 닿지 않아 조용히 깨진다. 테스트가 생성 클래스가 실제 `action-button.css`에 있는지 검사한다.

## 설계

### 1. `src/styles.css` — 토큰 계층 (열린 질문 1·2, 위험 R1, 대안 A1·A3·A4)

**임포트와 모드.**

```css
@import "@seed-design/css/base.css";
@import "@seed-design/css/recipes/action-button.css";
@import "./icons.css";
```

- `index.html:2`의 `<html lang="ko">`에 `data-seed-color-mode="light-only"`를 정적으로 추가한다(`base.css:452-459`가 `color-scheme: light only`와 라이트 팔레트를 적용). SEED `generateThemingScript`는 인라인 스크립트라 Tauri CSP(`src-tauri/tauri.conf.json:26` `default-src 'self'`)에 막히므로 쓰지 않는다. `data-seed-platform`/`data-seed-font-scaling`은 설정하지 않는다(데스크톱, `base.css:38-48`은 ios 전용). 갈피 `:root`의 `color-scheme: light`는 중복이라 제거한다.
- **비레이어 `base.css`/`action-button.css`를 쓴다.** `*.layered.css`는 거부(대안 A1): 갈피의 비레이어 리셋(`button { color: inherit }` `styles.css:71-73`, `button, input { font: inherit }` `:66-69`)이 레이어 recipe를 항상 이겨 버튼 글자색이 상속색으로 조용히 깨진다. 비레이어에서는 클래스 recipe(0,1,0)가 요소 리셋(0,0,1)을 이긴다.
- 스크래치 `vite build`(safari17 + lightningcss)로 확인: 오류 없이 `base + action-button` CSS가 78.5 kB(gzip 9.8 kB). `base.css`와 `action-button.css`에서 `color-mix`·`light-dark`·중첩(`&`)·`@container`·`@starting-style`·`:has`·`@layer`는 grep 0건이고 `:is()`, `scale:`, `clamp()`, `inset`을 쓴다.

**토큰 매핑표 — 색** (왼쪽: 현재 `src/styles.css:12-29`).

| 현재 토큰(hex) | 새 토큰 | 값(SEED 라이트) | 근거 |
|---|---|---|---|
| `--surface-primary` #f7f6f2 | `--seed-color-bg-neutral-muted` | gray-100 #f7f8f9 | 창 캔버스·상태 행·파일 선택·산출물 행 바탕 |
| `--surface-secondary` #efede7 | `--seed-color-bg-neutral-weak` | gray-200 #f3f4f5 | 레일·그룹 컨트롤. 캔버스보다 한 단 어둡다는 현재 명도 순서 유지 |
| `--surface-elevated` #fffefa | `--seed-color-bg-layer-default` | gray-00 #fff | 패널·시트·입력 바탕, 가장 밝은 층 |
| `--surface-inverse` #20201e | `--seed-color-bg-neutral-inverted` | gray-900 #2a3038 | 로그 `pre` |
| `--surface-sunken` #dedbd3 | `--seed-color-bg-neutral-weak-alpha` | black 5% | 세그먼트 트랙(recipe `segmented-control__root`와 같은 값) |
| `--text-primary` #24231f | `--seed-color-fg-neutral` | gray-1000 #1a1c20 | 본문(대비 17:1) |
| `--text-secondary` #666159 | `--seed-color-fg-neutral-muted` | gray-800 #555d6d | 보조 글. `fg-neutral-subtle`(gray-700)은 3.42:1이라 AA 실패 → 쓰지 않는다(6.6:1 / gray-200 위 6.0:1) |
| `--text-inverse` #f7f6f2 | `--seed-color-fg-neutral-inverted` (로그), `--seed-color-fg-on-brand-solid` (칩 선택) | white | 13.3:1 / 5.8:1 |
| `--border-default` #d8d4ca | `--seed-color-stroke-neutral-weak` | gray-400 #dcdee3 | 입력·구분선. SEED `text-input` outline과 동일 토큰 |
| `--border-subtle` #e8e4da | `--seed-color-stroke-neutral-muted` | black 약 6% | 면 분리 얇은 선. SEED의 구분선 토큰 중 `-weak`(gray-400) 다음으로 진해 현재 default/subtle 두 단계 위계를 유지한다(`-subtle`은 약 5%로 `-muted`와 거의 같아 단계가 사라진다) |
| `--accent-primary` #b75b37 | `--seed-color-bg-brand-solid`(채움), `--seed-color-fg-brand`(아이콘·숫자·글), `--seed-color-stroke-brand-solid`(호버 테두리) | AA 재지정 후 carrot-800 #b93901 / 테두리 carrot-700 | 현재 4.56:1 → 재지정으로 5.76:1 |
| `--accent-hover` #98482c | `--seed-color-bg-brand-solid-pressed` | 재지정 후 carrot-900 #862b00 | 눌림/호버. recipe가 같은 변수를 읽는다 |
| `--accent-on-primary` #fffefa | `--seed-color-fg-on-brand-solid` | white | |
| `--accent-text` #98482c | `--seed-color-fg-brand` | 재지정 후 carrot-800 | eyebrow·단계 라벨·텍스트 버튼. 흰 배경 5.76:1, gray-200 위 5.23:1 |
| `--status-success` #3f7356 | `--seed-color-fg-positive-contrast`(글), `--seed-color-bg-positive-solid`(체크 원), `--seed-color-fg-on-positive-solid` | green-900 8.89:1 / green-700 | `fg-positive`(green-700)는 3.96:1이라 글에는 `-contrast` 단계를 쓴다 |
| `--status-warning` #8a5e1c | `--seed-color-fg-warning-contrast` | yellow-900 #4f3e1f 10.3:1 | `fg-warning`(yellow-700)은 4.12:1(gray-200 위 3.74:1) 실패 |
| `--status-error` #a63f3f | `--seed-color-fg-critical-contrast`(글), `--seed-color-bg-critical-solid`(녹음 점), `--seed-color-stroke-critical-solid`(배너 좌측선), `--seed-color-bg-critical-weak`(배너/오류 배경) | red-900 #921708 8.9:1(red-100 위 8.05:1) | 기존 `color-mix(error 8%/12%)` 배경을 `bg-critical-weak`로 |
| `--focus-ring` #246b9b | `--seed-color-stroke-focus-ring` | AA 재지정 후 blue-700 #217cf9 | 기본 blue-600은 흰 배경 2.84:1(< 3:1). 재지정 후 3.95:1 |

hex가 아닌 파생 색: `color-mix(accent 6%/9%/5%…)` 틴트 → `--seed-color-bg-brand-weak`(carrot-100, 녹음기·진행 카드·파일 아이콘 바탕), `color-mix(accent 24%/22% …, border)` → `--seed-color-stroke-brand-weak`(carrot-300), 오버레이 `color-mix(inverse 52%)` → `--seed-color-bg-overlay`, 로그 `rgb(…)` 그림자 전부 → SEED 그림자 토큰. 반투명이 의미인 두 곳(topbar `92%` `styles.css:255`, 레일 현재 단계 `70%` `:160`)만 `color-mix(in srgb, var(--seed-color-…) N%, transparent)`로 두며 입력은 SEED 변수다(hex·rgb 리터럴 0).

**AA 재지정 블록** (단일 위치, 파일 상단 임포트 직후):

```css
/* SEED 시맨틱 토큰 중 흰/회색 배경에서 WCAG AA를 못 넘는 쌍만, 같은 SEED 팔레트의
   더 짙은 단계로 다시 가리킨다. 값을 새로 만들지 않는다. DESIGN.md §2 "AA 재지정" 표 참조. */
:root[data-seed-color-mode="light-only"] {
  --seed-color-bg-brand-solid: var(--seed-color-palette-carrot-800);
  --seed-color-bg-brand-solid-pressed: var(--seed-color-palette-carrot-900);
  --seed-color-fg-brand: var(--seed-color-palette-carrot-800);
  --seed-color-stroke-focus-ring: var(--seed-color-palette-blue-700);
  --seed-feedback-scale: var(--seed-scale-s98);
}
```

선택자는 `:root[data-seed-color-mode="light-only"]`(특이도 (0,2,0))로 `base.css:583`의 같은 선택자와 동점이며, 소스 순서로 이기도록 이 블록을 세 `@import` **뒤**에 둔다(`@import`는 항상 앞서므로 갈피 규칙은 그 뒤에 온다). `html[data-seed-color-mode="light-only"]`는 (0,1,1)이라 `:root[…]`(0,2,0)에 져서 쓰면 안 된다 — 독립 검토가 headless Chrome에서 `--seed-color-bg-brand-solid`가 `#f60`(기본값) 그대로임을 계산값으로 확인했다. 이 계획의 이전 판본이 한 "스크래치 빌드 산출물의 선언 순서만으로 승리 판단"은 철회하며, 승리 여부는 브라우저 계산값(`getComputedStyle(document.documentElement).getPropertyValue("--seed-color-bg-brand-solid")`가 carrot-800으로 풀림)으로 검증한다(게이트 G16·G17, 테스트 ⑧). 마지막 줄은 recipe의 `:active { scale: var(--seed-feedback-scale) }`(`action-button.css`)가 JS 없이도 눌림 피드백(현재 `scale(0.98)` `styles.css:887-890`)을 내게 하며, `--seed-scale-s98`은 `prefers-reduced-motion`에서 SEED가 1로 낮춘다(`base.css:1029-1035`).

**타이포.**

| 현재 | 새 토큰 쌍(글자/줄간격) |
|---|---|
| 10px(12곳), 11px(19곳) | `--seed-font-size-t1` / `--seed-line-height-t1` (11px; 10px 캡션이 11px가 된다 — `DESIGN.md` 표에서 Body/sm과 Caption은 크기가 같고 굵기·자간·대문자로 구분) |
| 12px(17) | `t2` |
| 13px(13) | `t3` |
| 14px(2) | `t4` |
| 17px(`.setup-progress-card h3`), 18px(2), 19px(brand) | `t6`(18px) |
| 20px(1) | `t7` |
| 21px(`.section-heading h2`) | `t8`(22px) |
| 24px(2) | `t9` |
| `clamp(24px, 3vw, 36px)`(`.topbar h1`) | `clamp(var(--seed-font-size-t9), 3vw, var(--seed-font-size-t12))` (24~32px) |
| 굵기 700/600/500/400 | `--seed-font-weight-bold`(600도 bold로 올림)/`-medium`/`-regular` |
| 줄간격 1.12/1.3/1.5/1.55/1.6/1.65 | 같은 `tN`의 `--seed-line-height-tN` (한글 본문 줄간격이 약간 촘촘해진다: 13px은 1.55→1.38) |
| 서체 | `font-family: var(--seed-font-family)`; 모노 서체(`"SFMono-Regular", Menlo, monospace`)는 SEED에 대응 토큰이 없어 유지, 크기는 `font: var(--seed-font-size-t1) / var(--seed-line-height-t1) …`로 토큰화 |
| `letter-spacing` | SEED 토큰 없음(`base.css` 0건). `em` 리터럴 유지 |

SEED의 글자 크기는 `rem` 기반(`.8125rem` 등, `base.css` 470대)이라 문서 루트 `font-size` 16px을 바꾸지 않는다.

**간격/반경/그림자/모션.**

| 대상 | 규칙 |
|---|---|
| `--space-1,2,3,4,5,6,8,10` (4/8/12/16/20/24/32/40) | `--seed-dimension-x1,x2,x3,x4,x5,x6,x8,x10` 정확 일치 |
| `padding`/`margin`/`gap` px 리터럴 | 가장 가까운 SEED 단계, 동률이면 큰 쪽: 2→`x0_5`, 3→`x1`, 5→`x1_5`, 6→`x1_5`, 7→`x2`, 9→`x2_5`, 10→`x2_5`, 11→`x3`, 13→`x3_5`, 14→`x3_5`, 15→`x4`, 18→`x4_5`, 19→`x5`, 26→`x7`, 28→`x7`, 30→`x8`, 56→`x14` (최대 2px 이동) |
| `min-height: 40px` 등 단계와 같은 크기 | `--seed-dimension-x10` 등 토큰(터치 타깃). 248px 레일, 560px 시트, 132px 칩 영역 같은 구조 치수는 리터럴 유지 |
| `border-radius` | 4→`r1`, 7·8→`r2`, 9→`r2_5`(입력류는 `r2`로 SEED `text-input` medium과 동일), 10→`r2_5`, 11·12→`r3`, 14→`r3_5`, 16→`r4`, `999px`→`--seed-radius-full`, `50%`/`22%`(brand-mark 스퀴클)은 유지 |
| 그림자(`rgb(78 50 23 / …)` 전부) | 패널·칩 Rest → `--seed-shadow-s1`, 시트·팝오버·부유 → `--seed-shadow-s3`, brand-mark → `--seed-shadow-s1` |
| `transition`/`animation` | 100·120ms→`--seed-duration-d2`, 160ms→`d3`, 180ms→`d4`, 320ms→`d6`; `ease-out`→`--seed-timing-function-easing`; 파형 clip-path의 `cubic-bezier(0.16, 1, 0.3, 1)`→`--seed-timing-function-enter-expressive`; 눌림 `scale(0.96/0.98)`→`scale: var(--seed-scale-s97/s98)`. `recording-pulse 1.6s`는 대응 토큰이 없어 유지(무한 루프) |
| `border: 1px` | 유지(recipe도 리터럴 1px 사용) |
| `opacity: 0.48` (비-recipe 비활성) | 유지. SEED는 `bg-disabled`/`fg-disabled` 색으로 표현하므로 recipe 버튼에는 쓰지 않는다 |

**글로벌 규칙.**
- `html [hidden] { display: none }`(`:44-46`)는 그대로. recipe `.seed-action-button { display: inline-flex }`(0,1,0)보다 `html [hidden]`(0,1,1)이 높아 `hidden` 버튼이 계속 숨는다(`#cancel-button`, `#attendee-clear`, `#open-minutes-button`).
- `p { overflow-wrap: break-word; word-break: keep-all; line-break: strict }`(`:60-64`) 및 `.settings-section p`(`:692-695`) 유지.
- 전역 포커스 링 `button:focus-visible, input:focus-visible, summary:focus-visible`(`:75-80`, 3px/3px)은 `outline: var(--seed-dimension-x0_5) solid var(--seed-color-stroke-focus-ring); outline-offset: var(--seed-dimension-x0_5)`로 바꿔 recipe의 포커스 링(`.seed-action-button:is(:focus-visible …)`)과 같은 2px/2px로 맞춘다. `select`·`textarea`·`.segmented-control input:focus-visible + span`·`.participant-chip:focus-within`도 같은 토큰을 쓴다.
- `button:disabled { cursor: not-allowed; opacity: .48 }`(`:892-895`)은 `button:disabled:not(.seed-action-button)`로 좁힌다(recipe 버튼은 SEED `bg-disabled`/`fg-disabled`로 표현되므로 이중 감쇠 방지).
- reduced-motion 블록(`:1700-1709`)은 `transition-duration`·`animation-duration`·`animation-iteration-count` 셋 모두에 `!important`를 붙인다(recipe의 `.seed-action-button { transition: … }` 클래스 규칙이 `*` 규칙을 이기기 때문). 현재 블록은 `animation-iteration-count: 1`을 `!important` 없이 두어 `.recording-dot`의 `animation: recording-pulse 1.6s ease-in-out infinite`(`styles.css:993`, 단축 속성이 `*` 규칙을 이김)가 duration 0.01ms로 **무한 반복**되며 오히려 깜빡인다. `animation-iteration-count: 1 !important`로 한 번만 재생되게 고정한다(게이트 G14가 녹음 점 정지를 확인).
- `.text-button` 본체 규칙을 recipe로 대체하더라도 현재 `margin-top: 10px`(`styles.css:840`)은 레이아웃 간격이므로 `.text-button { margin-top: var(--seed-dimension-x2_5) }` 한 줄을 유지한다(`.participant-picker-header .text-button { margin-top: 0 }` `:1243-1245` 재정의도 유지). 없으면 `토큰 설정 열기`·`모델 이용 조건 페이지 열기`·`참석자 추가`·`용어 추가` 버튼이 위 문단에 붙는다(게이트 G18).

### 2. 버튼 → `action-button` recipe (`src/ui/seed.ts`, `app-template.ts`, 동적 빌더)

recipe는 `<button>` 하나에 클래스만 붙이면 되고(슬롯 없음) 크기는 `size_xsmall`=32px 필 / `medium`=40px(r2, 14px 굵게) 이다(`action-button.css` size 규칙).

| 갈피 클래스(유지) | recipe 클래스 조합 | 추가 갈피 규칙 |
|---|---|---|
| `.primary-button` | `variant_brandSolid` / `size_medium` / `layout_withText` | 없음. 이전 `.primary-button`/`.secondary-button` 본체 규칙(`:859-905`) 삭제 |
| `.secondary-button` | `variant_neutralOutline` / medium / withText | 없음 |
| `.secondary-button.danger` | 위와 같음 | `.secondary-button.danger:not(:disabled) { color: var(--seed-color-fg-critical-contrast) }` — recipe에 critical outline이 없어 neutralOutline+글자색만 위험색 |
| `.record-stop` / `.record-discard` | neutralOutline / `size_xsmall` / withText | `.record-stop:not(:disabled)`에 critical 글자색 |
| `.text-button`, `.token-guide-trigger`, `.field-label button`(출력 폴더 `변경`), `.artifact-row button`(`열기`) | `variant_ghost` / medium / withText | `--seed-box-color: var(--seed-color-fg-brand)`(ghost의 공개 색 훅, recipe가 `color: var(--seed-box-color)`로 읽음). 40px 터치 타깃을 medium이 보장 |
| `.settings-button`, `.settings-close-button`, `.secret-visibility-button` | neutralOutline / medium / `layout_iconOnly` | 없음 |
| `.glossary-remove`, `.participant-remove` | ghost / medium / iconOnly | `:hover` 빨간색 규칙(`:1384-1387`, `:1419-1422`)은 `--seed-box-color`로 이식 |
| `.token-guide-header button`(`토큰 발급 안내` 닫기) | ghost / `size_xsmall` / iconOnly | |

복합 카드(`.record-button`, `.file-picker`)는 recipe 대응이 없으므로 토큰만 쓴다(비활성은 `opacity: .48`).

`src/ui/seed.ts` (named export, 더블쿼트, 세미콜론 없음):

```ts
export type ActionButtonVariant = "brandSolid" | "neutralOutline" | "ghost"
export type ActionButtonSize = "xsmall" | "medium"
export type ActionButtonLayout = "withText" | "iconOnly"

/** `@seed-design/css/recipes/action-button`의 클래스 규칙(shared.mjs createClassName)과 같다. */
export function actionButtonClass(
  variant: ActionButtonVariant,
  size: ActionButtonSize = "medium",
  layout: ActionButtonLayout = "withText",
): string {
  const root = "seed-action-button"
  return [
    root,
    `${root}--variant_${variant}`,
    `${root}--size_${size}`,
    `${root}--layout_${layout}`,
    `${root}--size_${size}-layout_${layout}`,
  ].join(" ")
}

/** variant 클래스만 교체하고 나머지 클래스는 건드리지 않는다. */
export function setActionButtonVariant(button: HTMLElement, variant: ActionButtonVariant): void
```

적용 지점(클래스 이름 변경만, DOM 구조·id·`data-*`·`aria-*` 불변):
- `src/ui/app-template.ts`: 파일 상단에서 `actionButtonClass` import 후 템플릿 리터럴에 `class="primary-button ${actionButtonClass("brandSolid")}"` 식으로 보간. 대상 줄: 24, 45, 58, 63, 84, 85, 96, 105, 108(내부 `text-button`), 134, 139, 147-149, 152, 163, 178, 182, 185, 186, 198, 219, 223, 225, 237, 246, 255, 265, 295, 296. `.artifact-row button`/`.field-label button`은 현재 클래스가 없으므로 클래스 속성을 새로 추가한다(selector가 아니라 클래스 추가만, `data-action`과 `aria-label` 유지).
- `src/ui/app-view.ts:560-563 setPrimary`: 기존 두 `classList.toggle`을 유지하고 `setActionButtonVariant(button, primary ? "brandSolid" : "neutralOutline")`를 추가한다. 호출부(`:503`, `:506`)와 `refreshActions` 논리는 불변. 주의: `prepare-button`은 이미 `textContent` 대입으로 아이콘이 사라지는 기존 동작(`:154-156`)이 있고 이 변경과 무관하다.
- `src/ui/glossary-settings.ts:68`, `src/ui/participant-settings.ts:86`: `remove.className = \`glossary-remove ${actionButtonClass("ghost", "medium", "iconOnly")}\``. `.glossary-row`/`.participant-row` 셀렉터와 테스트가 쓰는 `.glossary-term`, `.participant-name`, `.participant-team`, `.participant-role`, `.participant-aliases`, `.glossary-description` 클래스는 불변.

### 3. 입력·셀렉트·텍스트에어리어 — 토큰 기반 (위험 R2, 대안 A2)

- `text-input` recipe는 `input`을 `div.seed-text-input__root` 안에 넣고(`.seed-text-input__value:is(input) { width: 0 }`, root가 `::after` 테두리를 그림), 포커스·invalid를 root의 `[data-focus]`/`[data-invalid]`에 의존한다. 정적 입력 12개와 동적 빌더 입력 6종(`participant-settings.ts:107-`, `glossary-settings.ts`)에 wrapper를 넣고 JS로 상태 속성을 동기화해야 한다 → 완료 기준 3 위반. 입력류는 recipe의 `variant_outline`/`size_medium` 값을 SEED 토큰으로 그대로 옮긴다: `box-shadow: inset 0 0 0 1px var(--seed-color-stroke-neutral-weak)`, `border-radius: var(--seed-radius-r2)`, `min-height: var(--seed-dimension-x10)`, 포커스는 전역 링, 비활성은 `--seed-color-bg-disabled`/`--seed-color-fg-disabled`, placeholder는 `--seed-color-fg-neutral-subtle`(`fg-placeholder`는 2.1:1이라 제외).
- 규칙 통합: `.token-field input, .number-fields input, .secret-field input, .settings-input, .settings-select, .settings-textarea, .participant-row input, .glossary-row input, .participant-description`의 공통 선언(글자색·배경 `--seed-color-bg-layer-default`·테두리·반경·폰트 상속)을 한 목록 규칙으로 하고, 크기만 개별(`42px→x10`, 번호 입력 `36px→x9`, 행 입력 `font-size: t3`)로 남긴다. 선택자는 전부 유지.
- 네이티브 `<select>`는 recipe(`select-trigger`는 커스텀 팝업 셀렉트)와 호환되지 않으므로 같은 입력 규칙을 쓴다.

### 4. 세그먼트·칩 — 토큰 기반

- `.segmented-control`(`styles.css:1167-1222`, `app-template.ts:111-115, 206-209`: `label > input[type=radio] + span`): `segmented-control` recipe는 `__indicator` 요소 + `--segment-index`/`--segment-count` JS 변수가 필요하고 `__item`이 `min-width: 86px`, `padding-inline: x6`, 16px 굵은 글씨라 280px 열의 3분할(`.transcription-grid` `minmax(280px, 0.8fr)`, `styles.css:396-402`)에서 넘친다. 그래서 recipe 시각값을 토큰으로 옮긴다: 트랙 `background: var(--seed-color-bg-neutral-weak-alpha)`, 선택 슬롯 `background: var(--seed-color-bg-layer-default); box-shadow: inset 0 0 0 1px var(--seed-color-stroke-neutral-muted)`, 반경 `r2_5`/`r2`, 라벨 `t2` bold, `min-height: x10`. 포커스는 `input:focus-visible + span`(라디오가 `opacity: 0`이라 span에 그린다 `styles.css:1197-1200`)에 전역 링 토큰.
- `.participant-chip`(`:1259-1310`): `chip` recipe의 선택 상태는 `neutral-solid`(검정)이거나 회색 outline이라 현재 "선택 = 강조색 채움 + 체크 글리프"(루트 `DESIGN.md:137`)를 바꾼다. 그래서 선택 상태는 `data-selected="true"`(`participant-picker.ts:52-76`이 유지) 위에서 `background: var(--seed-color-bg-brand-solid); color: var(--seed-color-fg-on-brand-solid); border-color: var(--seed-color-bg-brand-solid)`, 비선택은 `bg-layer-default` + `stroke-neutral-muted` + `fg-neutral-muted`.

### 5. 설정 시트·카드·상태 — 토큰 기반

- `.settings-dialog`(`:578-587`) 바탕 → `--seed-color-bg-overlay`; `.settings-sheet`(`:589-599`) → `bg-layer-default`, `stroke-neutral-weak`, `--seed-radius-r4`, `--seed-shadow-s3`. `dialog` recipe는 positioner/backdrop/content/header/title/footer 슬롯과 `word-break: break-all`을 요구해 거부(마크업 추가 + 한글 계약 충돌). `.settings-section`, `.token-field`, `.speaker-panel`, `.path-display`는 `bg-neutral-weak`.
- `.panel`(`:357-365`), `.engine-chip`, `.status-row`, `.artifact-row`, `.file-picker`, `.recorder`, `.setup-progress-card`, `.error-message`, `.app-error`, `.job-summary i`, `.recording-dot`, `.wave-progress`: 색·반경·그림자·간격을 위 표대로 치환. 파형 룰은 구조 불변: 트랙 `repeating-linear-gradient(90deg, transparent 0 5px, var(--seed-color-stroke-neutral-weak) 5px 7px)`(현재 `color-mix(border-default 72%)`), 채움 `var(--seed-color-bg-brand-solid)`, `clip-path: inset(0 calc(100% - var(--progress)) 0 0)`와 `--progress` JS 변수(`app-view.ts:421`)는 유지.
- `data-state`/`data-status`/`data-selected`/`data-warning`/`data-visible` 선택자(`.status-row[data-state="ready"]`, `.setup-progress-card[data-status="completed"]`, `.recorder[data-warning="true"]`, `.settings-message[data-state=…]` 등)는 이름·구조 전부 유지하고 색만 바뀐다.
- `.workspace`의 `grid-template-rows: auto auto minmax(0, 1fr) auto`(`:244`), `.app-shell`의 `248px minmax(0, 1fr)`(`:84`), 960px 미디어쿼리(`:1685-1698`)는 그대로 둔다.

### 6. 테스트·문서 계약

- `src/styles.test.ts`에 계약 테스트 추가(기존 3개 중 21px 단언만 갱신): ① `@import "@seed-design/css/base.css"`와 `…/recipes/action-button.css` 존재, ② hex(`#[0-9a-fA-F]{3,8}\b`)·`rgb(`·`hsl(` 리터럴 0, ③ custom property 선언은 전부 `--seed-` 접두, `--space-N`류 정의 0, ④ `padding|margin|gap|font-size|border-radius` 선언에 px 리터럴 0(`clamp`·`calc` 안의 `var()` 포함 값만 허용), `ms` 리터럴은 reduced-motion 블록의 `0.01ms`만, ⑤ 루트 `DESIGN.md`의 `--seed-*` 이름 집합 = `src/styles.css`의 `--seed-*` 이름 집합, ⑥ reduced-motion 블록·`word-break: keep-all` 규칙 존재.
- 루트 `DESIGN.md` §7 모순 정리(테스트 ⑦): `DESIGN.md:211`의 "whisper-level warm shadows"와 `:220`의 "No glass blur, outer glow, or pure black shadow"는 SEED 그림자(`--seed-shadow-s1/s3`, `base.css`에서 순수 검정 알파 `#00000014`/`#0000001f`)와 모순된다. 아래 개정 목록의 §7 항목에서 두 문장을 "SEED 그림자 토큰만 쓴다(`--seed-shadow-s1` 면, `--seed-shadow-s3` 부유)", "유리 블러·외곽 글로우 없음"으로 다시 쓰고, §2 Rules의 "warm tonal shifts"(`:43`)와 §8 Accepted Debt의 "warm-light workspace"(`:237`)도 같이 고친다. 테스트 ⑦은 `DESIGN.md`에 `warm`·`pure black` 문자열이 0건임을 단언한다(게이트 G19).
- 테스트 ⑧(AA 재지정 단언): `src/styles.test.ts`는 `:root[data-seed-color-mode="light-only"] {` 블록이 정확히 1개이고 `html[data-seed-color-mode` 선택자는 0개이며, 그 블록이 모든 `@import` 줄보다 **뒤**에 있음을 단언한다. happy-dom은 `@import`를 풀지 않으므로 실제 승리 여부(계산값)는 게이트 G17(수동)로 확인한다.
- 루트 `DESIGN.md` 개정: §2 Palette 표를 `Role | Token | SEED palette | Usage`(hex 값 열 삭제, 값은 SEED가 소유)로 다시 쓰고 "AA 재지정" 표 4행 + `--seed-feedback-scale` 한 줄을 추가, Rules의 "Surfaces use warm tonal shifts. New colors must be added here first"를 "새 색은 SEED 토큰에서만 가져온다"로; §3 Scale 표를 `Level | Size token | Weight token | Line-height token | Usage`로(§3 위 설명 "px-locked" 문구 제거), Font Stack은 `var(--seed-font-family)`와 모노 스택; §4 Base Unit 표를 `--seed-dimension-x*`로; §5 Status Button의 "primary, secondary, quiet, destructive" 변형을 recipe variant 이름(`brandSolid`/`neutralOutline`/`ghost`)과 `danger` 글자색 규칙으로, Glossary Motion "100ms press scale, 180ms…"를 SEED 토큰으로; §6 Motion 표를 `d2/d4/d6` + easing 토큰으로; §7 Depth를 `--seed-shadow-s1/s3`·`--seed-radius-*`로("Cards use 14px radius; inputs 10px; primary buttons are pills" 문구도 토큰·SEED recipe 값으로 교체); §8 Accepted Debt의 "warm-light workspace" 문구를 "SEED 라이트 단색 작업 공간(`light-only`)"로.
- `.issueops/DESIGN.md`: 15행 "src/styles.css … authoritative over the root DESIGN.md typography table"을 "`@seed-design/css` `base.css`가 값의 단일 소스이며 `src/styles.css`는 `--seed-*`를 참조만 한다. 루트 `DESIGN.md` 표는 사용 중인 토큰 이름을 나열하고 `src/styles.test.ts`가 일치를 검증한다"로 바꾸고, "Before adding a color, spacing, or motion value" 항목에 "SEED 토큰만, 새 hex·px 금지, `@seed-design/css` 버전 변경 시 수동 게이트 재실행", Detected client surface에 `@seed-design/css` 3.0.2를 추가.
- `AGENTS.md` UNIQUE STYLES에 한 줄("스타일 값은 `@seed-design/css` 토큰만; 새 hex·px 토큰 정의 금지"), `.issueops/TECH_STACK.md` Frontend에 `@seed-design/css 3.0.2`(비-React CSS, Apache-2.0) 한 줄. 이슈 범위(DESIGN.md 2개)는 넘지 않는 문서 일관성 갱신이며 기능·동작을 넓히지 않는다.

### 7. 계층별 영향 요약

| 계층 | 변경 |
|---|---|
| domain / application / adapters / composition (TS·Rust) | 없음 |
| worker (Python) | 없음 |
| frontend `ui` | `src/ui/seed.ts`(신규), `app-template.ts`·`app-view.ts`·`glossary-settings.ts`·`participant-settings.ts` 클래스 이름, 선택자·`data-*`·id 계약 불변 |
| 스타일 | `src/styles.css` 전면 토큰 치환, `index.html` 속성 1개, `package.json`·`bun.lock` |
| 문서/테스트 | `DESIGN.md`, `.issueops/DESIGN.md`, `.issueops/TECH_STACK.md`, `AGENTS.md`, `src/styles.test.ts`, `src/ui/seed.test.ts`(신규), `app-view.dom.test.ts`(테스트 추가만) |

## 성능 영향

- **핫 패스**: 아님. 런타임 JS 경로는 `setPrimary`가 클래스 한 개를 더 바꾸는 정도(상태 전환당 O(1), `refreshActions` 호출 빈도 그대로)이고 렌더 루프·IPC·워커 경로는 건드리지 않는다.
- **복잡도**: 최초 CSS 파싱량이 늘어난다. 스크래치 `vite build`(safari17, lightningcss 압축)에서 `base.css`+`action-button.css`는 78.5 kB(gzip 9.8 kB). 현재 `dist/assets/index-Dib6wulI.css`는 27.2 kB이므로 합계 약 105 kB다(갈피 규칙이 토큰 치환으로 줄거나 늘 수 있어 정확한 값은 측정 대상이다). 로컬 번들(`frontendDist: ../dist`)이라 네트워크 영향은 없고 파싱은 한 번이다. 호버·눌림은 `scale`/배경색 전환뿐이라 레이아웃 비용이 없다. 쓰는 recipe만 개별 임포트하고 `all.css`(466 kB)는 쓰지 않는다.
- **측정 계획**: (a) 작업 전후 `bun run vite:build` 출력의 CSS 크기 비교(상한 140 kB, 게이트 G9), (b) 수동으로 설정 시트 열기·닫기와 전사 중 파형 진행을 눈으로 보아 끊김이 없는지 확인(게이트 G12·G13), (c) `bun test` 총 시간이 현재 대비 눈에 띄게 늘지 않는지 확인(DOM 테스트가 `styles.css` 원문을 happy-dom에 주입하며 `@import`는 무시된다).

## 하위 호환성과 side effect

- **IPC 명령·이벤트 계약, 워커 JSONL 프로토콜, Zod 스키마**: 변경 없음. `src/adapters`, `src/domain`, `src/application`, `src-tauri`, `worker` 무수정.
- **저장 설정·키체인**: 변경 없음. 설정 값·키체인 항목·출력 파일 이름(`.aligned.v2.json`, `.srt`, `_화자별.txt`)은 UI 스타일과 무관하다. 시각 변화만 있다: 베이지 따뜻한 톤 → SEED 중립 회색·당근 오렌지(AA 재지정 단계), 버튼 필 → 8px 반경(`xsmall`은 필), 보조 글 크기 단계 이동(10→11px, 17→18px, 21→22px) — 이슈가 수용한 값 변화이며 레이아웃·흐름·카피는 불변.
- **DOM 계약**: id, `data-action`, `data-state`/`data-status`/`data-selected`/`data-step-state`/`data-status-label`/`data-status-value`/`data-phase`/`data-setup-phase`, `input[name="speaker-mode"|"engine-preset"]`, `.participant-chip input`, `.participant-row`, `.glossary-row` 등 `app-view.ts`·`*.dom.test.ts`가 쓰는 selector 전부 불변. 클래스는 추가만 하고(`primary-button`/`secondary-button`/`text-button` 등 기존 클래스 유지) 제거는 하지 않는다.
- **CSP**: `src-tauri/tauri.conf.json:26`의 `style-src 'self' 'unsafe-inline'`로 충분하다(SEED CSS에 `url()` 0건, 인라인 스크립트 미사용).
- **롤백**: `git revert` 한 번으로 복귀(`package.json`/`bun.lock`/`index.html`/`styles.css`/문서 일괄). 데이터·마이그레이션 없음. SEED 버전 상승 시 클래스 이름이 바뀌면 `seed.test.ts`가 `node_modules`의 `action-button.css`와 대조해 실패한다.
- **라이선스**: `@seed-design/css`는 Apache-2.0(`NOTICE` 동봉)이다. 배포 DMG 안 CSS에 대한 귀속 고지는 위험 R6에서 다룬다.

## 구현 순서

모든 단계는 실패 테스트를 먼저 쓴다(RED → GREEN). `bun test`는 단계마다 해당 파일만, 마지막에 전체.

1. **계약 테스트 RED** — `src/styles.test.ts`에 ①~⑧(설계 §6)을 추가하고(⑤·⑦의 `test` 이름에 문자열 `DESIGN.md`를 넣어 게이트 G8의 `-t` 필터가 잡게 한다), 기존 21px 단언(`:28`)을 `/font-size:\s*var\(--seed-font-size-t8\)/u`로 갱신한다. 이때 전부 실패해야 한다(`styles.css`에 `@import`·토큰이 없고 hex 20줄이 남음, `DESIGN.md`에 `warm`·`pure black`이 남음). `bun test src/styles.test.ts` 실패 확인 후 진행.
2. **의존성·임포트·모드 GREEN(① 일부)** — `bun add -E @seed-design/css@3.0.2`(package.json의 정확 핀 관례, `dependencies`: `@phosphor-icons/web`도 CSS 자산이라 같은 칸), `index.html`에 `data-seed-color-mode="light-only"`, `styles.css` 상단에 임포트 3줄 + AA 재지정 블록 추가. `:root`의 갈피 토큰 38행(`:12-37`)과 `color`/`background` hex(`:8-9`), `color-scheme` 삭제 후 `font-family: var(--seed-font-family)`, `color: var(--seed-color-fg-neutral)`, `background: var(--seed-color-bg-neutral-muted)`.
3. **색·간격·타이포·반경·그림자·모션 치환 GREEN(②③④)** — 설계 §1 표대로 `src/styles.css` 전체를 위에서 아래로 섹션 단위(레일 → 톱바 → 패널 → 입력 → 설정 시트 → 칩 → 진행 카드 → 산출물 → 푸터 → 미디어쿼리) 치환. 각 섹션 후 `bun test src/styles.test.ts`로 남은 리터럴을 확인한다. `--space-N` 참조, `color-mix` 틴트(→ `bg-brand-weak`/`stroke-brand-weak`/`bg-critical-weak`), `rgb(78 50 23 / …)` 그림자(→ `--seed-shadow-s1/s3`)를 함께 제거. 입력류 규칙 통합(설계 §3), 세그먼트·칩(설계 §4), 시트·카드(설계 §5), 전역 포커스·비활성·reduced-motion(`!important`) 조정(설계 §1 마지막 목록).
4. **버튼 헬퍼 RED→GREEN** — `src/ui/seed.test.ts`(신규): (a) `actionButtonClass("brandSolid")`가 정확히 `seed-action-button seed-action-button--variant_brandSolid seed-action-button--size_medium seed-action-button--layout_withText seed-action-button--size_medium-layout_withText`, (b) 사용하는 variant×size×layout 조합의 모든 클래스가 `node_modules/@seed-design/css/recipes/action-button.css`에 선택자로 존재, (c) `setActionButtonVariant`가 variant 클래스만 교체. → `src/ui/seed.ts` 구현.
5. **상태→recipe 연결 RED→GREEN** — `src/ui/app-view.dom.test.ts`에 **테스트 추가만**(기존 줄 무수정): `#start-button`/`#refine-button`이 `renderResult`/`renderMinutes` 이후 `primary-button`과 함께 `seed-action-button--variant_brandSolid` ↔ `secondary-button`과 함께 `seed-action-button--variant_neutralOutline`으로 바뀌는지, 설정 시트 동적 빌더가 만든 `.glossary-remove`/`.participant-remove`에 `seed-action-button--layout_iconOnly`가 붙는지. → `app-template.ts`·`app-view.ts setPrimary`·`glossary-settings.ts`·`participant-settings.ts` 변경(설계 §2). 이어서 `.primary-button`/`.secondary-button` 본체 규칙·`.settings-button` 계열·`.text-button`·`.glossary-remove`/`.participant-remove` 본체 규칙을 삭제하고 §2 표의 추가 갈피 규칙만 남긴다. 단 `.text-button`은 본체를 지우되 `margin-top: var(--seed-dimension-x2_5)` 한 줄은 남긴다(설계 §1 글로벌 규칙, 게이트 G18).
6. **문서 RED→GREEN(⑤⑦)** — 단계 1의 ⑤(DESIGN.md ↔ styles.css 토큰 집합 일치)와 ⑦(`warm`·`pure black` 0건)이 이 시점에 여전히 실패하는 것을 확인 → `DESIGN.md` §2·§3·§4·§6·§7·§8(§7의 "warm shadows"·"pure black shadow" 두 문장 포함), `.issueops/DESIGN.md`, `.issueops/TECH_STACK.md`, `AGENTS.md`를 갱신해 GREEN(설계 §6). 토큰 이름 목록은 `grep -o -- '--seed-[a-z0-9_-]*' src/styles.css | sort -u`에서 만든다.
7. **전체 검증과 수동 게이트** — `bun run check`, `bun test`, `bun run vite:build`(CSS 크기), `bun run build`, 수동 게이트 G12~G15·G17 실행 후 스크린샷 비교 메모와 G17 계산값을 PR 본문에 남긴다.

## 게이트

G1: `@seed-design/css`가 정확히 3.0.2로 추가되고 lockfile에 반영됨 | CHECK: `grep -c '"@seed-design/css": "3.0.2"' package.json && grep -c '@seed-design/css' bun.lock` | EXPECT: 첫 줄 `1`, 둘째 줄 `1` 이상
G2: `styles.css`가 `base.css`와 `action-button.css`를 불러오고 `light-only` 모드가 고정됨 | CHECK: `grep -cE '^@import "@seed-design/css/(base|recipes/action-button)\.css";' src/styles.css && grep -c 'data-seed-color-mode="light-only"' index.html` | EXPECT: `2` 그리고 `1`
G3: 갈피 고유 hex·px 토큰 정의·원색 리터럴이 0이고 custom property 선언은 `--seed-*`뿐 (완료 기준 1, 검증 절 첫 항목) | CHECK: `grep -cE '#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(|^\s*--space-[0-9]+\s*:|^\s*--(surface|text|border|accent|status|focus)-' src/styles.css || true` | EXPECT: `0`
G4: 간격·타이포·반경·모션이 SEED 토큰에서 오고(계약 테스트 ②③④) `html [hidden]`·`.workspace` 4행·reduced-motion·`keep-all` 계약이 유지됨 (완료 기준 1·4) | CHECK: `bun test src/styles.test.ts` | EXPECT: 모든 테스트 통과, 실패 0
G5: DOM 구조·selector 계약이 유지되고 `src/ui` 기존 테스트가 한 줄도 삭제·수정되지 않음 (완료 기준 3) | CHECK: `bun -e 'const {execSync}=require("node:child_process");const {readFileSync}=require("node:fs");const f=(s)=>(s.match(/(id|name|role|data-[a-z-]+|aria-[a-z]+|type|for)="[^"]*"/g)??[]).sort().join("\n");const a=execSync("git show 456b500cd843577e381d56842465656e3b596e4a:src/ui/app-template.ts").toString();console.log(f(a)===f(readFileSync("src/ui/app-template.ts","utf8"))?"template-attrs-same":"template-attrs-DIFF")' && git diff --numstat 456b500cd843577e381d56842465656e3b596e4a -- 'src/ui/*.test.ts'` | EXPECT: 첫 줄 `template-attrs-same`, 이어지는 `numstat` 각 행의 둘째 열(삭제 줄 수)이 전부 `0`(새 테스트 추가만 허용)
G6: 버튼이 recipe 클래스로 렌더되고 `setPrimary` 상태 전환·삭제 버튼 빌더가 recipe를 따름 (완료 기준 2 버튼 부분, 상태 라벨 불변) | CHECK: `bun test src/ui/seed.test.ts src/ui/app-view.dom.test.ts src/ui/controller.test.ts` | EXPECT: 모두 통과(기존 + 신규)
G7: 전체 TS 테스트가 통과 (완료 기준 3) | CHECK: `bun test` | EXPECT: 실패 0
G8: 루트 `DESIGN.md`의 `--seed-*` 이름 집합이 `src/styles.css`가 쓰는 집합과 같고 `.issueops/DESIGN.md`가 SEED를 단일 소스로 가리킴 (완료 기준 5) | CHECK: `bun test src/styles.test.ts -t "DESIGN.md" && grep -c '@seed-design/css' .issueops/DESIGN.md DESIGN.md` | EXPECT: 테스트 통과, `.issueops/DESIGN.md:` 1 이상, `DESIGN.md:` 1 이상
G9: 정적 검사와 번들 크기 (완료 기준 6, 성능) | CHECK: `bun run check && bun run vite:build` | EXPECT: `check` 통과, `vite build` 출력의 `dist/assets/index-*.css` 크기가 `140 kB` 이하
G10: 프로덕션 빌드 (완료 기준 6) | CHECK: `bun run build` | EXPECT: 종료 코드 0, `.app`·DMG 산출
G11: 레이아웃 불변 문자열이 모두 남아 있음(완료 기준 4 정적 부분) | CHECK: `grep -qF 'grid-template-columns: 248px minmax(0, 1fr)' src/styles.css && grep -qF 'grid-template-rows: auto auto minmax(0, 1fr) auto' src/styles.css && grep -qF 'word-break: keep-all' src/styles.css && grep -qF 'clip-path: inset(0 calc(100% - var(--progress)) 0 0)' src/styles.css && echo ok` | EXPECT: `ok` 출력, 종료 코드 0
G12 (수동 MG1, 완료 기준 2·4): 설정 시트 상태 | CHECK: manual — `bun run dev`로 창을 띄워 (1) 톱바 톱니 버튼으로 설정 시트를 열고 (2) `Hugging Face 토큰` 입력에 포커스해 포커스 링이 보이는지, 눈 버튼으로 값 표시가 토글되는지, (3) `추론 강도` 셀렉트를 `낮음`으로 바꾸어 `자동 저장됨` 상태 줄이 보이는지, (4) 참석자 추가 후 이름 입력을 비운 채 `×` 버튼과 `참석자 추가` 버튼이 보이는지, (5) 개발자 도구(우클릭 → 검사) 계산값으로 `.settings-button`·`.text-button`·`.secret-visibility-button`·`.glossary-remove`의 `height`/`min-height`가 40px 이상인지 확인 | EXPECT: 모두 육안·계산값으로 일치, 시트 본문만 스크롤되고 헤더는 고정
G13 (수동 MG2, 완료 기준 2·4): 상태 라벨과 파형 | CHECK: manual — 오디오 파일을 고르고 `출력 폴더`가 없을 때 `전사 시작`이 비활성 회색(SEED `bg-disabled`)인지, 시작 후 `회의를 전사하고 있습니다` 카드의 파형 진행 룰이 채워지며 `작업 취소` 버튼이 룰 아래·상세 로그 위 같은 위치에 있는지, 취소 후 `전사를 취소했습니다` 제목과 상태 색+텍스트 쌍(`✓`/`●`), 오류 시 `#app-error` 배너가 텍스트가 가려지지 않고 보이는지(`elementsFromPoint`로 배너 중심점 요소가 배너인지 확인) | EXPECT: idle·loading(`전사 중`)·success(`전사 완료`)·error·disabled 라벨이 변경 전과 같고 배치가 같음
G14 (수동 MG3, 완료 기준 4): reduced-motion | CHECK: manual — macOS 시스템 설정 → 손쉬운 사용 → 디스플레이 → "동작 줄이기"를 켜고 (1) 전사 중 파형 룰이 즉시 갱신되고 (2) 버튼 눌림이 축소되지 않으며 (3) 호버 색 전환이 즉시이고 (4) 마이크 녹음을 시작해 `.recording-dot`(빨간 점)이 깜빡이거나 커졌다 작아지지 않고 정지 상태인지 확인(개발자 도구 계산값 `animation-iteration-count`가 `1`) | EXPECT: 룰 애니메이션·눌림 scale·색 전환 지연·녹음 점 반복 애니메이션이 모두 없다(레일 현재 단계 마커 이동도 없음)
G15 (수동 MG4, 완료 기준 4): 다섯 구역 레이아웃 비교 | CHECK: manual — 변경 전(`git stash`가 아닌 별도 워크트리의 `main@456b500`)과 변경 후를 같은 창 크기(1240×820, 920×640, 375px 폭 시뮬레이션)에서 레일·메인·보조 패널·시트 스크린샷을 나란히 비교 | EXPECT: 영역 배치·행 수·읽는 순서가 같고 색·반경·글자 단계 차이만 존재(DESIGN.md §7 변경 범위와 일치)
G16: AA 재지정 블록이 `html[…]`이 아니라 `:root[…]` 선택자로 정확히 한 번 존재 | CHECK: `grep -c '^:root\[data-seed-color-mode="light-only"\] {' src/styles.css` | EXPECT: `1`(`html[data-seed-color-mode` 사용은 `grep -c 'html\[data-seed-color-mode' src/styles.css`가 `0`)
G17 (수동, 완료 기준 1·6): AA 재지정이 실제 계산값으로 이김 | CHECK: manual — `bun run vite:dev` 후 `http://localhost:1420`(또는 `bun run dev` 창)에서 개발자 도구 콘솔에 `getComputedStyle(document.documentElement).getPropertyValue("--seed-color-bg-brand-solid").trim()`, 같은 방식으로 `--seed-color-bg-brand-solid-pressed`, `--seed-color-fg-brand`, `--seed-color-stroke-focus-ring`을 입력하고, 대기 상태에서 활성인 `#prepare-button`에 마우스를 올리지 않은 채 `getComputedStyle(document.querySelector("#prepare-button")).backgroundColor`를 확인(`#start-button`은 엔진 준비 전 비활성이라 측정 대상이 아님) | EXPECT: 계산값은 `var()`가 치환된 hex로 나온다. bg-brand-solid와 fg-brand는 `#b93901`(기본값 `#f60`이 아님), 눌림 변수는 `#862b00`, 포커스 링은 `#217cf9`, `#prepare-button` 배경은 `rgb(185, 57, 1)`
G18: `.text-button`의 위 간격 유지 | CHECK: `grep -cE '^\.text-button \{' src/styles.css && grep -cE 'margin-top: var\(--seed-dimension-x2_5\)' src/styles.css` | EXPECT: 각각 `1` 이상(`.text-button` 규칙이 남고 `margin-top`이 토큰으로 유지됨); 수동으로 `토큰 설정 열기` 버튼이 위 문단과 붙지 않는지 육안 확인
G19: 루트 `DESIGN.md`에서 SEED 그림자와 모순되는 'warm'/'pure black' 서술 제거 | CHECK: `grep -ciE 'warm|pure black' DESIGN.md || true` | EXPECT: `0`

## 검증 명령

게이트에 이미 들어 있는 명령(`bun run check`, `bun test`, `bun run vite:build`, `bun run build`)을 제외한 보조 명령:

- `bun test src/ui/seed.test.ts` — 헬퍼가 만든 클래스가 설치된 SEED CSS와 일치하는지(단계 4).
- `grep -o -- '--seed-[a-z0-9_-]*' src/styles.css | sort -u` — DESIGN.md 표 작성용 토큰 목록(단계 6).
- 정적 시각 항목만 확인할 때: `bun run vite:dev` 후 `http://localhost:1420`. 순수 브라우저에서는 IPC가 죽어 있으므로 IPC 의존 동작은 Not Run으로 분류한다(`cautions/2026-08-23-tauri-frontend-renders-in-a-plain-browser…`).
- `bun run check:rust`, `bun run check:worker`는 Rust·Python을 건드리지 않으므로 실행하지 않는다.

## 위험과 열린 질문

### 위험

- **R1 (접근성)**: SEED 기본 시맨틱 색은 AA 4.5:1 / 포커스 3:1 제약을 만족하지 못하는 쌍이 있다(이슈 본문과 다른 사실 6). 결정: SEED 팔레트 안에서 시맨틱 변수 4개만 재지정하고(설계 §1) 글자색 쪽은 `-contrast` 단계를 직접 쓴다. 재지정은 `:root[data-seed-color-mode="light-only"]` 한 블록(임포트 뒤)에만 있고 DESIGN.md §2 표에 기록되어 `styles.test.ts`가 표 일치를 검사한다. 승리 여부는 브라우저 계산값으로 확인한다(G17). SEED가 이후 기본값을 바꾸면 이 블록을 다시 평가한다.
- **R2 (구조 vs recipe)**: 완료 기준 2와 3이 충돌한다(본문과 다른 사실 4). 결정: 구조 불변을 우선하고 recipe는 버튼에만 적용한다. 입력·셀렉트·세그먼트·칩·시트는 recipe의 시각값을 같은 SEED 토큰으로 갈피 클래스에서 참조한다. 근거: wrapper/indicator/슬롯 추가는 `app-template.ts`·동적 빌더·테스트 selector를 건드리고 `dialog`의 `break-all`은 한글 `keep-all` 계약(`.issueops/DESIGN.md`)을 깬다.
- **R3 (cascade 충돌)**: 비레이어 recipe 클래스(0,1,0)와 같은 특이도의 갈피 규칙은 소스 순서(갈피가 뒤)로, recipe의 `:is(:disabled)`·`:hover` 규칙(0,2,0)은 갈피 단일 클래스 규칙이 못 이긴다. 결정: 갈피가 재정의하는 속성은 글자색(`danger`)과 `--seed-box-color`뿐이며 `danger`는 `:not(:disabled)`로 한정하고 ghost는 recipe 공개 변수만 쓴다. `button:disabled` 불투명도는 `:not(.seed-action-button)`로 좁힌다. 수동 게이트 MG1·MG2에서 비활성/호버를 확인한다.
- **R4 (happy-dom 테스트)**: DOM 테스트는 `styles.css` 원문을 주입하고 `@import`는 무시되므로 SEED 값은 테스트에 반영되지 않는다. 가시성 계약은 `html [hidden]`, `.workspace` 행 수, 선택자 이름뿐이며 이들은 유지된다. 결정: 픽셀 가시성은 수동 게이트(`elementsFromPoint`)로 확인한다(cautions 2026-08-23 DOM visibility).
- **R5 (시각 변화 인지)**: 필→8px 반경, 폰트 단계 이동, 한글 줄간격 축소는 "리디자인이 아니다"라는 사용자 의도와 경계에 있다. 결정: 값은 SEED 소유로 받아들이고(이슈 "사용자 확인에 따라 값 변화 수용"), 레이아웃·정보 구조는 게이트 G11·G15로 고정한다.
- **R6 (라이선스 고지)**: `@seed-design/css`는 Apache-2.0이며 `NOTICE`가 재배포 시 귀속 고지 전달을 요구한다. 저장소 루트에는 `LICENSE`/`NOTICE`/서드파티 고지 파일이 없고(루트 목록 확인), 기존 서드파티 의존성(Phosphor Icons 등)도 같은 상태다. 결정: 이 이슈 범위(DESIGN.md 2개)를 넘으므로 이번 사이클에서 파일을 만들지 않고, PR 본문과 최종 보고에 "서드파티 고지 체계 신설은 별도 이슈 후보"로 명시한다.
- **R7 (SEED 업그레이드)**: 클래스 이름·토큰 이름이 바뀌면 조용히 무스타일이 된다. 결정: `seed.test.ts`(클래스 ↔ 설치된 CSS 대조)와 `styles.test.ts`(토큰 집합 일치)가 업그레이드 때 실패하도록 두고, `package.json`은 정확 핀(`3.0.2`)이다.

### 열린 질문 — 모두 결정으로 종결 (0건)

1. 갈피 시맨틱 변수를 별칭으로 남길지 → **`--seed-*` 직접 사용, 별칭 0개**. 근거: 별칭을 두면 `styles.css`의 변수 선언이 다시 갈피 고유 토큰이 되어 완료 기준 1("고유 토큰 정의 0개")과 DESIGN.md 표↔코드 일치 검증이 약해진다.
2. 레이어드 CSS 사용 여부 → **비레이어**(위 §1, 대안 A1).
3. 단계 사이 값의 반올림 → **가장 가까운 SEED 단계, 동률은 큰 쪽**(표에 명시).
4. 본문 `"기존 bun test 변경 없이"`와 `styles.test.ts` 21px의 충돌 → **21px 단언 한 줄만 `t8` 토큰 단언으로 갱신**, `src/ui/**` 테스트는 변경 금지(G5).
5. 눌림 피드백을 JS(`.seed-scale-feedback`)로 구현할지 → **하지 않는다**. `--seed-feedback-scale`을 `--seed-scale-s98` 토큰으로 지정해 CSS만으로 처리(JS 측정 변수 `--seed-element-height/width` 불필요, reduced-motion은 SEED가 1로 낮춤).
6. `all.css` 대신 개별 임포트 → **개별**(466 kB vs 약 78 kB).

### 기각한 대안

- **A1 레이어드 CSS(`*.layered.css`) 사용**: 갈피의 비레이어 리셋(`button { color: inherit }` 등)이 항상 이겨 recipe 색이 깨진다. 리셋을 별도 `@layer`로 내리는 방법도 있으나 전역 리셋 구조 변경이 늘어난다.
- **A2 모든 컨트롤을 recipe 클래스로 전면 채택**: `text-input` wrapper, `segmented-control` indicator·JS 변수, `dialog` 슬롯·`break-all`이 DOM 구조 유지(완료 기준 3)와 한글 줄바꿈 계약을 깬다.
- **A3 갈피 시맨틱 변수(`--surface-primary` 등)를 SEED 별칭으로 유지**: 선언 약 18개가 남아 "고유 토큰 정의 0개"와 표↔코드 일치 검증을 약화한다.
- **A4 SEED 기본 색을 그대로 사용(AA 재지정 없음)**: 주 버튼 대비 2.94:1, 포커스 링 2.84:1로 `DESIGN.md` §8 제약을 어긴다.
- **A5 `@seed-design/react`·Tailwind 플러그인**: 이슈 비목표.
