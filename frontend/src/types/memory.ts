export type MemoryRecordKind = "session" | "web";
export type RecentConversationView = "project" | "time";
export type RecentMemoryEventCategory =
  | "progress"
  | "decision"
  | "research"
  | "verification"
  | "blocker"
  | "follow_up";
export type MemoryRecallSessionStatus =
  "active" | "completed" | "failed" | "cancelled" | "resume_unavailable";
export type MemoryRecallTurnStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"
  | "resume_unavailable";
export type MemoryPublicTaskStatus =
  "pending" | "running" | "cancelling" | "succeeded" | "failed" | "cancelled";

export interface RecentMemoryEvent {
  id: string;
  category: RecentMemoryEventCategory;
  title: string;
  summary: string;
  occurred_at: string;
}

export interface RecentMemorySession {
  session: { title: string; updated_at: string | null };
  project_path: string | null;
  last_activity_at: string;
  source_agent: string;
  question_count: number;
  turn_count: number;
  recent_events: RecentMemoryEvent[];
}

export interface RecentMemoryEventTarget {
  record_kind: MemoryRecordKind;
  session_id: string;
  question_id: string | null;
  turn_id: string | null;
  block_id: string | null;
}

export interface MemoryScope {
  app_id: string | null;
  source_id: string | null;
  project_path: string | null;
  session_id: string | null;
}

export interface MemoryNavigationTarget {
  record_kind: MemoryRecordKind;
  source_id: string | null;
  session_id: string;
  question_id: string | null;
  turn_id: string | null;
  part_id: string | null;
  block_id: string | null;
}

export interface MemoryRecallSearchHit {
  record_kind: MemoryRecordKind;
  source_id: string;
  session_id: string;
  session_title: string;
  project_path: string | null;
  question_id: string;
  question_index: number;
  turn_id: string | null;
  part_id: string | null;
  block_id: string;
  card_type: string;
  snippet: string;
  lexical_score: number;
  semantic_score: number;
  score: number;
  sources: string[];
}

export interface MemoryRecallSearchResult {
  query: string;
  backend: string;
  total_count: number;
  hits: MemoryRecallSearchHit[];
}

export interface MemoryRecallSessionReference {
  recordKind: MemoryRecordKind;
  sessionId: string;
  questionId: string | null;
}

export interface MemoryRecallContentReference {
  recordKind: MemoryRecordKind;
  sessionId: string;
  questionId: string;
  turnId: string | null;
  partId: string | null;
  blockId: string;
}

export interface MemoryRecallStructuredOutput {
  answer: string;
  sessionReferences: MemoryRecallSessionReference[];
  contentReferences: MemoryRecallContentReference[];
  followUpSuggestions: string[];
}

export interface MemoryRecallTurn {
  id: string;
  sessionId: string;
  sequence: number;
  conversationSessionId: string;
  conversationTurnId: string;
  status: MemoryRecallTurnStatus;
  userText: string;
  structuredOutput: MemoryRecallStructuredOutput | null;
  lastError: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface MemoryRecallSession {
  id: string;
  status: MemoryRecallSessionStatus;
  scope: MemoryScope;
  executionContextKey: string;
  agentId: string;
  model: string | null;
  turnCount: number;
  activeTurnId: string | null;
  lastError: string | null;
  createdAt: string;
  updatedAt: string;
  turns: MemoryRecallTurn[];
}

export interface MemoryContextReference {
  kind: string;
  id: string;
  source_revision: number | null;
}

export interface MemoryContextResult {
  text: string;
  revision: string;
  generated_at: string | null;
  estimated_tokens: number;
  token_budget: number;
  references: MemoryContextReference[];
  global_version: Record<string, unknown> | null;
  project_version: Record<string, unknown> | null;
  project_sources: Record<string, unknown>[];
}

export interface MemoryProjectView {
  project: Record<string, unknown>;
  version: Record<string, unknown> | null;
  sources: Record<string, unknown>[];
}

export interface MemoryRebuildResult {
  scope: MemoryScope;
  queued: boolean;
  scheduled_tasks: number;
}

export interface MemoryTaskProgress {
  current: number;
  total: number | null;
  note: string | null;
}

export interface MemoryTaskView {
  id: string;
  status: MemoryPublicTaskStatus;
  kind: string;
  progress: MemoryTaskProgress | null;
  started_at: string;
  finished_at: string | null;
  result: unknown;
  error: {
    code: string;
    message: string;
    retryable: boolean;
    details?: unknown;
  } | null;
  detail: Record<string, unknown>;
}

export type RecentMemoryStatus = "empty" | "generating" | "ready" | "update_failed";
export type RecentSnapshotPublicationKind = "generated" | "reused";
export type SourceAvailability = "available" | "partially_unavailable" | "unavailable";

export interface RecentMemoryErrorView {
  code: string;
  message: string;
  retryable: boolean;
}

export interface RecentSessionReferenceView {
  sourceId: string;
  sessionId: string;
  sessionTitle: string;
  sourceAgent: string;
  lastActivityAt: string;
  available: boolean;
  unavailableReason: string | null;
}

export interface RecentMemoryItemView {
  itemId: string;
  revisionId: string;
  category: string;
  status: string;
  title: string;
  summary: string;
  rationale: string;
  occurredAt: string;
  recommendationRank: number | null;
  sourceAvailability: SourceAvailability;
  sessionReferences: RecentSessionReferenceView[];
}

export interface RecentProjectView {
  projectKey: string;
  projectTitle: string;
  projectPath: string | null;
  summary: string;
  noMaterialChange: boolean;
  latestActivityAt: string;
  sourceSessionCount: number;
  items: RecentMemoryItemView[];
}

export interface RecentMemorySnapshotView {
  snapshotId: string;
  sequence: number;
  targetWatermark: string;
  windowStart: string;
  windowEnd: string;
  windowHours: number;
  publicationKind: RecentSnapshotPublicationKind;
  reusedFromSnapshotId: string | null;
  contentGeneratedAt: string;
  publishedAt: string;
  projects: RecentProjectView[];
}

export interface RecentMemoryStateView {
  status: RecentMemoryStatus;
  snapshot: RecentMemorySnapshotView | null;
  latestAttemptTaskId: string | null;
  latestAttemptError: RecentMemoryErrorView | null;
}
