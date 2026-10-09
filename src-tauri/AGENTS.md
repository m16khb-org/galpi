# SRC-TAURI KNOWLEDGE BASE

Rust/Tauri host for Galpi. Repo-wide rules live in `../AGENTS.md`; this guide
covers the crate only.

## OVERVIEW

Crate `galpi` (lib `galpi_lib`, edition 2024, MSRV 1.85). Entry chain is
`main.rs` -> `lib.rs::run()` -> `composition.rs`, the only file that builds
concrete adapters and registers Tauri state and handlers. `Application`
(`src/application/use_cases.rs`) is the facade behind all 28 IPC commands.
It drives ports declared in `src/application/ports.rs`: `DesktopAdapter`
implements engine, transcription, transcript import, refinement, and artifacts
ports; `NativeRecorder` covers CPAL capture. Architecture principles (DDD value
objects in `domain/`, port ownership, SOLID mapping) are normative in
`../docs/ARCHITECTURE.md`. Outbound internals are documented in
`src/adapters/outbound/AGENTS.md`.

## STRUCTURE

```text
src-tauri/
├── src/
│   ├── domain/               # Requests, roster value objects, artifacts, worker protocol
│   ├── application/          # Ports, use cases, job registry, DTOs, errors, tests
│   ├── adapters/
│   │   ├── inbound/tauri.rs  # Command surface plus TauriEvents bridge
│   │   └── outbound/         # Desktop, setup, transcription, paths, process, recording
│   └── composition.rs        # Builder, plugins, state, handlers
├── tauri.conf.json           # Window, bundle, resources
├── capabilities/default.json # Tauri permissions
└── build.rs
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add IPC command | `src/adapters/inbound/tauri.rs` | Also register it in `composition.rs` |
| Add platform behavior | `src/application/ports.rs` | Add port, outbound impl, composition wiring |
| Change roster value objects | `src/domain/roster.rs` | `AssistantSettings`, `Participant`, `GlossaryEntry` and trimming rules |
| Change job lifecycle | `src/application/jobs.rs` | Active job, cancellation, artifact registry |
| Change recording rules | `src/application/use_cases.rs` | Active UUID and ID verification |
| Change wire DTOs | `src/application/model.rs`, `src/domain/worker.rs` | camelCase IPC vs tagged worker events; worker wire formats live in `domain/worker.rs` |
| Add application test | `src/application/tests.rs` | `FakePort` implements every port |
| Grant plugin permission | `capabilities/default.json` | Initialize matching plugin in composition |

## CONVENTIONS

- IPC DTOs use Serde camelCase; worker events use snake_case tagged variants
  flattened into a versioned envelope.
- Errors are `AppError { code, message }`: stable ASCII code, Korean message.
  Wrap IO failures with `AppError::io(context, error)`.
- Cargo lint policy denies Clippy `all`, unwrap, expect, panic, TODO,
  unimplemented, unsafe-operation mistakes, and non-ASCII identifiers.
- Ports use `Arc<dyn Trait + Send + Sync>`; cancellable operations receive a
  mutable oneshot receiver.
- `JobRegistry` permits one job, checks client job-ID reuse, and owns artifacts.
  `Application` separately permits one active recording.
- Sync CPAL/filesystem work runs through `spawn_blocking`.
- Events cross only through `JobEvents` and `RecordingEvents`, implemented by
  `TauriEvents` as `job-event` and `recording-event`.
- Tests use inline modules or sibling `tests.rs`, Given/When/Then comments, and
  port fakes rather than mocked internals.

## ANTI-PATTERNS

- No Tauri, adapter, or composition imports in `domain` or `application`.
- No concrete port construction, `.manage`, plugin setup, or handler
  registration outside `composition.rs`.
- Commands stay thin: deserialize, call `Application`, return mapped errors.
- Do not bypass `JobRegistry` or the `active_recording` mutex.
- Do not leak raw IO or Serde failures across IPC.
- Do not change worker event shapes without the Python emitter and frontend
  schemas in the same change.
