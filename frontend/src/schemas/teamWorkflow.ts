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
