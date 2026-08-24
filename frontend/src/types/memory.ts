export type MemoryItemKind = "preference" | "decision" | "method" | "context" | "follow_up";
export type MemoryItemStatus = "candidate" | "active" | "completed" | "superseded" | "archived" | "rejected";
export type MemoryItemOrigin = "manual" | "auto_dream" | "deep_recall" | "full_organize";
export type MemoryStaleReason = "evidence_changed" | "evidence_missing" | "source_unavailable";
export type MemoryRevisionChangeKind = "create" | "accept" | "update" | "status" | "supersedes";
export type MemoryEvidenceRecordKind = "session" | "web";
export type MemoryRunKind = "auto_dream" | "deep_recall" | "full_organize";
export type MemoryDreamTrigger = "automatic" | "manual";
export type MemoryDreamGateKind = "enabled" | "runtime" | "time" | "sessions" | "lock" | "budget";
export type MemoryTaskStatus = "running" | "completed" | "failed" | "cancelled";
export type MemoryDreamNoteStatus = "active" | "promoted" | "archived" | "stale";
export type MemoryRecallMode = "exact" | "full";
export type MemoryExtractionValidationStatus = "pending" | "valid" | "invalid";

export interface MemoryScope {
  app_id: string | null;
  source_id: string | null;
  project_path: string | null;
  session_id: string | null;
}

export interface MemoryDreamGateResult {
  gate: MemoryDreamGateKind;
  passed: boolean;
  reason_code: string;
  message: string;
  actual: number | null;
  required: number | null;
}

export interface MemoryDreamCursor {
  session_sort_key: string;
  question_offset: number;
}

export interface MemoryDreamDeltaQuestion {
  id: string;
  question_index: number;
  input_char_count: number;
  input_truncated: boolean;
}

export interface MemoryDreamDeltaSession {
  record_kind: MemoryEvidenceRecordKind;
  session_id: string;
  source_id: string;
  adapter_id: string;
  project_path: string | null;
  title: string;
  imported_at: string;
  session_sort_key: string;
  available_question_count: number;
  questions: MemoryDreamDeltaQuestion[];
  input_char_count: number;
}

export interface MemoryDreamState {
  scope: MemoryScope;
  scope_fingerprint: string;
  last_successful_run_id: string | null;
  last_successful_at: string | null;
  source_revision_cursor: number;
  session_cursor: MemoryDreamCursor | null;
  next_gate_at: string | null;
  last_error_kind: string | null;
  last_error_message: string | null;
  updated_at: string;
}

export interface MemoryDreamPreview {
  scope: MemoryScope;
  scope_fingerprint: string;
  trigger: MemoryDreamTrigger;
  ready: boolean;
  gates: MemoryDreamGateResult[];
  state: MemoryDreamState | null;
  source_revision_start: number;
  source_revision_end: number;
  cursor_start: MemoryDreamCursor | null;
  cursor_end: MemoryDreamCursor | null;
  stable_before: string;
  sessions: MemoryDreamDeltaSession[];
  session_count: number;
  question_count: number;
  input_char_count: number;
  max_sessions: number;
  max_questions: number;
  max_input_chars: number;
  has_more: boolean;
}

export interface MemoryDreamRunResult {
  dry_run: boolean;
  run_id: string | null;
  note_id: string | null;
  markdown: string | null;
  preview: MemoryDreamPreview;
}

export interface MemoryDreamNote {
  id: string;
  run_id: string;
  scope: MemoryScope;
  scope_fingerprint: string;
  markdown: string;
  session_count: number;
  question_count: number;
  evidence_count: number;
  source_revision: number;
  status: MemoryDreamNoteStatus;
  created_at: string;
  updated_at: string;
}

export interface MemoryDreamNoteDetail {
  note: MemoryDreamNote;
  evidence: MemoryEvidenceSnapshot[];
}

export interface MemoryDreamNotePage {
  total_count: number;
  items: MemoryDreamNote[];
  limit: number;
  offset: number;
}

export interface MemoryOverview {
  follow_ups: MemoryItem[];
  recent_items: MemoryItem[];
  candidate_count: number;
  latest_dream: MemoryDreamNoteDetail | null;
  stale_count: number;
  dream_status: MemoryDreamPreview;
}

export interface MemoryRecallEvidenceSnapshot {
  record_kind: MemoryEvidenceRecordKind;
  source_id: string | null;
  session_id: string;
  question_id: string | null;
  turn_id: string | null;
  part_id: string | null;
  block_id: string;
  content_hash: string;
  excerpt: string;
  translated_excerpt: string | null;
  event_time: string | null;
  source_revision: number;
  source_unavailable: boolean;
}

export interface MemoryRecallEvidence {
  reference: string;
  card_type: string;
  snapshot: MemoryRecallEvidenceSnapshot;
}

export interface MemoryRecallQuestion {
  record_kind: MemoryEvidenceRecordKind;
  source_id: string;
  session_id: string;
  session_title: string;
  project_path: string | null;
  question_id: string;
  question_index: number;
  question_title: string;
  evidence_ids: string[];
  input_char_count: number;
}

export interface MemoryRecallPreview {
  mode: MemoryRecallMode;
  scope: MemoryScope;
  query: string | null;
  backend: string;
  source_revision: number;
  total_question_count: number;
  selected_question_count: number;
  skipped_question_count: number;
  evidence_count: number;
  input_char_count: number;
  truncated: boolean;
  include_unavailable: boolean;
  questions: MemoryRecallQuestion[];
  evidence: MemoryRecallEvidence[];
  formal_matches: MemoryItem[];
  dream_matches: MemoryDreamNote[];
}

