---
name: overview
description: Family module overview: dependency direction and runtime topology.
---

# Architecture — Overview

Canonical index: [ARCHITECTURE.md](../ARCHITECTURE.md)

Normative document: `docs/ARCHITECTURE.md` (Korean, commit 8eca865). This
overview defers to it; resolve boundary questions there first.

## Style actually observed

Hexagonal (ports and adapters) over a clean-architecture inward dependency
rule, with DDD tactical patterns — applied consistently across three runtimes:

```text
TypeScript (WebView)   Rust (Tauri host)              Python (WhisperX sidecar)
ui/          outer     adapters/inbound/tauri.rs      __main__.py  outer
application/           application/ (13 ports)        engine/refine use cases
domain/      inner     domain/ (inner)                protocol.py (port: stdout)
adapters/tauri-backend composition.rs (root)          domain pure modules
```

- One dependency rule everywhere: dependencies point inward (domain). `domain`
  imports nothing framework-ish (serde allowed as data library); frameworks
  (Tauri, Zod, CPAL, tokio::process) live only in adapters and composition
  roots (`composition.rs`, `src/main.ts`).
- Ports are owned by the consuming inner layer (`application/ports.rs` traits;
  `src/domain/backend.ts` `BackendPort`) and implemented by adapters.
- DDD tactical: value objects (`SpeakerHint`, `Participant`, `GlossaryEntry`,
  `AssistantSettings`), aggregate root `Artifacts::path_for(kind)`, domain
  services as pure functions, conceptual in-memory `JobRegistry`.
- Inbound adapters: `tauri.rs` (14 `#[tauri::command]`s + event bridge),
  `tauri-backend.ts` (Zod boundary), worker `__main__.py`.
- Outbound adapters: `process.rs` (worker supervision), `NativeRecorder`
  (CPAL), settings/paths adapters, worker `EventWriter` (stdout JSONL v1).

## Enforcement

- `scripts/check-architecture.ts` is the authoritative fence (TS layer purity,
  `ui/` must not import `adapters/` implementations or `@tauri-apps/*`).
- Runs inside `bun run check` with Biome and `tsc --noEmit`.

## Mandatory change sets (before changing)

1. Worker protocol: `worker/galpi_worker/protocol.py` ↔
   `src-tauri/src/domain/worker.rs` ↔ `src/domain/job.ts` ↔ Zod schemas ↔
   `application/job-machine.ts` reducer — one commit set.
2. New IPC command: `adapters/inbound/tauri.rs` + `composition.rs` registration
   + `BackendPort`/Zod schema (+ `docs/ARCHITECTURE.md` §2 table).
3. New platform capability: trait in `application/ports.rs` → outbound
   implementation → wiring in `composition.rs` → `FakePort` in
   `application/tests.rs`.
4. New UI state: reducer in `application/*-machine.ts` + colocated test; the
   view only renders.

## Guidance

- Before large design changes, read `docs/ARCHITECTURE.md` §6 (found
  violations and deliberate non-refactors) before proposing refactors.
- Do not split `DesktopAdapter`'s 4 ports or hide `JobRegistry` behind a port;
  those simplifications are deliberate (§6 의도적으로 남겨둔 것).
