# ADR-007: Delete command-only Execution shells

## Status

Accepted

## Date

2026-08-11

## Context

ADR-006 introduced `source_execution_id` so Core could deterministically group interleaved Command and Result Cards without adjacency, text, or timestamp heuristics. After the Conversation payload cleanup, successful shell output and low-value read/search output are normally removed before persistence, while file changes are standalone Cards. Most remaining command-only Execution nodes therefore add a parent shell and expose an opaque source call ID without adding user-visible information.

The source identity still has value for exact Command/Result correlation, result-status propagation, and the smaller set of executions that retain failure diagnostics or other meaningful Result Cards.

## Decision

1. Keep nullable `source_execution_id` on normalized and persisted Conversation Parts.
2. Keep exact `(turn_id, source_execution_id)` grouping when an Execution has a meaningful Result Card.
3. Core projects a Command with no Result children as a normal Card node, not an Execution node.
4. The frontend also flattens legacy or status-only Execution nodes after hidden and empty Results are filtered, so a command-only shell is never rendered.
5. Raw source call IDs remain internal correlation data and are not displayed in the Execution header.
6. Drop the two Execution projection indexes. Current query paths load Parts by Question/Turn and group them in memory; no SQL lookup filters by `source_execution_id`.
7. Keep result-only Execution nodes because the missing Command may reflect an incomplete source record while the Result remains useful.

## Consequences

- Normal successful commands render as direct Command Cards with status and exit code.
- Executions with retained diagnostics still render Command and Result together.
- File changes remain standalone Cards.
- Existing `source_execution_id` values and persisted Part identity remain compatible; no Conversation reparse or ID migration is required.
- Removing the unused indexes reduces schema and write overhead without removing the correlation field.
- Older Engine responses that still contain command-only Execution nodes are flattened by the frontend.

## Supersedes

This ADR supersedes ADR-006 only where ADR-006 required every identified Command to appear inside an Execution presentation shell. ADR-006 remains authoritative for source identity preservation and exact Command/Result correlation.

## References

- `docs/decisions/ADR-006-source-execution-grouping.md`
- `specs/design.md`
