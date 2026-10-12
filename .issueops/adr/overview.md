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
- [마이크와 시스템 소리 트랙은 정지 뒤에 합치고, Windows 루프백은 무음 출력 스트림으로 살려 둔다](2026-10-11-system-audio-two-track-merge.md)
- [auth-gateway 의존을 없애고 Drive는 사용자가 저장한 Google 클라이언트로만 연결한다(소스에 클라이언트를 두지 않음)](2026-10-12-drop-auth-gateway-drive-client-in-source.md)
- [Google Drive 연동은 appProperties로 무상태 동기화한다(gateway 대행 교환은 위 결정이 대체)](2026-10-11-google-drive-auth-gateway-appproperties.md)
- [Windows CUDA 빌드에서 ASR도 CTranslate2 CUDA float16으로 실행](2026-10-09-windows-asr-on-cuda.md)
