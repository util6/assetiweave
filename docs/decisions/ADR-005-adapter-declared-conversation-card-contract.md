# ADR-005: Adapter-declared Conversation Card contract

## Status

Accepted

## Date

2026-07-26

## Context

Conversation cards are currently constrained to five globally fixed types. The same list is repeated in adapter scripts, Rust DTOs and repositories, the search index, frontend rendering, filters, translations, colors and export logic. Different AI Apps expose different semantic records, so extending the list requires coordinated changes across unrelated layers and unknown types can disappear silently.

The system needs App-specific semantic card types without allowing third-party adapters to inject arbitrary UI code or destabilize search and persistence.

## Decision

Adopt a versioned Conversation Card contract with these properties:

1. Card semantic `kind` is adapter-declared and extensible.
2. Card presentation uses a small Core-controlled renderer registry.
3. A Part produces zero or one Card; adapters split multiple cards into multiple Parts.
4. External adapter output is validated once at the protocol boundary.
5. Core produces one structured Card projection consumed by UI, search, export and Memory.
6. Old `metadata_json.content_card` remains supported through an additive compatibility adapter while consumers migrate.
7. Adapter content hash and Card contract version participate in rehydration state.
8. User-derived data is stored/protected independently from replaceable parser output.
9. App-specific kinds are adapter-namespaced (for example `claude-code.reasoning`); optional `semantic_role` supplies cross-App aggregation.
10. Card identity is the stable Part ID and never contains kind, renderer or legacy suffix. Old anchors remain read-compatible metadata only.
11. Manifest declarations select only Core-owned renderers and explicitly constrain renderer overrides through `allowed_renderers`.
12. Source-specific text decoding and noise cleanup belong to the Adapter package. Core and the frontend do not interpret source runner labels, escape conventions or terminal envelopes.

## Alternatives considered

### Continue expanding the global enum

- Simple for one additional type.
- Repeats the current coupling in every consumer and cannot support independent App evolution.
- Rejected because it increases coordination cost and silent mismatch risk with every type.

### Allow adapters to provide frontend components

- Maximally flexible rendering.
- Introduces arbitrary code execution, version skew, styling fragmentation and a much larger trust surface.
- Rejected. Adapters provide data and renderer hints, not UI implementations.

### Store only opaque metadata and let each consumer interpret it

- Avoids a migration initially.
- Preserves multiple parsers and inconsistent semantics.
- Rejected as the target architecture; retained only as a temporary legacy input format.

### Replace the entire Conversation subsystem at once

- Produces a clean end state quickly on paper.
- Risks breaking existing adapters, history, translations, search IDs and current Memory work.
- Rejected in favor of an additive strangler migration inside the existing module tree.

## Consequences

- New App-specific semantic kinds can use existing renderers without an AssetIWeave release.
- New renderers remain intentional Core features with security and compatibility gates.
- Command/Result text can evolve with each source format by releasing that Adapter alone; Core remains a source-agnostic protocol, persistence and projection layer.
- Core gains a card validation/projection module and becomes the owner of generic search/export semantics.
- Card Contract v1 imports emit only structured Card descriptors; Core temporarily dual-reads legacy metadata for historical databases and adapters.
- Aggregated sync/export operation logs provide the two-release-cycle, zero-use retirement evidence without logging individual Parts.
- Search and frontend type APIs must widen from fixed enums to validated dynamic identifiers.
- Deep links use stable Part/Card IDs while continuing to resolve legacy `${part_id}-${type}` anchors.
- Parser refresh logic must preserve translations and other user-derived data.
- The migration is larger than adding another enum variant, but removes the recurring cross-layer cost rather than postponing it.

## References

- `specs/feature-plans/conversation-card-contract-v1.md`
