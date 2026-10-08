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

### Palette

| Role | Token | Light | Usage |
|---|---|---:|---|
| Surface/primary | `--surface-primary` | `#f7f6f2` | Window canvas |
| Surface/secondary | `--surface-secondary` | `#efede7` | Setup rail, grouped controls |
| Surface/elevated | `--surface-elevated` | `#fffefa` | Task and result surfaces |
| Surface/inverse | `--surface-inverse` | `#20201e` | Log panel |
| Surface/sunken | `--surface-sunken` | `#dedbd3` | Segmented-control track behind raised options |
| Text/primary | `--text-primary` | `#24231f` | Headings and body |
| Text/secondary | `--text-secondary` | `#666159` | Supporting copy |
| Text/inverse | `--text-inverse` | `#f7f6f2` | Log panel text |
| Border/default | `--border-default` | `#d8d4ca` | Inputs and dividers |
| Border/subtle | `--border-subtle` | `#e8e4da` | Surface separation |
| Accent/primary | `--accent-primary` | `#b75b37` | Primary action and current phase |
| Accent/hover | `--accent-hover` | `#98482c` | Primary action hover |
| Accent/on-primary | `--accent-on-primary` | `#fffefa` | Label on an accent-filled button |
| Accent/text | `--accent-text` | `#98482c` | Small accent labels and text links (eyebrow, section index, phase label, text buttons) |
| Status/success | `--status-success` | `#3f7356` | Ready and completed |
| Status/warning | `--status-warning` | `#8a5e1c` | Setup attention |
| Status/error | `--status-error` | `#a63f3f` | Failures |
| Focus | `--focus-ring` | `#246b9b` | Keyboard focus only |

### Rules

- Accent marks an action or current pipeline state; it is never decorative.
- Surfaces use warm tonal shifts. New colors must be added here first.
- Status colors always pair with text or a state label, never color alone.

## 3. Typography

### Scale

Values below are the implemented compact scale, px-locked to `src/styles.css`
(the stylesheet is authoritative; this table documents it — drift is a defect).

| Level | Size | Weight | Line Height | Tracking | Usage |
|---|---:|---:|---:|---:|---|
| Display | `clamp(24px, 3vw, 36px)` | 500 | 1.12 | `-0.03em` | Topbar task heading |
| H2 | `21px` | 700 | 1.3 | `-0.015em` | Panel heading, left-aligned under its eyebrow |
| Brand | `19px` | 700 | normal | `-0.02em` | Rail brand lockup |
| Step title | `14px` | 700 | normal | 0 | Rail step names |
| Status label | `13px` | 400 | normal | 0 | Status rows, job messages |
| Body | `12px` | 400 | 1.55 | normal | Default UI prose |
| Body/sm | `11px` | 400 | normal | 0 | Supporting copy, small labels |
| Caption/eyebrow | `10px` | 700 | normal | `0.12em` | Eyebrows, section indices, footer |
| Mono | `10–12px` | 400 | 1.5 | 0 | Logs, paths, inline code |

### Font Stack

- Primary: `"Avenir Next", "Pretendard", -apple-system, BlinkMacSystemFont, "Apple SD Gothic Neo", sans-serif`
- Mono: `"SFMono-Regular", "JetBrains Mono", Menlo, monospace`

### Korean line breaking

- Korean prose keeps words intact with `word-break: keep-all` and `line-break: strict`.
- `overflow-wrap: break-word` is the last-resort overflow safety; ordinary body copy must not split forms such as `처/리`, `전/사본`, or `있습/니다`.
- Paths, tokens, and code keep their component-specific `nowrap` or `overflow-wrap: anywhere` behavior.

## 4. Spacing & Layout

### Base Unit

All spacing derives from 4px.

| Token | Value | Usage |
|---|---:|---|
| `--space-1` | 4px | Tight icon/label spacing |
| `--space-2` | 8px | Compact inline groups |
| `--space-3` | 12px | Input interior spacing |
| `--space-4` | 16px | Standard grouping |
| `--space-5` | 20px | Panel interior spacing |
| `--space-6` | 24px | Primary panel padding |
| `--space-8` | 32px | Section separation |
| `--space-10` | 40px | Major task separation |

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

- **Structure**: one label plus optional progress/status glyph from Phosphor Icons.
- **Variants**: primary, secondary, quiet, destructive.
- **Emphasis follows the task**: one primary action per stage. After a transcription renders, `전사 시작` steps down to a secondary `새 전사 시작` until new audio arrives; after minutes render, `회의록 열기` becomes primary and the augment action steps down to a secondary `다시 증강`.
- **States**: idle, loading, success, error, disabled.

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
- **Motion**: 100ms press scale, 180ms opacity label swap; instant under reduced motion.

### Field Group

- **Structure**: visible label, control, optional helper, contextual error.
- **States**: default, hover, focus, disabled, error.
- **Accessibility**: labels are never placeholders; error and helper IDs connect with `aria-describedby`.
- **Layout**: stack primitive; related controls use a wrapping cluster.

### Engine Preset Picker

