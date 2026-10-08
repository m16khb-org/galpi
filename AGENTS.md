# PROJECT KNOWLEDGE BASE

**Generated:** 2026-08-18T07:03:44Z
**Commit:** 0d831b7
**Branch:** main

## OVERVIEW

Galpi is an Apple Silicon macOS desktop app for local Korean meeting recording and
transcription. It spans a Bun/Vite TypeScript frontend, a Rust/Tauri host, and a
bundled Python/WhisperX worker connected by a versioned JSONL protocol.

## STRUCTURE

```text
galpi/
├── docs/ARCHITECTURE.md          # Normative DDD/hexagonal/clean/OOP/SOLID mapping
├── src/                         # Framework-light DOM frontend and Tauri IPC adapter
│   ├── application/             # Pure job and recording state machines
│   ├── domain/                  # Frontend contracts (job, speaker, backend port) and validation
│   └── ui/                      # Controllers, view, template, and interaction helpers
├── src-tauri/                   # Rust crate, Tauri configuration, and bundle metadata
│   └── src/
│       ├── domain/              # Framework-free requests, roster value objects, artifacts, worker protocol
│       ├── application/         # Ports, use cases, job/recording lifecycle
│       └── adapters/            # Tauri ingress and OS/process/audio egress
├── worker/                      # Python transcription sidecar (Qwen3/WhisperX), strict stubs
├── scripts/                     # Architecture check, sidecar staging, DMG packaging
├── DESIGN.md                    # UI system and accessibility contract
└── package.json                 # Canonical Bun workflows
```

Generated trees (`node_modules`, `dist`, `src-tauri/target`,
`src-tauri/resources/worker`, `src-tauri/binaries`) are not source.

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Frontend startup | `src/main.ts` | Composes backend, view, controller |
| Frontend orchestration | `src/ui/controller.ts` | Setup, transcription, artifacts, cancellation |
| Recording UI lifecycle | `src/ui/recording-controller.ts` | Buffers early native failures |
| DOM rendering | `src/ui/app-view.ts` | Required selectors fail fast |
| UI markup/style contract | `src/ui/app-template.ts`, `src/styles.css`, `DESIGN.md` | Keep all three aligned |
| Pure frontend transitions | `src/application/*-machine.ts` | Immutable reducers; colocated Bun tests |
| Frontend backend port | `src/domain/backend.ts` | Port contract owned by the inner layer; `TauriBackend` implements it |
| Tauri IPC client | `src/adapters/tauri-backend.ts` | `invoke`/`listen` plus Zod boundary parsing |
| Native composition | `src-tauri/src/composition.rs` | Only concrete port wiring and Tauri registration |
| IPC command surface | `src-tauri/src/adapters/inbound/tauri.rs` | 23 frontend commands and event bridges (`job-event`, `recording-event`, `chatgpt-event`) |
| ChatGPT sign-in (use cases) | `src-tauri/src/application/chatgpt.rs` | Session, token refresh, model list, sign-out behind `ChatGptAuthPort`/`ChatGptStore` |
| ChatGPT sign-in (adapters) | `src-tauri/src/adapters/outbound/chatgpt/` | OAuth/PKCE, loopback callback, ID-token, models, system browser |
| ChatGPT sign-in (UI) | `src/ui/chatgpt-controller.ts` | Sign-in/out, model choice, `chatgpt-event` handling |
| Responses API stream | `worker/galpi_worker/responses_stream.py` | Worker-side refinement transport for ChatGPT sign-in |
| Backend use cases | `src-tauri/src/application/use_cases.rs` | Central `Application` facade |
| Port contracts | `src-tauri/src/application/ports.rs` | Add platform behavior behind a port |
| Roster value objects | `src-tauri/src/domain/roster.rs` | `AssistantSettings`, `Participant`, `GlossaryEntry`, trimming rules |
| Worker supervision | `src-tauri/src/adapters/outbound/process.rs` | Bounded JSONL, cancellation, process groups |
| Native recording | `src-tauri/src/adapters/outbound/recording/` | CPAL callback, bounded queue, WAV writer |
| Worker pipeline | `worker/galpi_worker/engine.py` | ASR, alignment, diarization, output publication |
| Worker protocol | `worker/galpi_worker/protocol.py`, `src-tauri/src/domain/worker.rs` | Coupled cross-language contract |
| Build staging | `scripts/stage-sidecars.ts` | Verified ARM64 `uv`; copies worker resources |
| Architecture fences | `scripts/check-architecture.ts` | Authoritative dependency/locality check |

## CODE MAP

LSP was unavailable during generation; reference counts below are observed ast-grep
call sites, not semantic workspace references.

| Symbol | Type | Location | Refs | Role |
|--------|------|----------|------|------|
| `AppController` | class | `src/ui/controller.ts` | 1 construction | Frontend workflow coordinator |
| `TauriBackend` | class | `src/adapters/tauri-backend.ts` | 1 construction | IPC and runtime-validation boundary |
| `Application` | struct | `src-tauri/src/application/use_cases.rs` | 23 command paths | Backend use-case facade |
| `run` | function | `src-tauri/src/composition.rs` | 1 entry call | Native composition root |
| `run_process` | async function | `src-tauri/src/adapters/outbound/process.rs` | 3 production calls | Worker/process supervisor |
| `NativeRecorder` | struct | `src-tauri/src/adapters/outbound/recording/mod.rs` | 1 production wiring | Recording port implementation |
| `EventWriter.emit` | method | `worker/galpi_worker/protocol.py` | 17 call sites | Versioned JSONL event boundary |
| `transcribe` | function | `worker/galpi_worker/engine.py` | CLI entry path | ML pipeline and artifact publication |

