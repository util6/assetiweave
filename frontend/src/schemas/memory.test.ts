import { describe, expect, it } from "vitest";
import { memoryItemDetailSchema, memoryItemPageSchema } from "./memory";

const item = {
  id: "memory-1",
  kind: "decision",
  status: "active",
  title: "Use AppService",
  content_markdown: "Desktop and CLI share one workflow.",
  scope: { app_id: null, source_id: null, project_path: "~/assetiweave", session_id: null },
  scope_fingerprint: "scope-hash",
  origin: "manual",
  origin_run_id: null,
  origin_dream_note_id: null,
  origin_extraction_id: null,
  confidence: 1,
  supersedes_item_id: null,
  source_revision: 0,
  verified_revision: 0,
  stale_reason: null,
  created_at: "2026-07-23T00:00:00Z",
  updated_at: "2026-07-23T00:00:00Z",
} as const;

describe("memory schemas", () => {
  it("parses paginated Memory items", () => {
    expect(
      memoryItemPageSchema.parse({ total_count: 1, items: [item], limit: 50, offset: 0 }),
    ).toMatchObject({ total_count: 1, items: [{ id: "memory-1" }] });
  });

  it("parses evidence and revision history while rejecting illegal enums", () => {
    const detail = {
      item,
      evidence: [
        {
          id: "evidence-1",
          record_kind: "session",
          source_id: "codex",
          session_id: "session-1",
          question_id: "question-1",
          turn_id: "turn-1",
          part_id: "part-1",
          block_id: "part-1",
          content_hash: "sha256:one",
          excerpt: "Evidence",
          translated_excerpt: null,
          event_time: null,
          source_revision: 1,
          source_unavailable: false,
          created_at: "2026-07-23T00:00:00Z",
          updated_at: "2026-07-23T00:00:00Z",
        },
      ],
      revisions: [
        {
          id: "revision-1",
          item_id: "memory-1",
          revision_number: 1,
          change_kind: "create",
          kind: "decision",
          status: "active",
          title: item.title,
          content_markdown: item.content_markdown,
          scope: item.scope,
          scope_fingerprint: item.scope_fingerprint,
          origin: "manual",
          confidence: 1,
          supersedes_item_id: null,
          source_revision: 0,
          verified_revision: 0,
          stale_reason: null,
          changed_at: "2026-07-23T00:00:00Z",
        },
      ],
    };

    expect(memoryItemDetailSchema.parse(detail).evidence).toHaveLength(1);
    expect(() => memoryItemDetailSchema.parse({ ...detail, item: { ...item, status: "invented" } })).toThrow();
  });
});
