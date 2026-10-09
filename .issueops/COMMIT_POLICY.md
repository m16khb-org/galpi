---
name: COMMIT_POLICY.md
description: Commit message format, scope, and decision-record rules.
---

# Commit Policy

## Repo-observed format (git log)

Conventional Commits, lowercase `type(scope): imperative summary` — observed
types: `fix`, `docs`, `refactor`, `test`; observed scopes: `ui`, `readme`,
`design`, `architecture`, `frontend`, `rust` (e.g. `fix(ui): preserve Korean
word boundaries`, `docs(architecture): document DDD/hexagonal/clean/OOP/SOLID
mapping`).

~~~text
<type>(<scope>): <summary>

Why: <why this change exists>
Tested: <commands run>
Not-tested: <known verification gaps>
~~~

## Verification before committing

- Match scope to the gates in TESTING.md: TS-only changes need at least
  `bun run check` + `bun test`; Rust changes add fmt/clippy/test; worker
  changes add ruff + unittest + basedpyright.
- The worker protocol, Rust parser, frontend event schema, and job reducer
  form one change set — keep them in one commit when the protocol changes.

## Safety

- Do not stage unrelated changes; never stage generated trees
  (`node_modules`, `dist`, `src-tauri/target`, `src-tauri/resources/worker`,
  `src-tauri/binaries`).
- Manually inspect secret-like paths (API keys, credential files) before
  committing.
