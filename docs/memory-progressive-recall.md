# Memory and progressive Recall operations

Memory is an independent local-first domain built on normalized Conversation Cards. SQLite remains the source of truth. Dream Notes, Recall extractions, answers, and formal Memory retain evidence snapshots; no source repository is modified.

## Runtime and privacy

- Auto-Dream is disabled by default. Enabling it or selecting AI synthesis may start the configured OpenCode or Gemini CLI and may send redacted text over that runtime's network path.
- Preview, Overview, Library, evidence-only Recall, and freshness verification are local and do not call AI.
- AI work runs in an application-owned empty temporary directory with timeout, bounded stdout/stderr, process-tree cancellation, deterministic secret redaction, and evidence-ID validation.
- Conversation content is untrusted data. It is never interpreted as shell instructions by the Memory pipeline.

## Limits

- Dream: at most 8 Sessions, 40 Questions, and 60,000 input characters; Notes are at most 6 KiB.
- Recall Phase 1: at most 8 Questions and 30,000 characters per batch, with concurrency 2 and one automatic per-batch retry.
- Recall preview reports retrieval backend, total/selected/skipped counts, unavailable-source inclusion, and truncation. Full organize is paged and must use an explicit scope.
- Model output creates review candidates only. A user must explicitly accept or edit a candidate before it becomes formal Memory.

## Recovery behavior

- Dream cursor advancement and Note/evidence persistence are one transaction. Failure, cancellation, invalid output, or disk error does not advance the cursor.
- Phase 1 extractions are persisted for 30 days so Phase 2 can be inspected and retried without changing formal Memory. Candidate creation, revisions, evidence links, and Recall completion finalize in one transaction.
- Startup marks abandoned queued/running Memory runs as interrupted. The desktop task registry exposes event updates plus polling fallback and participates in the application-exit warning.
- Freshness checks run only for selected items after the Conversation revision changes. They distinguish changed evidence, missing evidence, and unavailable sources while preserving the snapshot excerpt.

## Release verification

Run from the repository root:

```bash
cargo fmt --all -- --check
cargo test --workspace -- --test-threads=1
cargo test --workspace memory_recall_100k_scope_first_page_p95_is_below_350ms -- --ignored --test-threads=1
pnpm typecheck
pnpm test
pnpm build
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

Manual desktop checks:

1. Open all four Memory subpages and confirm Overview does not invoke AI.
2. Preview a Dream with Auto-Dream disabled, then run a manual Dream with a fixture or configured runtime.
3. Build exact session and web Recall bundles and open evidence for each of the six Card families.
4. Start an AI Recall, navigate away, observe global progress, cancel it, and verify the persisted run is cancelled.
5. Simulate a changed Card, a removed Card, and a missing Session; verify the three freshness labels and snapshot fallback.
6. Attempt to close the app with a Memory task running and confirm the exit warning.

The performance fixture creates 100,000 synthetic Question/Card rows in a temporary database, warms the scoped first-page query, and enforces the 350 ms p95 gate. Deep offset pages are reported separately by product telemetry and are not represented as first-hit latency.
