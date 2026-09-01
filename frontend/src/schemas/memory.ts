import { z } from "zod";
import type {
  MemoryContextResult,
  MemoryProjectView,
  MemoryRecallSearchResult,
  MemoryRecallSession,
  MemoryRebuildResult,
  MemoryTaskView,
  RecentMemoryEventTarget,
  RecentMemorySession,
} from "../types/memory";

export const memoryScopeSchema = z.object({
  app_id: z.string().nullable(),
  source_id: z.string().nullable(),
  project_path: z.string().nullable(),
  session_id: z.string().nullable(),
});

export const recentMemorySessionSchema: z.ZodType<RecentMemorySession> = z.object({
  session: z.object({ title: z.string(), updated_at: z.string().nullable() }),
  project_path: z.string().nullable(),
  last_activity_at: z.string().min(1),
  source_agent: z.string(),
  question_count: z.number().int().nonnegative(),
  turn_count: z.number().int().nonnegative(),
  recent_events: z.array(z.object({
    id: z.string().min(1),
    category: z.enum(["progress", "decision", "research", "verification", "blocker", "follow_up"]),
    title: z.string(),
    summary: z.string(),
    occurred_at: z.string().min(1),
  })),
});

export const recentMemoryEventTargetSchema: z.ZodType<RecentMemoryEventTarget> = z.object({
  record_kind: z.enum(["session", "web"]),
  session_id: z.string().min(1),
  question_id: z.string().nullable(),
  turn_id: z.string().nullable(),
  block_id: z.string().nullable(),
});

export const memoryContextResultSchema: z.ZodType<MemoryContextResult> = z.object({
  text: z.string(),
  revision: z.string().min(1),
  generated_at: z.string().nullable(),
  estimated_tokens: z.number().int().nonnegative(),
  token_budget: z.number().int().positive(),
  references: z.array(z.object({ kind: z.string().min(1), id: z.string().min(1), source_revision: z.number().int().nonnegative().nullable() })),
  global_version: z.record(z.string(), z.unknown()).nullable(),
  project_version: z.record(z.string(), z.unknown()).nullable(),
  project_sources: z.array(z.record(z.string(), z.unknown())),
});

export const memoryProjectViewSchema: z.ZodType<MemoryProjectView> = z.object({
  project: z.record(z.string(), z.unknown()),
  version: z.record(z.string(), z.unknown()).nullable(),
  sources: z.array(z.record(z.string(), z.unknown())),
});

export const memoryRebuildResultSchema: z.ZodType<MemoryRebuildResult> = z.object({
  scope: memoryScopeSchema,
  queued: z.boolean(),
  scheduled_tasks: z.number().int().nonnegative(),
});

const memoryRecallSearchHitSchema = z.object({
  record_kind: z.enum(["session", "web"]),
  source_id: z.string().min(1),
  session_id: z.string().min(1),
  session_title: z.string(),
  project_path: z.string().nullable(),
  question_id: z.string().min(1),
  question_index: z.number().int(),
  turn_id: z.string().nullable(),
  part_id: z.string().nullable(),
  block_id: z.string().min(1),
  card_type: z.string().min(1),
  snippet: z.string(),
  lexical_score: z.number().int().nonnegative(),
  semantic_score: z.number().int().nonnegative(),
  score: z.number().int().nonnegative(),
  sources: z.array(z.string()),
});

export const memoryRecallSearchResultSchema: z.ZodType<MemoryRecallSearchResult> = z.object({
  query: z.string(),
  backend: z.string().min(1),
  total_count: z.number().int().nonnegative(),
  hits: z.array(memoryRecallSearchHitSchema),
});

const memoryRecallSessionReferenceSchema = z.object({
  recordKind: z.enum(["session", "web"]),
  sessionId: z.string().min(1),
  questionId: z.string().nullable(),
});
const memoryRecallContentReferenceSchema = z.object({
  recordKind: z.enum(["session", "web"]),
  sessionId: z.string().min(1),
  questionId: z.string().min(1),
  turnId: z.string().nullable(),
  partId: z.string().nullable(),
  blockId: z.string().min(1),
});

export const memoryRecallSessionSchema: z.ZodType<MemoryRecallSession> = z.object({
  id: z.string().min(1),
  status: z.enum(["active", "completed", "failed", "cancelled", "resume_unavailable"]),
  scope: memoryScopeSchema,
  executionContextKey: z.string().min(1),
  agentId: z.string().min(1),
  model: z.string().nullable(),
  turnCount: z.number().int().nonnegative(),
  activeTurnId: z.string().nullable(),
  lastError: z.string().nullable(),
  createdAt: z.string().min(1),
  updatedAt: z.string().min(1),
  turns: z.array(z.object({
    id: z.string().min(1),
    sessionId: z.string().min(1),
    sequence: z.number().int().nonnegative(),
    conversationSessionId: z.string().min(1),
    conversationTurnId: z.string().min(1),
    status: z.enum(["queued", "running", "completed", "failed", "cancelled", "resume_unavailable"]),
    userText: z.string(),
    structuredOutput: z.object({
      answer: z.string(),
      sessionReferences: z.array(memoryRecallSessionReferenceSchema),
      contentReferences: z.array(memoryRecallContentReferenceSchema),
      followUpSuggestions: z.array(z.string()),
    }).strict().nullable(),
    lastError: z.string().nullable(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })),
});

export const memoryTaskViewSchema: z.ZodType<MemoryTaskView> = z.object({
  id: z.string().min(1),
  status: z.enum(["pending", "running", "cancelling", "succeeded", "failed", "cancelled"]),
  kind: z.string().min(1),
  progress: z.object({ current: z.number().int().nonnegative(), total: z.number().int().nonnegative().nullable(), note: z.string().nullable() }).nullable(),
  started_at: z.string().min(1),
  finished_at: z.string().nullable(),
  result: z.unknown().nullable(),
  error: z.object({ code: z.string(), message: z.string(), retryable: z.boolean(), details: z.unknown().optional() }).nullable(),
  detail: z.record(z.string(), z.unknown()),
});
