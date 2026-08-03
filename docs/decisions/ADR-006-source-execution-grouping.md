# ADR-006: Preserve source execution identity for Conversation grouping

## Status

Accepted

## Date

2026-08-02

## Context

Conversation adapters currently normalize commands and their results into independent Parts and Cards. Their array order is not a reliable pairing mechanism because multiple tool calls can be interleaved, while reconstructing relationships from command text, timestamps or adjacency would add heuristic behavior and false matches.

Codex, Claude Code and OpenCode already provide a stable call identifier in their source records. Antigravity Updater does not yet expose an equally reliable identifier. The UI should display a command and its results as one parent-child Execution unit without rebuilding an in-memory lookup table or owning source-specific matching rules.

## Decision

1. Add nullable `source_execution_id` fields to normalized and persisted Conversation Parts.
2. Codex, Claude Code and OpenCode adapters copy the source call identifier without rewriting or semantically validating it. Antigravity leaves the field null.
3. Do not add an Execution entity or relationship table. Execution is a read projection over Parts, not independently mutable domain data.
4. Rust Core groups only Cards whose semantic role is `command` or `result` and whose Parts share the exact `(turn_id, source_execution_id)` pair.
5. Question detail responses retain the flat `cards` array and add ordered `content_nodes`. Execution nodes reference command/result Cards by array index, so Card bodies are not duplicated.
6. The frontend follows `content_nodes` directly and performs no ID matching. Records without structured nodes retain the existing flat Card rendering path.
7. The first Card occurrence determines an Execution node's timeline position. Result order inside the node remains source Part order.

## Alternatives considered

### Add an Execution table or relationship table

- Provides a first-class persisted relation.
- Adds schema lifecycle, write synchronization and cleanup work for data that can be derived exactly from complete imported records.
- Rejected because the source identifier plus Part ownership is sufficient and more stable than a second mutable representation.

### Match command and result in the frontend

- Avoids a backend DTO change.
- Repeats domain logic in the presentation layer, increases per-render allocations and makes non-UI consumers inconsistent.
- Rejected because Core owns normalized Conversation projections.

### Match by order, command text or timestamps

- Could cover sources without call identifiers.
- Produces ambiguous matches for interleaved or repeated commands.
- Rejected. Sources without a reliable identifier remain flat until their adapter can supply one.

### Reconfirm source IDs in Rust

- Could reject malformed adapter output.
- Cannot improve an authoritative opaque identifier without adding source-specific heuristics.
- Rejected. Protocol shape validation remains at the adapter boundary; grouping uses exact equality only.

## Consequences

- Command/Result grouping is deterministic even when results arrive out of order.
- Persistence gains one nullable source identity field while Execution remains a replaceable read model.
- Frontend memory use stays proportional to the returned node/index arrays and does not require a pairing Map.
- Flat Cards remain available for search, export, deep links and compatibility.
- `content_nodes` indices are valid only against the `cards` array in the same Question detail response and must not be persisted by consumers.
- Antigravity conversations remain ungrouped rather than receiving speculative matches.

## References

- `docs/decisions/ADR-005-adapter-declared-conversation-card-contract.md`
- `specs/design.md`
