---
name: 2026-10-09-adopt-seed-css-tokens-with-the-action-button-recipe-only
description: Accepted decision record with rationale, alternatives, and consequences.
---

# Adopt SEED CSS tokens with the action-button recipe only

- Date: 2026-10-09
- Kind: `adr`
- Source: issueops-docs
- Summary: Galpi styles take every value from @seed-design/css 3.0.2 (non-layered base.css plus recipes/action-button.css); only buttons use a recipe, other controls keep their markup and read SEED tokens.
- Context: Issue #5 asked for Daangn SEED tokens and component styles without a redesign. text-input needs a wrapper root with data-focus/data-invalid, segmented-control needs an indicator element and JS variables, and dialog needs extra slots plus word-break: break-all, so those recipes would change app-template.ts DOM and break the Korean keep-all contract. SEED light defaults fail WCAG AA for the brand fill (2.9:1), brand text (2.9:1), and focus ring (2.8:1).
- Decision: Import base.css and recipes/action-button.css unlayered from src/styles.css; pin data-seed-color-mode=light-only on <html>; reference --seed-* tokens directly with no Galpi alias tokens; render buttons with action-button classes composed by src/ui/seed.ts while keeping the Galpi selector classes; point four semantic tokens (bg-brand-solid, bg-brand-solid-pressed, fg-brand, stroke-focus-ring) at darker SEED palette steps in one :root[data-seed-color-mode=light-only] block after the imports. Rejected: *.layered.css (unlayered Galpi resets such as button { color: inherit } would always beat layered recipes); adopting every recipe (DOM and keep-all breakage); keeping --surface-*/--text-* aliases (re-creates app-owned tokens and weakens the DESIGN.md parity test); SEED default colors (AA failures); @seed-design/react or Tailwind (issue non-goals).
- Consequences: src/styles.test.ts fails on hex/rgb/hsl literals, non-SEED custom properties, px spacing/type/radius, and any drift between the --seed-* names in src/styles.css and root DESIGN.md. A SEED upgrade must re-check the AA overrides and src/ui/seed.test.ts compares helper classes with the installed recipe CSS. The production CSS bundle grows from 27.2 kB to about 109 kB (gzip 5.8 to 14.6 kB). Third-party NOTICE delivery for Apache-2.0 assets remains a separate follow-up.
