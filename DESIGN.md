# Galpi Design System

## 0. Research Log

- Embedded refs: shortlisted ElevenLabs, Linear, and Notion; picked operational `taste-skill` + ElevenLabs because an audio workstation benefits from quiet typography, waveform cues, and tactile but restrained controls.
- Lazyweb: searched `audio transcription desktop app` and `meeting recorder transcription speakers`; viewed Descript and VOMO screens. Kept Descript's task-first canvas and compact utility rail, while rejecting its editing-tool density for Galpi's simpler prepare-and-run workflow.
- StyleGallery: adopted `scroll-body-shell` for one bounded main scroll owner and `supporting-pane` for setup/progress context beside the primary task.
- Interaction reference: consulted beui.dev `button` source; retained explicit idle/loading/success/error labels, `aria-live`, press feedback, and reduced-motion behavior without importing its React implementation.
- Imagen drafts: skipped after two `generate_image` calls returned HTTP 404; no generated image is used as a visual contract.

## 1. Atmosphere & Identity

Galpi is a calm local audio workbench: technically precise, quiet during long work, and explicit about what is installed, downloaded, and running. Its signature is a horizontal waveform rule that fills by pipeline phase, making invisible model work legible without pretending to estimate time remaining.

## 2. Color

Values come from `@seed-design/css` 3.0.2 (`base.css`), pinned to the light palette by
`data-seed-color-mode="light-only"` on `<html>`. This table names the semantic tokens in use; SEED owns
the values, so none are copied here.

### Palette

| Role | Token | SEED step (light) | Usage |
|---|---|---|---|
| Canvas | `--seed-color-bg-neutral-muted` | gray-100 | Window canvas, status and artifact rows, file pickers, footer |
| Grouped surface | `--seed-color-bg-neutral-weak` | gray-200 | Setup rail, token, speaker, and settings sections, path display, completed job row |
| Layer | `--seed-color-bg-layer-default` | gray-00 | Panels, settings sheet, popover, inputs, selected segment, engine chip |
| Inverse | `--seed-color-bg-neutral-inverted` | gray-900 | Log panel |
| Track | `--seed-color-bg-neutral-weak-alpha` | black alpha-200 | Segmented-control track behind the selected option |
| Overlay | `--seed-color-bg-overlay` | black alpha-700 | Settings dialog backdrop |
| Text | `--seed-color-fg-neutral` | gray-1000 | Headings and body (17.1:1 on white) |
| Text/muted | `--seed-color-fg-neutral-muted` | gray-800 | Supporting copy (6.6:1 on white, 6.0:1 on gray-200) |
| Placeholder | `--seed-color-fg-neutral-subtle` | gray-700 | Input placeholders only, never copy (3.4:1) |
| Text/inverse | `--seed-color-fg-neutral-inverted` | gray-00 | Log text (13.3:1) |
| Stroke | `--seed-color-stroke-neutral-weak` | gray-400 | Inputs, dividers, sheet and popover edges, waveform track |
| Stroke/subtle | `--seed-color-stroke-neutral-muted` | black alpha-300 | Surface separation, selected-segment edge, chip edge |
| Brand fill | `--seed-color-bg-brand-solid` | carrot-800 (AA override) | Primary action, selected chip, record button, waveform fill |
| Brand pressed | `--seed-color-bg-brand-solid-pressed` | carrot-900 (AA override) | Primary action hover, read by the recipe |
| Brand text | `--seed-color-fg-brand` | carrot-800 (AA override) | Eyebrows, section indices, phase labels, ghost buttons, file and artifact icons (5.8:1 on white, 5.2:1 on gray-200) |
| Brand stroke | `--seed-color-stroke-brand-solid` | carrot-700 | Current rail step marker, file-picker hover border |
| Brand tint | `--seed-color-bg-brand-weak`, `--seed-color-stroke-brand-weak` | carrot-100, carrot-300 | Recorder and progress-card surface and edge, file icon tile |
| On brand | `--seed-color-fg-on-brand-solid` | white | Label and glyph on a brand fill (5.8:1) |
| Success | `--seed-color-fg-positive-contrast` | green-900 | Ready and completed text (8.9:1) |
| Success badge | `--seed-color-bg-positive-solid`, `--seed-color-fg-on-positive-solid` | green-700, white | Completion check glyph (non-text, 4.0:1) |
| Warning | `--seed-color-fg-warning-contrast` | yellow-900 | Setup attention, saving state (10.3:1) |
| Error | `--seed-color-fg-critical-contrast` | red-900 | Failures and destructive labels (8.9:1, 8.1:1 on its tint) |
| Error surface | `--seed-color-bg-critical-weak`, `--seed-color-stroke-critical-solid`, `--seed-color-bg-critical-solid` | red-100, red-700, red-700 | Error banners, banner edge, recording dot |
| Disabled | `--seed-color-bg-disabled`, `--seed-color-fg-disabled` | gray-200, gray-500 | Disabled inputs; recipe buttons use the same pair |
| Focus | `--seed-color-stroke-focus-ring` | blue-700 (AA override) | Keyboard focus only (3.95:1 on white, 3.6:1 on gray-200) |

