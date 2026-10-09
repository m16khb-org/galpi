---
name: overview
description: Family module overview: implementation and interface conventions.
---

# Conventions — Overview

Canonical index: [CONVENTIONS.md](../CONVENTIONS.md)

Full list owned by `AGENTS.md` (CONVENTIONS / ANTI-PATTERNS / UNIQUE STYLES)
and `docs/ARCHITECTURE.md`; those win over this overview.

## Confirmed conventions (repo-observed)

- TypeScript: strict, `exactOptionalPropertyTypes`,
  `noUncheckedIndexedAccess`, type-only imports, named exports, double
  quotes, 2 spaces, no semicolons.
- Parse every native response/event with Zod at the Tauri boundary
  (`src/adapters/tauri-backend.ts`) before it reaches frontend state.
- State machines are immutable `(state, event) -> state` reducers with
  colocated tests; views only render.
- `ui/` and `application/` import contracts from `src/domain/backend.ts`
  only, never `TauriBackend` or `@tauri-apps/*`.
- Rust `domain`/`application` stay framework-free; wiring only in
  `composition.rs`. Failures use `AppError` with stable ASCII codes.
- Python: strict PyRight, local WhisperX stubs, lazy heavy imports; worker
  stdout is JSONL only.
- Value objects with business rules live in Rust `domain/` (e.g.
  `domain/roster.rs`), not `application/model.rs`.
- User-facing copy is Korean; protocol/error identifiers stay ASCII.
  Output names (`.aligned.v2.json`, `.srt`, `_화자별.txt`) are external
  behavior.

## Editing rules

- Follow existing style first; no repo-wide formatting unless asked.
- `scripts/check-architecture.ts` is the authoritative fence: run
  `bun run check` after structural edits.

## SOLID as practiced (docs/ARCHITECTURE.md SOLID section)

- Ports exist only at real boundaries (13 Rust traits in
  `application/ports.rs`, TS `BackendPort`), each with a production adapter
  and a test fake.
- Extend by tagged union + exhaustive `match`/`switch`; never absorb new
  cases with `default:`.
- The deliberate non-refactors (DesktopAdapter's 4 ports, un-ported
  JobRegistry, unified TS BackendPort, Python layer non-separation) are
  intentional — do not "fix" them.

## Error-handling style

- Rust: errors as values (`AppError`, stable codes).
- TypeScript: Zod at the boundary; failures flow through state machines.
- Python worker: failures are versioned JSONL error events on stdout.
- No HTTP error contract (see OPEN_API_SPEC.md).
