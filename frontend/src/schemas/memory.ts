import { z } from "zod";
import type { MemoryItemDetail, MemoryItemPage } from "../types/memory";

export const memoryItemKindSchema = z.enum(["preference", "decision", "method", "context", "follow_up"]);
export const memoryItemStatusSchema = z.enum(["candidate", "active", "completed", "superseded", "archived", "rejected"]);
export const memoryItemOriginSchema = z.enum(["manual", "auto_dream", "deep_recall", "full_organize"]);
export const memoryStaleReasonSchema = z.enum(["evidence_changed", "evidence_missing", "source_unavailable"]);
export const memoryRevisionChangeKindSchema = z.enum(["create", "accept", "update", "status", "supersedes"]);
export const memoryEvidenceRecordKindSchema = z.enum(["session", "web"]);

export const memoryScopeSchema = z.object({
  app_id: z.string().nullable(),
  source_id: z.string().nullable(),
  project_path: z.string().nullable(),
  session_id: z.string().nullable(),
});

export const memoryItemSchema = z.object({
  id: z.string().min(1),
  kind: memoryItemKindSchema,
  status: memoryItemStatusSchema,
  title: z.string().min(1),
  content_markdown: z.string().min(1),
  scope: memoryScopeSchema,
  scope_fingerprint: z.string().min(1),
  origin: memoryItemOriginSchema,
  origin_run_id: z.string().nullable(),
  origin_dream_note_id: z.string().nullable(),
  origin_extraction_id: z.string().nullable(),
  confidence: z.number().min(0).max(1).nullable(),
  supersedes_item_id: z.string().nullable(),
  source_revision: z.number().int().nonnegative(),
  verified_revision: z.number().int().nonnegative(),
  stale_reason: memoryStaleReasonSchema.nullable(),
  created_at: z.string().min(1),
  updated_at: z.string().min(1),
});

export const memoryEvidenceSnapshotSchema = z.object({
  id: z.string().min(1),
  record_kind: memoryEvidenceRecordKindSchema,
  source_id: z.string().nullable(),
  session_id: z.string().min(1),
  question_id: z.string().nullable(),
  turn_id: z.string().nullable(),
  part_id: z.string().nullable(),
  block_id: z.string().min(1),
  content_hash: z.string().min(1),
  excerpt: z.string(),
  translated_excerpt: z.string().nullable(),
  event_time: z.string().nullable(),
  source_revision: z.number().int().nonnegative(),
  source_unavailable: z.boolean(),
  created_at: z.string().min(1),
  updated_at: z.string().min(1),
});

export const memoryItemRevisionSchema = z.object({
  id: z.string().min(1),
  item_id: z.string().min(1),
  revision_number: z.number().int().positive(),
  change_kind: memoryRevisionChangeKindSchema,
  kind: memoryItemKindSchema,
  status: memoryItemStatusSchema,
  title: z.string(),
  content_markdown: z.string(),
  scope: memoryScopeSchema,
  scope_fingerprint: z.string().min(1),
  origin: memoryItemOriginSchema,
  confidence: z.number().min(0).max(1).nullable(),
  supersedes_item_id: z.string().nullable(),
  source_revision: z.number().int().nonnegative(),
  verified_revision: z.number().int().nonnegative(),
  stale_reason: memoryStaleReasonSchema.nullable(),
  changed_at: z.string().min(1),
});

export const memoryItemPageSchema: z.ZodType<MemoryItemPage> = z.object({
  total_count: z.number().int().nonnegative(),
  items: z.array(memoryItemSchema),
  limit: z.number().int().positive(),
  offset: z.number().int().nonnegative(),
});

export const memoryItemDetailSchema: z.ZodType<MemoryItemDetail> = z.object({
  item: memoryItemSchema,
  evidence: z.array(memoryEvidenceSnapshotSchema),
  revisions: z.array(memoryItemRevisionSchema),
});
