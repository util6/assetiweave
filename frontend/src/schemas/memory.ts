import { z } from "zod";
import type {
  MemoryDreamPreview,
  MemoryDreamNoteDetail,
  MemoryDreamNotePage,
  MemoryDreamRunResult,
  MemoryItemDetail,
  MemoryItemPage,
  MemoryTaskSnapshot,
  MemoryOverview,
  MemoryRecallPreview,
  MemoryRecallRunResult,
  MemoryVerifyResult,
} from "../types/memory";

export const memoryItemKindSchema = z.enum(["preference", "decision", "method", "context", "follow_up"]);
export const memoryItemStatusSchema = z.enum(["candidate", "active", "completed", "superseded", "archived", "rejected"]);
export const memoryItemOriginSchema = z.enum(["manual", "auto_dream", "deep_recall", "full_organize"]);
export const memoryStaleReasonSchema = z.enum(["evidence_changed", "evidence_missing", "source_unavailable"]);
export const memoryRevisionChangeKindSchema = z.enum(["create", "accept", "update", "status", "supersedes"]);
export const memoryEvidenceRecordKindSchema = z.enum(["session", "web"]);
export const memoryRunKindSchema = z.enum(["auto_dream", "deep_recall", "full_organize"]);
export const memoryDreamTriggerSchema = z.enum(["automatic", "manual"]);
export const memoryDreamGateKindSchema = z.enum(["enabled", "runtime", "time", "sessions", "lock", "budget"]);
export const memoryTaskStatusSchema = z.enum(["running", "completed", "failed", "cancelled"]);
export const memoryDreamNoteStatusSchema = z.enum(["active", "promoted", "archived", "stale"]);
export const memoryRecallModeSchema = z.enum(["exact", "full"]);

export const memoryScopeSchema = z.object({
  app_id: z.string().nullable(),
  source_id: z.string().nullable(),
  project_path: z.string().nullable(),
  session_id: z.string().nullable(),
});

export const memoryDreamCursorSchema = z.object({
  session_sort_key: z.string(),
  question_offset: z.number().int().nonnegative(),
});

export const memoryDreamDeltaSessionSchema = z.object({
  record_kind: memoryEvidenceRecordKindSchema,
  session_id: z.string().min(1),
  source_id: z.string().min(1),
  adapter_id: z.string().min(1),
  project_path: z.string().nullable(),
  title: z.string(),
  imported_at: z.string().min(1),
  session_sort_key: z.string().min(1),
  available_question_count: z.number().int().nonnegative(),
  questions: z.array(z.object({
    id: z.string().min(1),
    question_index: z.number().int(),
    input_char_count: z.number().int().nonnegative(),
    input_truncated: z.boolean(),
  })),
  input_char_count: z.number().int().nonnegative(),
});

export const memoryDreamPreviewSchema: z.ZodType<MemoryDreamPreview> = z.object({
  scope: memoryScopeSchema,
  scope_fingerprint: z.string().min(1),
  trigger: memoryDreamTriggerSchema,
  ready: z.boolean(),
  gates: z.array(z.object({
    gate: memoryDreamGateKindSchema,
    passed: z.boolean(),
    reason_code: z.string().min(1),
    message: z.string(),
    actual: z.number().int().nullable(),
    required: z.number().int().nullable(),
  })),
  state: z.object({
    scope: memoryScopeSchema,
    scope_fingerprint: z.string().min(1),
    last_successful_run_id: z.string().nullable(),
    last_successful_at: z.string().nullable(),
    source_revision_cursor: z.number().int().nonnegative(),
    session_cursor: memoryDreamCursorSchema.nullable(),
    next_gate_at: z.string().nullable(),
    last_error_kind: z.string().nullable(),
    last_error_message: z.string().nullable(),
    updated_at: z.string().min(1),
  }).nullable(),
  source_revision_start: z.number().int().nonnegative(),
  source_revision_end: z.number().int().nonnegative(),
  cursor_start: memoryDreamCursorSchema.nullable(),
  cursor_end: memoryDreamCursorSchema.nullable(),
  stable_before: z.string().min(1),
  sessions: z.array(memoryDreamDeltaSessionSchema),
  session_count: z.number().int().nonnegative(),
  question_count: z.number().int().nonnegative(),
  input_char_count: z.number().int().nonnegative(),
  max_sessions: z.number().int().positive(),
  max_questions: z.number().int().positive(),
  max_input_chars: z.number().int().positive(),
  has_more: z.boolean(),
});

export const memoryDreamRunResultSchema: z.ZodType<MemoryDreamRunResult> = z.object({
  dry_run: z.boolean(),
  run_id: z.string().nullable(),
  note_id: z.string().nullable(),
  markdown: z.string().nullable(),
  preview: memoryDreamPreviewSchema,
});

