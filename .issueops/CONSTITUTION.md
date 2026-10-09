---
name: CONSTITUTION.md
description: Instruction priority, safety, and accuracy principles.
---

# Constitution

## SessionStart contract

This project-specific constitution should be read at session start. Follow the general LLM coding behavior guidelines at the top of AGENTS.md; this document adds harness structure, security, and verification invariants. Treat it as the baseline principle document for MCP routing.

## Source of truth

1. Latest explicit user/system instructions
2. Current repo AGENTS.md or a nearer nested AGENTS.md
3. .issueops/*.md
4. Current files and command output

## Normative project documents (rank by topic)

- `docs/ARCHITECTURE.md` — normative for layering, port ownership, SOLID
  mapping, change sets, and deliberate non-refactors.
- `DESIGN.md` — normative for palette, layout, motion, accessibility, and
  component states.
- `scripts/check-architecture.ts` — authoritative dependency fence; a passing
  fence outranks prose claims about boundaries.
- `AGENTS.md` — project knowledge base and conventions owner.

## Principles

- Host adapters must not bypass core policy.
- Never put raw secrets (API keys, tokens) in docs, logs,
  test fixtures, or MCP/CLI responses.
- Preserve explicit workspace-root and command-policy boundaries.
- Harness results observed from Codex and Claude Code should match.
- Do not invent elapsed-time estimates for model downloads or long-running
  phases; progress reports phase completion, not ETA.