### AA overrides

SEED's light defaults miss §8 for four pairs. One `:root[data-seed-color-mode="light-only"]` block placed
after the imports in `src/styles.css` points them at a darker step of the same SEED palette; it has the
same specificity as `base.css`, so source order makes it win.

| Token | SEED default | Override | Reason |
|---|---|---|---|
| `--seed-color-bg-brand-solid` | carrot-600 (white label 2.9:1) | `--seed-color-palette-carrot-800` | White label reaches 5.8:1 |
| `--seed-color-bg-brand-solid-pressed` | carrot-700 | `--seed-color-palette-carrot-900` | Hover stays darker than rest |
| `--seed-color-fg-brand` | carrot-600 (2.9:1 on white) | `--seed-color-palette-carrot-800` | Small brand text reaches 5.8:1 |
| `--seed-color-stroke-focus-ring` | blue-600 (2.8:1 on white) | `--seed-color-palette-blue-700` | Focus indicator clears 3:1 |
| `--seed-feedback-scale` | 1 | `--seed-scale-s98` | Recipe press feedback without SEED's JS hook; SEED sets the scale to 1 under reduced motion |

### Rules

- Accent marks an action or current pipeline state; it is never decorative.
- Colors come from SEED tokens: semantic tokens in rules, palette steps only inside the AA overrides.
  Hex, `rgb()`, and `hsl()` literals are not allowed; a new token joins this table in the same change,
  and `src/styles.test.ts` fails when the table and the stylesheet differ.
- The two translucent surfaces (topbar, current rail step) mix a SEED token with `transparent`.
- Status colors always pair with text or a state label, never color alone.

## 3. Typography

### Scale

Sizes are SEED type tokens (`rem`-based on the default 16px root). Off-scale sizes moved to the nearest
step: 10px captions became 11px, 17px and 19px became 18px, 21px became 22px.

| Level | Size | Weight | Line height | Tracking | Usage |
|---|---|---|---|---|---|
| Display | `clamp(var(--seed-font-size-t9), 3vw, var(--seed-font-size-t12))` | `--seed-font-weight-medium` | 1.12 | `-0.03em` | Topbar task heading |
| H2 | `--seed-font-size-t8` | `--seed-font-weight-bold` | `--seed-line-height-t8` | `-0.015em` | Panel heading, left-aligned under its eyebrow |
| Sheet title, job percent | `--seed-font-size-t9` | bold, medium | normal | `-0.025em` (sheet title) | Settings title, progress percent |
| Card title, brand | `--seed-font-size-t6` | bold | normal | `-0.02em` (brand) | Rail brand lockup, progress-card title |
| Artifact icon | `--seed-font-size-t7` | — | — | — | Artifact row glyph |
| Step title | `--seed-font-size-t4` | bold | normal | 0 | Rail step names, recording clock (mono) |
| Status label | `--seed-font-size-t3` | regular | normal; `--seed-line-height-t3` for panel descriptions | 0 | Status rows, job messages, chips, roster rows, panel descriptions |
| Body | `--seed-font-size-t2` | regular | `--seed-line-height-t2` | normal | Default UI prose, field labels, settings fields |
| Body/sm, Caption | `--seed-font-size-t1` | regular, bold | `--seed-line-height-t1` | `0.12em` eyebrows; `0.04em` rail brand caption (fits one line) | Supporting copy; eyebrows, section indices, footer |
| Mono | `--seed-font-size-t1` | regular | `--seed-line-height-t1` | 0 | Logs, paths, inline code |

Former 600 weights round up to bold; SEED has no letter-spacing tokens, so tracking stays in `em`.

### Font Stack

- Primary: `var(--seed-font-family)` (SEED system stack: `-apple-system`, Apple SD Gothic Neo, Pretendard).
- Mono: `"SFMono-Regular", Menlo, monospace`; SEED ships no mono token.

### Korean line breaking

