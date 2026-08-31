export type TeamRole = "leader" | "teammate";

export interface Team {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
}

export interface TeamMember {
  id: string;
  team_id: string;
  role: TeamRole;
  sort_order: number;
  agent_id: string;
  model: string | null;
  execution_context_key: string;
  created_at: string;
  updated_at: string;
}

export interface TeamDetail {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
  members: TeamMember[];
}

export interface TeamMemberInput {
  id?: string;
  role: TeamRole;
  sort_order?: number;
  agent_id: string;
  model?: string | null;
}

export interface CreateTeamInput {
  id?: string;
  name: string;
  description?: string | null;
  members: TeamMemberInput[];
}

export interface UpdateTeamInput {
  team_id: string;
  name: string;
  description?: string | null;
  members: TeamMemberInput[];
}
