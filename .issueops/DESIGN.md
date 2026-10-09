---
name: DESIGN.md
description: Client design system: palette, typography, spacing, motion, accessibility, and component states.
---

# Design System

## Purpose

Client design contract for this repository. Read the root `DESIGN.md` before any UI, styling, component, or interaction work, and update it when a design decision changes. This file is the agent-facing routing record; it never duplicates the root document's tokens or tables.

## Authoritative design document

- Root [`DESIGN.md`](../DESIGN.md) is normative for palette, layout, motion, accessibility, and component states (galpi `AGENTS.md`, "Unique Styles").
- `@seed-design/css` 3.0.2 (`base.css`, `recipes/action-button.css`) is the single source of style values. `src/styles.css` only references `--seed-*` tokens; the root `DESIGN.md` tables list the tokens in use, and `src/styles.test.ts` fails when the two token sets differ.

## When to read

- Before editing `src/styles.css`, `src/ui/app-template.ts`, or any `src/ui/*` view/controller that changes layout, states, or copy presentation.
- Before adding a color, spacing, or motion value anywhere in the client: take it from a SEED token only (no new hex, `rgb()`, or px spacing/type/radius literals), and re-run the manual visual and AA checks whenever the `@seed-design/css` version changes.

## How changes are verified

- The markup/style contract triple `src/ui/app-template.ts` + `src/styles.css` + root `DESIGN.md` must stay aligned; a change touching one is reviewed against the other two (galpi `AGENTS.md`, "Where to look").
- Status presentation always pairs color with text; labels are never placeholders (galpi `AGENTS.md`, "Unique Styles").
- Interactive text buttons meet the 40px touch target (commit 6b39acf).
- Korean prose keeps `word-break: keep-all` / `line-break: strict`; ordinary body copy must not split Hangul forms (root `DESIGN.md`, section 3).

## Detected client surface

- Tauri (desktop shell: `src-tauri/tauri.conf.json`)
- Vite (`package.json:vite`)
- `@seed-design/css` 3.0.2 (`package.json` dependencies; framework-neutral CSS, no React)