- Korean prose keeps words intact with `word-break: keep-all` and `line-break: strict`.
- `overflow-wrap: break-word` is the last-resort overflow safety; ordinary body copy must not split forms such as `처/리`, `전/사본`, or `있습/니다`.
- Paths, tokens, and code keep their component-specific `nowrap` or `overflow-wrap: anywhere` behavior.

## 4. Spacing & Layout

### Base Unit

Padding, margin, and gap use SEED's 4px dimension scale with half steps. Off-scale values moved to the
nearest step, ties going up; structural sizes (248px rail, 560px sheet, 132px chip area, icon tiles) stay literal.

| Token | Value | Usage |
|---|---:|---|
| `--seed-dimension-x0_5` | 2px | Hairline gaps, focus-ring width and offset |
| `--seed-dimension-x1` | 4px | Tight icon and label spacing |
| `--seed-dimension-x1_5` | 6px | Chip and row gaps |
| `--seed-dimension-x2` | 8px | Compact inline groups |
| `--seed-dimension-x2_5` | 10px | Row padding, text-button top gap |
| `--seed-dimension-x3` | 12px | Input interior spacing |
| `--seed-dimension-x3_5` | 14px | Card and row interior spacing |
| `--seed-dimension-x4` | 16px | Standard grouping |
| `--seed-dimension-x4_5` | 18px | Popover padding, settings status line |
| `--seed-dimension-x5` | 20px | Panel interior spacing |
| `--seed-dimension-x6` | 24px | Section padding |
| `--seed-dimension-x7` | 28px | Panel padding |
| `--seed-dimension-x8` | 32px | Section separation |
| `--seed-dimension-x9` | 36px | Speaker-count number inputs |
| `--seed-dimension-x10` | 40px | Touch targets, control height, workspace gutter |
| `--seed-dimension-x12` | 48px | Status row height |
| `--seed-dimension-x14` | 56px | Rail step offset, workspace bottom padding |

### Grid

- Window target: 1240×820, minimum 920×640.
- Shell: fixed 248px setup rail plus fluid work area above 960px; single column below.
- Main work: intrinsic `supporting-pane`, primary task minimum 32rem where space permits.
- The work area owns vertical scrolling; rail and footer remain fixed.

### Rules

- Use `100dvb`, `minmax(0, 1fr)`, and `min-block-size: 0` for the bounded shell.
- At 375px equivalent width, controls stack into one readable column with no primary horizontal scroll.
- Paths use `overflow-wrap: anywhere`; long file names truncate only when the full value is available in a title.

## 5. Components

### Step Rail

- **Structure**: three user stages — `01 회의 전사`, `02 전사 결과`, `03 전사 결과 AI 증강` — each with a state label and short explanation. The rail lockup shows the app icon (`assets/app-icon.svg`, the same mark as the bundled DMG/app icon) in a 42px rounded tile above the `갈피` brand; it is decorative (`aria-hidden`) because the adjacent text carries the name. Engine and model preparation are a pre-gate panel (`00 / 준비`), not rail stages; they disappear once the local environment is ready.
- **Stage mapping**: `01` completes when transcription artifacts render; `02` becomes current with the results panel; `03` completes when augmented minutes render. The augment stage hint links to Settings when no assistant key is saved and otherwise waits for a transcription, or starts directly from an imported transcript file (`전사문 파일 가져오기`) which registers the transcript as the meeting result without a new recording. Augmentation streams progress: the refine phase emits `N자 작성됨` updates on the existing phase-event channel while the provider generates.
- **States**: pending, current, completed, blocked. Each item carries a text state label under its subtitle — `대기`, `● 현재 단계`, `✓ 완료` — so the state never rests on color; the glyph is decorative (CSS `content` with empty alt text).
- **Accessibility**: `aria-current="step"` on the current item; text accompanies every state color.
- **Motion**: current marker fades and translates no more than 4px; no motion under reduced motion.
- **Layout**: fixed shell rail; never owns scroll.

### Top Bar Task Heading

- **Structure**: `LOCAL AUDIO WORKSPACE` eyebrow, the Display heading, and one 13px secondary status line.
- **Content**: the heading names the task, not a slogan — `로컬 엔진을 준비해 주세요` before setup, `새 회의 전사` when idle, then the meeting name (the chosen audio or imported transcript file name without its extension). The status line follows the work: `녹음 중`, `전사 중 · 정렬 단계`, `전사 완료 · 회의록 작성 대기`, `회의록 작성 중`, `회의록 완료`.
- **Layout**: long names wrap with `overflow-wrap: anywhere`; the engine chip and settings button keep their place.

### Panel Heading

