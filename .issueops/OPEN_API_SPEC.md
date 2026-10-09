---
name: OPEN_API_SPEC.md
description: Endpoint, DTO, and OpenAPI documentation gate rules.
---

# OpenAPI Spec Guidance

## Current API surfaces (confirmed)

Galpi has **no HTTP API and no OpenAPI/Swagger surface**. The contract
surfaces that do exist:

- 24 Tauri IPC commands in `src-tauri/src/adapters/inbound/tauri.rs`,
  consumed through `BackendPort` with Zod-validated responses
  (`src/adapters/tauri-backend.ts`).
- Versioned JSONL worker protocol v1 (`worker/galpi_worker/protocol.py` ↔
  `src-tauri/src/domain/worker.rs` ↔ `src/domain/job.ts` Zod schemas ↔
  `application/job-machine.ts` reducer) — these four form ONE change set.
- Error contract: Rust `AppError` with stable machine-readable ASCII codes.

Changing any of these is an IPC/protocol change, not an HTTP change: update
all copies of the change set and run the full gates in TESTING.md. The
OpenAPI gates below apply only if an HTTP API is introduced later.

## Gate order (future HTTP APIs)

1. Static gate: `issueops api-doc static-check --json`
2. Agent gate prompt/schema: `issueops api-doc review --json`
3. Agent gate evidence: `issueops api-doc review --result FILE --json`
4. Combined gate with evidence: `issueops api-doc check --result FILE --json`

Default scope is staged API candidate files. Scan all legacy debt only when
`--all` is explicitly supplied.

## Static omissions to block

- missing route operation summary/description
- description does not follow the repo's sectioned Markdown format
- missing path/query/header/body parameter documentation
- missing 400 response when validation surface exists
- missing 401 response for private/auth endpoints
- OpenAPI decorator or optional-validation mismatch on required/optional DTO fields

## Agent review prompt

Static checks catch decorator/comment-level omissions. Agent review reads
directly related business logic to detect public API contract drift.

The agent must inspect service/usecase/domain/error-mapping code called by
changed endpoints. If these errors can occur, they must appear in OpenAPI
responses.

- entity/resource not found → 404
- auth/session/token failure → 401
- permission/ownership/tier/role failure → 403
- validation/body/query/header problem → 400
- duplicate/state conflict/idempotency conflict → 409

Documentation must not contradict real behavior.

## Clean Swagger style

- Operation summary should be short and client-oriented.
- Prefer sectioned Markdown plus bullets for descriptions, such as `### Purpose`, `### Request Rules`/`### Processing`, and `### Auth/Notes`.
- Path/query/header/body parameters should include name, requiredness, format, and example.
- Responses should include client-handled failure statuses with schema/description, not success-only docs.
- Document single-object responses as top-level objects without unnecessary wrapper objects. Exceptions: pagination/list envelopes, explicit metadata contracts, backward compatibility, and standard error envelopes.
- If public/admin/internal docs are separated, filter paths/schemas for the intended audience.
