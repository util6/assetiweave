export type MemoryItemKind = "preference" | "decision" | "method" | "context" | "follow_up";
export type MemoryItemStatus = "candidate" | "active" | "completed" | "superseded" | "archived" | "rejected";
export type MemoryItemOrigin = "manual" | "auto_dream" | "deep_recall" | "full_organize";
export type MemoryStaleReason = "evidence_changed" | "evidence_missing" | "source_unavailable";
export type MemoryRevisionChangeKind = "create" | "accept" | "update" | "status" | "supersedes";
export type MemoryEvidenceRecordKind = "session" | "web";

export interface MemoryScope {
  app_id: string | null;
  source_id: string | null;
  project_path: string | null;
  session_id: string | null;
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