- **Structure**: eyebrow (`00 / 준비`, `01 / 녹음·파일`, `02 / 산출물`, `03 / AI 증강`) over a left-aligned H2; the description sits right-aligned beside it. The eyebrow names the panel's material and never repeats the title.

### Status Button

- **Structure**: one label plus optional progress/status glyph from Phosphor Icons, rendered with the SEED
  `action-button` recipe. `src/ui/seed.ts` composes the recipe classes; the Galpi classes
  (`primary-button`, `secondary-button`, `text-button`, …) stay alongside them as selectors.
- **Variants**: primary → `brandSolid`; secondary → `neutralOutline`; quiet → `ghost`, whose text color
  is set through the recipe hook `--seed-box-color` (`--seed-color-fg-brand` for text actions); destructive →
  `neutralOutline` with a `--seed-color-fg-critical-contrast` label, because SEED has no critical outline.
- **Sizes**: `medium` (40px) everywhere a touch target is required; `xsmall` (32px pill) only for the
  recorder stop/discard pair and the token-guide close button, which were already compact.
- **Text actions**: ghost text buttons (`text-button`, the token-guide trigger) start-align and narrow the
  recipe's inline padding through `--seed-box-padding-left` / `--seed-box-padding-right`
  (`--seed-dimension-x1_5`) so the label keeps the left edge of the copy above it.
- **Emphasis follows the task**: one primary action per stage. After a transcription renders, `전사 시작` steps down to a secondary `새 전사 시작` until new audio arrives; after minutes render, `회의록 열기` becomes primary and the augment action steps down to a secondary `다시 증강`. The step-down swaps the recipe variant together with the Galpi class.
- **States**: idle, loading, success, error, disabled. Disabled recipe buttons use SEED's disabled colors instead of opacity.

### Participant Chips

- **Structure**: the per-meeting attendee picker renders one toggle chip per roster entry (`이름 · 팀 · 역할`, omitting absent parts); a saved roster is edited in Settings under `참석자 명부` with name, optional team, optional role, optional freeform description (`담당 업무 등 설명`), and comma-separated aliases. A chip's description surfaces as its tooltip; the chip label itself stays compact. An unselected chip renders no mark slot — the check glyph appears only on selection.
- **States**: unselected, selected, disabled; selection state pairs the filled accent with a check glyph and the `N명 선택` counter, never color alone.
- **Behavior**: selecting attendees fills the speaker-count hint (`정확히 N`) and a note says the value was auto-filled; a later manual change is never overridden. An empty roster shows a hint linking to Settings instead of an empty chip group.
- **Accessibility**: each chip is a real checkbox inside `role="group"`; focus ring follows the global outline token; chips scroll independently above six entries.

### Glossary

- **Structure**: Settings hosts a `단어집` section of `용어` plus optional `뜻/설명` rows; entries persist with assistant settings and apply to every minutes refinement — there is no per-meeting toggle.
- **Behavior**: entries reach the worker as a `<단어집>` prompt block so misheard terms are corrected against the saved spelling; an empty glossary states that no terms are registered.
- **States**: the section header carries a `N개` counter (or `비어 있음`); rows are removed individually with a labeled X button.
- **Accessibility**: `aria-busy` while loading and polite live label updates.
- **Motion**: row removal buttons follow the recipe press feedback; instant under reduced motion.

### Field Group

- **Structure**: visible label, control, optional helper, contextual error.
- **States**: default, hover, focus, disabled, error.
- **Accessibility**: labels are never placeholders; error and helper IDs connect with `aria-describedby`.
- **Layout**: stack primitive; related controls use a wrapping cluster.

### Engine Preset Picker

- **Structure**: the settings dialog hosts a `전사 엔진` section with a two-slot segmented picker (`Qwen3 기본`, `WhisperX 이전 엔진`); each slot carries a readiness badge (`준비됨`/`준비 필요`) and the section header shows the active preset. The picker lives in settings — never in the setup panel — because the setup panel hides itself once the selected engine is ready, which would swallow the control.
- **States**: switching saves immediately and re-diagnoses; an unready selection re-opens the setup panel path via `로컬 엔진 준비`. Badges pair text with color.
- **Behavior**: the change applies from the next transcription; a running job is unaffected.

### Settings Autosave

- **Behavior**: text fields persist when the user commits the edit (blur or Enter); selects and row removals persist immediately. There is no global save button.
- **Concurrency**: while one local write is active, later changes coalesce into one latest-state write instead of racing or disabling the sheet.
- **Feedback**: the polite settings status line moves through `저장 중` → `자동 저장됨`, or an actionable error. Errors preserve the edited values and the next change retries.
- **Destructive actions**: clearing a stored credential remains an explicit labeled action; autosave never turns a destructive clear into an implicit side effect.
- **Accessibility**: persistence feedback uses the existing `role="status"` live region and never steals focus.