- **Structure**: the settings dialog hosts a `전사 엔진` section with a two-slot segmented picker (`Qwen3 기본`, `WhisperX 이전 엔진`); each slot carries a readiness badge (`준비됨`/`준비 필요`) and the section header shows the active preset. The picker lives in settings — never in the setup panel — because the setup panel hides itself once the selected engine is ready, which would swallow the control.
- **States**: switching saves immediately and re-diagnoses; an unready selection re-opens the setup panel path via `로컬 엔진 준비`. Badges pair text with color.
- **Behavior**: the change applies from the next transcription; a running job is unaffected.

### ChatGPT Panel

- **Structure**: the `AI 증강` settings section opens with a two-slot segmented control (`API 키`, `ChatGPT`, radio group `assistant-auth-mode`) that reuses the engine-picker styling. `API 키` shows the existing key, model, effort and base-URL fields (`#assistant-api-key-panel`); `ChatGPT` shows `#assistant-chatgpt-panel` and hides the API fields. `사전 정보` is shared by both modes. The ChatGPT panel holds the status line, the sign-in actions, a polite message line, the first-sign-in notice, the account's model `<select>` with `ChatGPT 요금제 사용 중 · 사용량 관리`, and the data notice. No OpenAI logo or other brand asset is used; the entry point is a text button.
- **States**: the section header badge and the status line state the account in words — `로그인 안 됨` / `ChatGPT에 로그인하지 않았습니다.`, `로그인됨` / `<이메일>로 로그인됨` (success color), `로그인 필요` / `<이메일> 계정의 로그인이 만료되었습니다. 다시 로그인해 주세요.` (warning color). Actions follow the state: signed out shows `ChatGPT로 계속하기`; waiting for the browser or code exchange replaces it with `취소` and the polite message (`브라우저에서 ChatGPT 로그인을 마쳐 주세요…`, `로그인을 마무리하는 중입니다.`); session expired shows `다시 로그인`; signed in or expired also shows `ChatGPT 로그아웃`. Failures use the message line's error state; a denied consent reads `ChatGPT 요금제 사용 동의가 거부되었습니다…`. A failed or cancelled sign-in never retries on its own — the user presses the button again.
- **Model list**: shown only while signed in, filled from the account in the server's order (`displayName` shown, `slug` as value); a saved choice the account still lists is kept, otherwise the first entry is selected. The list is a remote call, so it loads after the sheet's busy state ends and only in ChatGPT mode (on open, or right after switching to ChatGPT); the rest of the sheet stays editable meanwhile. A list failure appears in the message line and leaves the sign-in intact.
- **Sign-out outcome**: sign-out returns the sheet to API key mode and hides the ChatGPT panel, so its result goes to the sheet-wide status line (`#settings-message`), not the panel's message line. An unconfirmed server-side revocation uses the error state and tells the user to disconnect Galpi under ChatGPT's connected apps.
- **First sign-in notice**: an inline `augment-hint` inside the panel with a `확인` text button, never a modal stacked on the settings dialog; acknowledging it is saved as a preference.
- **Usage limit**: when a refinement fails with `CHATGPT_USAGE_LIMIT_EXCEEDED`, `#chatgpt-limit-hint` appears in the augment panel with `사용량 관리` as its primary button (opens `https://chatgpt.com/settings/usage`) and states that the app does not retry; the next run clears it.
- **Accessibility**: the message line is a `role="status"` live region (`#chatgpt-limit-hint` is `role="alert"`); every state carries text beside its color; buttons keep the 40px minimum height and Korean copy uses `word-break: keep-all`. Autosave of the mode and model follows the Settings Autosave rules; the window never holds a token, only the account state and e-mail.

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
|---|---:|---|---|
| Micro | 100ms | `ease-out` | Press feedback |
| Standard | 180ms | `cubic-bezier(0.16, 1, 0.3, 1)` | State swaps |
| Emphasis | 320ms | `cubic-bezier(0.16, 1, 0.3, 1)` | Phase transition |

- Animate only transform, opacity, and progress clip/scale.
- Subscribe to Tauri events before invoking setup or transcription.
- Running work always exposes a cancel action.
- `prefers-reduced-motion: reduce` removes transforms and keeps instant state changes.

## 7. Depth & Surface

Strategy: mixed tonal shift and whisper-level warm shadows.

| Level | Value | Usage |
|---|---|---|
| Edge | `inset 0 0 0 1px rgb(89 72 51 / 0.08)` | Inputs and compact controls |
| Rest | `0 1px 2px rgb(78 50 23 / 0.05), 0 8px 24px rgb(78 50 23 / 0.04)` | Main surfaces |
| Raised | `0 2px 6px rgb(78 50 23 / 0.07), 0 18px 42px rgb(78 50 23 / 0.07)` | Modal or active result |

- Cards use 14px radius; inputs use 10px; primary buttons are pills.
- No glass blur, outer glow, or pure black shadow.

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
| No dark theme in first release | Whole app | The desktop utility uses one controlled warm-light workspace; a second theme would double initial visual QA without changing task completion. | Add only after a user preference request. |
| macOS ARM64 packaging first | Build pipeline | Current target workstation is Apple Silicon and ML dependencies are platform-heavy. | Add signed Intel/Windows packages with platform-specific QA. |
