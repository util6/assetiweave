import { z } from "zod";

export const teamRoleSchema = z.enum(["leader", "teammate"]);

export const teamMemberSchema = z.object({
  id: z.string(),
  team_id: z.string(),
  role: teamRoleSchema,
  sort_order: z.number(),
  agent_id: z.string(),
  model: z.string().nullable().default(null),
  execution_context_key: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
});

export const teamSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullable().default(null),
  created_at: z.string(),
  updated_at: z.string(),
});

export const teamDetailSchema = teamSchema.extend({
  members: z.array(teamMemberSchema),
});

export const teamListSchema = z.array(teamDetailSchema);
