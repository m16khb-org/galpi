---
name: overview
description: Family module overview: accepted architecture decisions and roadmap.
---

# Architecture Decision Records — Overview

Canonical index: [ADR.md](../ADR.md)

## Purpose

Record structural choices, rejected alternatives, and decisions that affect long-term maintenance. This is not an implementation note; preserve why this structure was chosen and which alternatives should not be retried.

## When to read

- Before architecture changes, large refactors, or dependency/framework replacement
- When changing or bypassing existing structure
- When modifying code whose historical rationale is unclear

## When to append

- A new structure or boundary was chosen.
- Alternatives were considered and rejection reasons will reduce future re-analysis.
- Operations, performance, or security constraints shaped the design.

## Entry template

### YYYY-MM-DD: <decision title>

- Context: <problem and constraints>
- Decision: <chosen structure>
- Alternatives rejected:
  - <alternative>: <why rejected>
- Consequences: <tradeoffs and follow-up>
- Evidence: <files, commands, issues, docs>
- [Google Drive 연동은 auth-gateway가 토큰을 교환하고 appProperties로 무상태 동기화한다](2026-10-11-google-drive-auth-gateway-appproperties.md)
