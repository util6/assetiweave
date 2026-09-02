import { z } from "zod";

const teamRunStateSchema = z.enum(["drafting", "awaiting_review", "executing", "terminal"]);
const teamTaskStateSchema = z.enum(["draft", "queued", "running", "succeeded", "failed", "canceled"]);

export const teamTaskSchema = z.object({
  id: z.string(),
  run_id: z.string(),
  team_id: z.string(),
  title: z.string(),
  description: z.string(),
  sort_order: z.number(),
  recommended_member_id: z.string(),
  owner_member_id: z.string().nullable(),
  state: teamTaskStateSchema,
  revision: z.number(),
  result: z.string().nullable(),
  error_code: z.string().nullable(),
  created_at: z.string(),
  updated_at: z.string(),
});

const teamRunSchema = z.object({
  id: z.string(),
  team_id: z.string(),
  state: teamRunStateSchema,
  revision: z.number(),
  leader_member_id: z.string(),
  roster_snapshot: z.array(z.object({
    member_id: z.string(),
    role: z.enum(["leader", "teammate"]),
    sort_order: z.number(),
    agent_id: z.string(),
    model: z.string().nullable(),
    execution_context_key: z.string(),
  })),
  created_at: z.string(),
  updated_at: z.string(),
  finished_at: z.string().nullable(),
  error_code: z.string().nullable(),
});

export const teamRunSnapshotSchema = z.object({
  run: teamRunSchema,
  tasks: z.array(teamTaskSchema),
  unread_mailbox_count: z.number(),
});

export const teamLeaderChatResultSchema = z.object({
  team_id: z.string(),
  member_id: z.string(),
  execution_id: z.string(),
  text: z.string(),
  replay: z.boolean(),
});

export const teamRestoreTaskResultSchema = z.object({
  run_id: z.string(),
  leader_error_code: z.string().nullable(),
  members: z.array(z.object({
    member_id: z.string(),
    role: z.enum(["leader", "teammate"]),
    state: z.enum(["ready", "unavailable"]),
    error_code: z.string().nullable(),
  })),
});

export const teamRestoreSnapshotSchema = z.object({
  run: teamRunSnapshotSchema,
  leader: z.object({
    team_id: z.string(),
    member_id: z.string(),
    execution_id: z.string(),
    text: z.string(),
    replay: z.boolean(),
  }).nullable(),
  leader_error_code: z.string().nullable(),
  members: z.array(z.object({
    member_id: z.string(),
    role: z.enum(["leader", "teammate"]),
    state: z.enum(["ready", "unavailable"]),
    error_code: z.string().nullable(),
  })),
});

export const teamRuntimeTaskSnapshotSchema = z.object({
  task_id: z.string(),
  kind: z.literal("TeamRun"),
  tenant_id: z.string().optional(),
  dedup_key: z.string().nullable(),
  state: z.enum(["Pending", "Running", "Cancelling", "Succeeded", "Failed", "Canceled"]),
  progress: z.object({
    current: z.number(),
    total: z.number().nullable(),
    note: z.string().nullable(),
  }).nullable(),
  error: z.object({
    code: z.string(),
    message: z.string(),
    retryable: z.boolean(),
    details: z.unknown().optional(),
  }).nullable(),
  started_at: z.string(),
  finished_at: z.string().nullable(),
  detail: z.unknown(),
  result: z.unknown().nullable(),
});

export const sessionEventIdentitySchema = z.object({
  session_id: z.string(),
  member_id: z.string(),
  execution_id: z.string(),
  turn_id: z.string(),
  item_id: z.string(),
  event_id: z.string(),
});

export const sessionItemIdentitySchema = z.object({
  session_id: z.string(),
  member_id: z.string(),
  execution_id: z.string(),
  turn_id: z.string(),
  item_id: z.string(),
});