### Phase Timeline

- **Structure**: waveform progress rule plus four named phases and live phase message. Completed phases carry `✓` and the current phase `●` beside their color.
- **States**: waiting, active, completed, failed, cancelled. The card title states the outcome (`회의를 전사하고 있습니다` → `전사를 마쳤습니다` / `전사하지 못했습니다` / `전사를 취소했습니다`); setup and augmentation cards follow the same rule.
- **Completion**: a finished transcription folds the card into one summary row on the secondary surface — success check, `전사 완료 · N개 발화 보존 · N개 환각 제거`, and the log disclosure on the right — and keeps it until the next run. Failed and cancelled runs keep the full card so the error stays in view.
- **Accessibility**: `role="progressbar"` with phase-local value; no fabricated time estimate.
- **Motion**: waveform fill uses transform only and stops under reduced motion.

### Artifact Row

- **Structure**: artifact kind, canonical path, open and reveal actions. An imported transcript renders only the transcript row — subtitle and checkpoint rows stay hidden until a real transcription produces them.
- **States**: ready, opening, missing, error.
- **Accessibility**: action labels include artifact kind; paths remain selectable.
- **Layout**: cluster that wraps actions before the path overflows.

### Augment Target

- **Structure**: before any transcript exists, a waiting hint followed by the `전사문 파일 가져오기` picker (no "또는" divider — there is no option before it). Once a transcript is chosen or produced, the same control reads `대상 전사문` with its path and a `다른 파일 가져오기` text action, on a solid border.

### Log Disclosure

- **Structure**: native `details` with capped mono output, always on its own row below the job's cancel action.
- **States**: collapsed by default, expanded, error-highlighted.
- **Accessibility**: raw diagnostics remain copyable; user-facing error summary sits outside the disclosure.
- **Layout**: the log body owns its own bounded scroll only when expanded.

## 6. Motion & Interaction

| Type | Duration | Easing | Usage |
|---|---|---|---|
| Micro | `--seed-duration-d2` | `--seed-timing-function-easing` | Chip selection color |
| Press | recipe-owned | `--seed-feedback-scale` (`--seed-scale-s98`) | Action-button press |
| Emphasis | `--seed-duration-d6` | `--seed-timing-function-enter-expressive` | Waveform fill |

- Animate only transform, opacity, and progress clip/scale.
- The recording pulse keeps its 1.6s loop; SEED has no looping-duration token.
- Subscribe to Tauri events before invoking setup or transcription.
- Running work always exposes a cancel action.
- `prefers-reduced-motion: reduce` collapses transition and animation durations and caps iteration at one
  with `!important`, because recipe class transitions outrank a universal rule; SEED drops the press scale to 1.

## 7. Depth & Surface

Strategy: tonal layers plus SEED shadow tokens only.

| Level | Token | Usage |
|---|---|---|
| Edge | 1px `--seed-color-stroke-neutral-weak` border | Inputs and compact controls |
| Rest | `--seed-shadow-s1` | Panels, engine chip, brand mark |
| Raised | `--seed-shadow-s3` | Settings sheet, token-guide popover |

- Radius: `--seed-radius-r3_5` panels; `--seed-radius-r4` settings sheet; `--seed-radius-r3` sections,
  cards, and pickers; `--seed-radius-r2_5` rows, engine chip, segmented track, and log; `--seed-radius-r2`
  inputs, recipe buttons, segment options, and banners; `--seed-radius-r1` waveform track;
  `--seed-radius-full` chips and the settings close button.
- No glass blur or outer glow.

## 8. Accessibility Constraints & Accepted Debt

### Constraints

- WCAG 2.2 AA: 4.5:1 body text, 3:1 large text and controls.
- Every action is keyboard reachable with a visible focus ring.
- Native dialogs handle file and folder selection.
- Progress and errors are announced without repeatedly reading raw log lines.
- Touch targets are at least 40×40px.
- Long model downloads never rely on color, animation, or elapsed-time guesses.

### Accepted Debt

| Item | Location | Why accepted | Owner / Exit |
|---|---|---|---|
| No dark theme in first release | Whole app | The desktop utility uses one controlled light workspace (`data-seed-color-mode="light-only"`); a second theme would double initial visual QA without changing task completion. | Add only after a user preference request. |
| macOS ARM64 packaging first | Build pipeline | Current target workstation is Apple Silicon and ML dependencies are platform-heavy. | Add signed Intel/Windows packages with platform-specific QA. |