export interface MemoryRawMemory {
  kind: MemoryItemKind;
  text: string;
  evidence_ids: string[];
  confidence: number | null;
  uncertainty: string | null;
}

export interface MemoryExtraction {
  id: string;
  run_id: string;
  batch_index: number;
  raw_memories: MemoryRawMemory[];
  session_summary: string;
  question_count: number;
  input_char_count: number;
  evidence_count: number;
  validation_status: MemoryExtractionValidationStatus;
  attempt_count: number;
  error_message: string | null;
  created_at: string;
  updated_at: string;
}

export interface MemoryRecallClaim { text: string; evidence_ids: string[]; }
export interface MemoryRecallCandidate {
  kind: MemoryItemKind;
  title: string;
  content_markdown: string;
  evidence_ids: string[];
  confidence: number | null;
  supersedes_item_id: string | null;
}
export interface MemoryRecallConflict { description: string; evidence_ids: string[]; }

export interface MemoryRecallRunResult {
  run_id: string | null;
  preview: MemoryRecallPreview;
  synthesized: boolean;
  answer_markdown: string | null;
  claims: MemoryRecallClaim[];
  memory_candidates: MemoryRecallCandidate[];
  conflicts: MemoryRecallConflict[];
  insufficient_evidence: boolean;
  extractions: MemoryExtraction[];
}

export interface MemoryVerifyResult {
  source_revision: number;
  unchanged_revision: boolean;
  items: MemoryItemDetail[];
}

export interface MemoryRecallPreviewParams {
  mode: MemoryRecallMode;
  scope?: MemoryScope;
  query?: string | null;
  since?: string | null;
  until?: string | null;
  include_unavailable?: boolean;
  limit?: number;
  offset?: number;
}

import type { AppErrorView } from "./index";

export interface MemoryTaskSnapshot {
  id: string;
  status: MemoryTaskStatus;
  kind: MemoryRunKind;
  scope: MemoryScope;
  scope_fingerprint: string;
  trigger: MemoryDreamTrigger;
  dry_run: boolean;
  phase: string;
  processed_count: number;
  total_count: number;
  run_id: string | null;
  cancel_requested: boolean;
  started_at: string;
  finished_at: string | null;
  result: unknown;
  error: AppErrorView | null;
}

export interface MemoryTaskStartParams {
  kind: MemoryRunKind;
  scope?: MemoryScope;
  trigger?: MemoryDreamTrigger;
  dry_run?: boolean;
  recall?: MemoryRecallPreviewParams;
  synthesize?: boolean;
}

export interface MemoryItem {
  id: string;
  kind: MemoryItemKind;
  status: MemoryItemStatus;
  title: string;
  content_markdown: string;
  scope: MemoryScope;
  scope_fingerprint: string;
  origin: MemoryItemOrigin;
  origin_run_id: string | null;
  origin_dream_note_id: string | null;
  origin_extraction_id: string | null;
  confidence: number | null;
  supersedes_item_id: string | null;
  source_revision: number;
  verified_revision: number;
  stale_reason: MemoryStaleReason | null;
  created_at: string;
  updated_at: string;
}

export interface MemoryEvidenceSnapshot {
  id: string;
  record_kind: MemoryEvidenceRecordKind;
  source_id: string | null;
  session_id: string;
  question_id: string | null;
  turn_id: string | null;
  part_id: string | null;
  block_id: string;
  content_hash: string;
  excerpt: string;
  translated_excerpt: string | null;
  event_time: string | null;
  source_revision: number;
  source_unavailable: boolean;
  created_at: string;
  updated_at: string;
}

export interface MemoryItemRevision {
  id: string;
  item_id: string;
  revision_number: number;
  change_kind: MemoryRevisionChangeKind;
  kind: MemoryItemKind;
  status: MemoryItemStatus;
  title: string;
  content_markdown: string;
  scope: MemoryScope;
  scope_fingerprint: string;
  origin: MemoryItemOrigin;
  confidence: number | null;
  supersedes_item_id: string | null;
  source_revision: number;
  verified_revision: number;
  stale_reason: MemoryStaleReason | null;
  changed_at: string;
}

export interface MemoryItemDetail {
  item: MemoryItem;
  evidence: MemoryEvidenceSnapshot[];
  revisions: MemoryItemRevision[];
}

export interface MemoryItemPage {
  total_count: number;
  items: MemoryItem[];
  limit: number;
  offset: number;
}

export interface MemoryItemPageResult extends MemoryItemPage {
  availability: "tauri" | "browser_preview";
}

export interface MemoryItemListParams {
  kinds?: MemoryItemKind[];
  statuses?: MemoryItemStatus[];
  origins?: MemoryItemOrigin[];
  scope?: MemoryScope | null;
  stale_only?: boolean;
  limit?: number;
  offset?: number;
}

export interface MemoryItemCreateParams {
  kind: MemoryItemKind;
  title: string;
  content_markdown: string;
  scope?: MemoryScope;
  confidence?: number | null;
  evidence_ids?: string[];
}

export interface MemoryItemUpdateParams {
  item_id: string;
  kind?: MemoryItemKind | null;
  title?: string | null;
  content_markdown?: string | null;
  scope?: MemoryScope | null;
  confidence?: number | null;
  evidence_ids?: string[] | null;
}

export interface MemoryCandidateAcceptParams extends MemoryItemUpdateParams {}