export const sessionEventKindSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("user_message_acknowledged"), accepted: z.boolean() }),
  z.object({ type: z.literal("assistant_text_delta"), text: z.string() }),
  z.object({ type: z.literal("assistant_text_snapshot"), text: z.string() }),
  z.object({ type: z.literal("processing"), state: z.enum(["started", "active", "completed"]) }),
  z.object({ type: z.literal("thinking_delta"), text: z.string() }),
  z.object({ type: z.literal("thinking_snapshot"), text: z.string() }),
  z.object({ type: z.literal("tool_start"), name: z.string().nullable() }),
  z.object({ type: z.literal("tool_update"), state: z.enum(["running", "succeeded", "failed", "cancelled"]), detail: z.string().nullable() }),
  z.object({ type: z.literal("tool_result"), success: z.boolean(), detail: z.string().nullable() }),
  z.object({ type: z.literal("task_projection"), task_id: z.string() }),
  z.object({ type: z.literal("task_status"), status: z.enum(["queued", "running", "succeeded", "failed", "cancelled"]) }),
  z.object({ type: z.literal("task_result"), success: z.boolean(), detail: z.string().nullable() }),
  z.object({ type: z.literal("notice"), code: z.string(), detail: z.string().nullable() }),
  z.object({ type: z.literal("terminal_result"), text: z.string().nullable() }),
  z.object({ type: z.literal("cancel") }),
  z.object({ type: z.literal("error"), code: z.string(), retryable: z.boolean() }),
]);

export const sessionEventSchema = z.object({
  identity: sessionEventIdentitySchema,
  sequence: z.number(),
  delivery: z.enum(["live", "replay"]),
  kind: sessionEventKindSchema,
});

export const sessionItemSnapshotSchema = z.object({
  identity: sessionItemIdentitySchema,
  kind: z.enum([
    "user_message",
    "assistant_text",
    "processing",
    "thinking",
    "tool",
    "task",
    "notice",
    "final_result",
    "cancelled",
    "error",
  ]),
  sequence: z.number(),
  delivery: z.enum(["live", "replay"]),
  state: z.enum(["pending", "streaming", "completed", "succeeded", "failed", "cancelled"]),
  text: z.string().nullable(),
  status: z.enum(["queued", "running", "succeeded", "failed", "cancelled"]).nullable(),
  code: z.string().nullable(),
});

export const sessionSnapshotSchema = z.object({
  revision: z.number(),
  event_count: z.number(),
  items: z.array(sessionItemSnapshotSchema),
});

const teamMemberTaskCleanupSchema = z.object({
  process_reaped: z.boolean(),
  workspace_removed: z.boolean(),
  failure_count: z.number(),
  session_closed: z.boolean().nullable(),
  session_deleted: z.boolean().nullable(),
  session_delete_method: z.enum(["acp", "provider_fallback"]).nullable(),
});

export const teamMemberTaskDetailSchema = z.object({
  workflow: z.literal("team_member_turn"),
  tenant_id: z.string(),
  team_id: z.string(),
  member_id: z.string(),
  execution_id: z.string(),
  replay: z.boolean(),
  phase: z.string(),
  cleanup: teamMemberTaskCleanupSchema.optional(),
});

export const teamMemberTaskResultSchema = z.object({
  workflow: z.literal("team_member_turn"),
  team_id: z.string(),
  member_id: z.string(),
  execution_id: z.string(),
  replay: z.boolean(),
  terminal: z.literal(true),
});

export const teamMemberTaskSnapshotSchema = teamRuntimeTaskSnapshotSchema.extend({
  detail: teamMemberTaskDetailSchema,
  result: teamMemberTaskResultSchema.nullable(),
});

export const teamMemberStreamSnapshotSchema = z.object({
  team_id: z.string(),
  member_id: z.string(),
  execution_id: z.string(),
  sequence: z.number(),
  replay: z.boolean(),
  task: teamMemberTaskSnapshotSchema,
  stream: sessionSnapshotSchema,
});