export const memoryTaskSnapshotSchema: z.ZodType<MemoryTaskSnapshot> = z.object({
  id: z.string().min(1),
  status: memoryTaskStatusSchema,
  kind: memoryRunKindSchema,
  scope: memoryScopeSchema,
  scope_fingerprint: z.string().min(1),
  trigger: memoryDreamTriggerSchema,
  dry_run: z.boolean(),
  phase: z.string().min(1),
  processed_count: z.number().int().nonnegative(),
  total_count: z.number().int().nonnegative(),
  run_id: z.string().nullable(),
  cancel_requested: z.boolean(),
  started_at: z.string().min(1),
  finished_at: z.string().nullable(),
  result: z.unknown().nullable(),
  error: z.object({
    code: z.string(),
    message: z.string(),
    retryable: z.boolean(),
    details: z.unknown().optional(),
  }).nullable(),
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

export const memoryDreamNoteSchema = z.object({
  id: z.string().min(1),
  run_id: z.string().min(1),
  scope: memoryScopeSchema,
  scope_fingerprint: z.string().min(1),
  markdown: z.string(),
  session_count: z.number().int().nonnegative(),
  question_count: z.number().int().nonnegative(),
  evidence_count: z.number().int().nonnegative(),
  source_revision: z.number().int().nonnegative(),
  status: memoryDreamNoteStatusSchema,
  created_at: z.string().min(1),
  updated_at: z.string().min(1),
});

export const memoryDreamNoteDetailSchema: z.ZodType<MemoryDreamNoteDetail> = z.object({
  note: memoryDreamNoteSchema,
  evidence: z.array(memoryEvidenceSnapshotSchema),
});

export const memoryDreamNotePageSchema: z.ZodType<MemoryDreamNotePage> = z.object({
  total_count: z.number().int().nonnegative(),
  items: z.array(memoryDreamNoteSchema),
  limit: z.number().int().positive(),
  offset: z.number().int().nonnegative(),
});

export const memoryOverviewSchema: z.ZodType<MemoryOverview> = z.object({
  follow_ups: z.array(memoryItemSchema),
  recent_items: z.array(memoryItemSchema),
  candidate_count: z.number().int().nonnegative(),
  latest_dream: memoryDreamNoteDetailSchema.nullable(),
  stale_count: z.number().int().nonnegative(),
  dream_status: memoryDreamPreviewSchema,
});

const memoryRecallEvidenceSnapshotSchema = z.object({
  record_kind: memoryEvidenceRecordKindSchema, source_id: z.string().nullable(), session_id: z.string().min(1),
  question_id: z.string().nullable(), turn_id: z.string().nullable(), part_id: z.string().nullable(),
  block_id: z.string().min(1), content_hash: z.string().min(1), excerpt: z.string(),
  translated_excerpt: z.string().nullable(), event_time: z.string().nullable(),
  source_revision: z.number().int().nonnegative(), source_unavailable: z.boolean(),
});

const memoryRecallEvidenceSchema = z.object({ reference: z.string().min(1), card_type: z.string().min(1), snapshot: memoryRecallEvidenceSnapshotSchema });
const memoryRecallQuestionSchema = z.object({
  record_kind: memoryEvidenceRecordKindSchema, source_id: z.string().min(1), session_id: z.string().min(1),
  session_title: z.string(), project_path: z.string().nullable(), question_id: z.string().min(1),
  question_index: z.number().int(), question_title: z.string(), evidence_ids: z.array(z.string().min(1)),
  input_char_count: z.number().int().nonnegative(),
});

export const memoryRecallPreviewSchema: z.ZodType<MemoryRecallPreview> = z.object({
  mode: memoryRecallModeSchema, scope: memoryScopeSchema, query: z.string().nullable(), backend: z.string().min(1),
  source_revision: z.number().int().nonnegative(), total_question_count: z.number().int().nonnegative(),
  selected_question_count: z.number().int().nonnegative(), skipped_question_count: z.number().int().nonnegative(),
  evidence_count: z.number().int().nonnegative(), input_char_count: z.number().int().nonnegative(),
  truncated: z.boolean(), include_unavailable: z.boolean(), questions: z.array(memoryRecallQuestionSchema),
  evidence: z.array(memoryRecallEvidenceSchema), formal_matches: z.array(memoryItemSchema), dream_matches: z.array(memoryDreamNoteSchema),
});

const memoryRawMemorySchema = z.object({ kind: memoryItemKindSchema, text: z.string(), evidence_ids: z.array(z.string()), confidence: z.number().min(0).max(1).nullable(), uncertainty: z.string().nullable() });
const memoryExtractionSchema = z.object({ id: z.string(), run_id: z.string(), batch_index: z.number().int().nonnegative(), raw_memories: z.array(memoryRawMemorySchema), session_summary: z.string(), question_count: z.number().int().nonnegative(), input_char_count: z.number().int().nonnegative(), evidence_count: z.number().int().nonnegative(), validation_status: z.enum(["pending", "valid", "invalid"]), attempt_count: z.number().int().positive(), error_message: z.string().nullable(), created_at: z.string(), updated_at: z.string() });
const memoryRecallClaimSchema = z.object({ text: z.string(), evidence_ids: z.array(z.string()) });
const memoryRecallCandidateSchema = z.object({ kind: memoryItemKindSchema, title: z.string(), content_markdown: z.string(), evidence_ids: z.array(z.string()), confidence: z.number().min(0).max(1).nullable(), supersedes_item_id: z.string().nullable() });
const memoryRecallConflictSchema = z.object({ description: z.string(), evidence_ids: z.array(z.string()) });

export const memoryRecallRunResultSchema: z.ZodType<MemoryRecallRunResult> = z.object({
  run_id: z.string().nullable(), preview: memoryRecallPreviewSchema, synthesized: z.boolean(), answer_markdown: z.string().nullable(),
  claims: z.array(memoryRecallClaimSchema), memory_candidates: z.array(memoryRecallCandidateSchema), conflicts: z.array(memoryRecallConflictSchema),
  insufficient_evidence: z.boolean(), extractions: z.array(memoryExtractionSchema),
});

export const memoryVerifyResultSchema: z.ZodType<MemoryVerifyResult> = z.object({
  source_revision: z.number().int().nonnegative(),
  unchanged_revision: z.boolean(),
  items: z.array(memoryItemDetailSchema),
});