## CONVENTIONS

- `docs/ARCHITECTURE.md` is the normative architecture document: DDD tactical
  mapping, hexagonal port ownership, SOLID enforcement, change-set rules, and the
  deliberate non-refactors. Boundary questions resolve there first.
- One dependency rule across all three runtimes: dependencies point inward
  (domain). Ports are owned by the consuming inner layer; adapters implement
  them. Framework code (Tauri, Zod, CPAL, tokio::process) lives only in
  adapters and composition.
- Keep runtime boundaries explicit: TypeScript -> Tauri commands -> `Application` ports
  -> outbound adapters -> Python worker.
- Frontend `ui/` and `application/` import contracts from `src/domain/backend.ts`;
  they never import the `TauriBackend` implementation or `@tauri-apps/*` symbols.
- TypeScript uses strict mode, exact optional properties, unchecked index checks,
  type-only imports, named exports, double quotes, 2 spaces, and no semicolons.
- Parse native responses/events with Zod before exposing them to frontend state.
- Frontend state machines return immutable states; tests live beside implementations.
- Rust domain and application layers stay framework-free; concrete adapters are wired only
  in `composition.rs`.
- Rust failures use `AppError` with stable machine-readable codes.
- Python is strict-Pyright code with local WhisperX stubs and lazy heavy imports.
- Behavioral tests use Given/When/Then comments where setup is nontrivial.
- Subscribe to Tauri events before invoking the operation that emits them.

## ANTI-PATTERNS (THIS PROJECT)

- Do not add `application`, `adapters`, `composition`, or Tauri dependencies to Rust `domain`.
- Do not add adapter, composition, or Tauri dependencies to Rust `application`.
- Do not define frontend port/contract types in `src/adapters/`; the adapter implements
  contracts declared in `src/domain/`.
- Value objects with business rules (validation, trimming) belong in Rust `domain/`,
  not `application/model.rs`.
- Keep Tauri commands in `src-tauri/src/adapters/inbound/tauri.rs`.
- Keep `.manage`, `.plugin`, and handler registration in `composition.rs`.
- Keep `tokio::process` and `nix` process primitives inside the process adapter.
- Never emit dependency diagnostics to worker stdout; stdout is machine-readable JSONL only.
- Never block the CPAL callback; it feeds a bounded queue with nonblocking sends.
- Preserve process-group cancellation, bounded output lines, and `.wav.part` cleanup.
- Do not edit staged worker copies or generated build output.
- Do not invent elapsed-time estimates for model downloads or long-running phases.

## UNIQUE STYLES

- User-facing copy is Korean; protocol/error identifiers remain stable ASCII.
- `DESIGN.md` is normative for palette, layout, motion, accessibility, and component states.
- `docs/ARCHITECTURE.md` is normative for layering and port ownership.
- Status always pairs color with text; labels are never placeholders.
- Running setup/transcription exposes cancellation; progress reports phase completion, not ETA.
- Output names (`.aligned.v2.json`, `.srt`, `_화자별.txt`) are external behavior.

## COMMANDS

```bash
bun install
bun run dev
bun run check
bun test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
uvx ruff check worker
uvx ruff format --check worker
uvx basedpyright --pythonpath <WhisperX-Python>
PYTHONPATH=. python3 -m unittest discover -s worker/tests -t . -v
bun run build
```

## NOTES

- Supported distribution target: macOS 14+ on Apple Silicon; Rust 1.88+, Bun 1.3+.
- `bun run check` covers architecture, Biome, and TypeScript only; `bun run check:all` adds the
  Rust and Python gates.
- Dev/build staging may download the pinned ARM64 `uv` archive before compiling Tauri.
- `bun run build` expects macOS `hdiutil`; signing and notarization remain separate.
- Build scripts hardcode ARM64; the DMG name follows `tauri.conf.json`'s version.
- Worker protocol, Rust parser, frontend event schema, and job reducer form one change set.

<!-- AGENT_HARNESS:START -->
## issueops project docs

This repository uses issueops project docs. Read existing AGENTS.md rules first, then read only the additional documents relevant to the task.

- Architecture or large design changes: .issueops/ARCHITECTURE.md, .issueops/CONSTITUTION.md
- Testing or verification changes: .issueops/TESTING.md
- Endpoint/DTO/OpenAPI changes: .issueops/OPEN_API_SPEC.md
- Commit or PR work: .issueops/COMMIT_POLICY.md
- Code style or structure changes: .issueops/CONVENTIONS.md
- Dependency or tech-stack changes: .issueops/TECH_STACK.md
- Run, deploy, environment, or local development: .issueops/OPERATIONS.md
- Agent start, verification, and completion workflow: .issueops/AGENT_WORKFLOW.md
- Risky or recurring-failure work: .issueops/CAUTIONS.md
- Structural rationale, alternatives, and decisions: .issueops/ADR.md
- Session start, instruction conflicts, and principle decisions: .issueops/CONSTITUTION.md
- UI, styling, or design-system changes: .issueops/DESIGN.md (client repositories only)
<!-- AGENT_HARNESS:END -->

<!-- OPENWIKI:START -->

## OpenWiki

This repository has a generated `openwiki/` evidence index. It is optional just-in-time context, not required startup reading.

- Treat source code and tests as authoritative. A brief's unknowns and review items are verification gaps, not automatic requirements.
- Prefer the narrowest quiet validation that proves the changed behavior. Preserve complete failure output.

The scheduled OpenWiki GitHub Actions workflow refreshes the repository wiki. Do not hand-edit generated OpenWiki pages unless explicitly asked; prefer updating source code/docs and letting OpenWiki regenerate.

<!-- OPENWIKI:END -->
