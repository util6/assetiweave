# ADR-004: Build Memory as an independent dual-layer domain

## Status

Accepted

## Date

2026-07-23

## Context

AssetIWeave already stores normalized Conversation Sessions, Questions, Turns, and Cards in SQLite and can search Cards through the shared Conversation search path. Users also need durable conclusions such as preferences, decisions, methods, context, and follow-ups, plus a way to recall prior work without loading an entire conversation archive into one prompt.

Treating those conclusions as Catalog `AssetKind::Memory` files would lose their evidence relationships, review lifecycle, freshness state, and cross-Conversation scope. A single summarization workflow would also mix two different jobs: inexpensive recent-work orientation and evidence-backed answers or long-term consolidation.

## Decision

- Add Memory as an independent top-level product domain alongside Conversations, Skills, Prompts, and Rules. It has its own HeaderTab, SQLite records, AppService workflows, Engine methods, CLI commands, and frontend services.
- Keep normalized Conversation Cards in SQLite as the factual evidence source. Dream notes, extractions, answers, and formal Memory items are derived records that retain evidence snapshot references.
- Use two complementary workflows:
  - Lightweight Dream consumes only stable Conversation deltas after a persisted cursor and produces short, auditable notes after time, session, and lock gates pass.
  - Deep Recall and Full Organize build an evidence bundle, persist bounded Phase 1 extractions, and use a scope-locked Phase 2 consolidation to produce cited answers and reviewable candidates.
- Keep SQLite as the only source of truth. Do not add a Memory Git repository, frontend-only store, CLI persistence path, or separate Card index.
- Default all external AI execution to off. Users must explicitly configure and enable a supported local CLI/runtime before automatic Dream or synthesized Recall can send redacted evidence outside the application process.
- Never promote Dream output, model output, or a consolidation candidate directly into formal Memory. Formal items are created manually or through an explicit accept/edit action that writes a revision and evidence links transactionally.
- Route desktop, Engine, Go CLI, and the bundled `assetiweave-memory` Skill through shared AppService workflows. The Skill organizes calls to the contract and does not duplicate persistence or consolidation rules.

## Alternatives Considered

### Reuse Catalog `AssetKind::Memory`

Rejected because scanned files are source-owned assets, while formal Memory is application-owned derived knowledge with evidence, revisions, freshness, conflicts, and review state.

### Make Memory a Conversations subpage

Rejected because formal Memory spans Conversation sources and projects, has its own lifecycle, and must remain usable even when users are not browsing a particular Session.

### Use one full-history summarization prompt

Rejected because it cannot honestly report coverage, is expensive to retry, weakens citation validation, and does not separate recent orientation from evidence-backed consolidation.

### Automatically accept model output as formal Memory

Rejected because model output can be incomplete, injected, stale, or contradictory. Automatic promotion would turn an untrusted derived value into product truth without user review.

## Consequences

- Memory requires dedicated migrations, repositories, DTOs, task state, settings, frontend screens, Engine/CLI contract coverage, and recovery tests.
- Conversation search and hydration remain the first evidence path; missing sources are explained through bounded snapshots rather than silently discarded.
- External AI calls require deterministic redaction, bounded input/output, structured validation, cancellation, process cleanup, and explicit disclosure of provider/network capability.
- Auto-Dream can run after sync only as a background-capable follow-up check. It must not run inside the Conversation sync transaction or hold the global application lock during model I/O.
- Future Memory export or target-App injection is a separate explicit mount/export decision and is not part of the first Memory module release.
