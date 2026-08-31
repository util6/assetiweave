use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
    Leader,
    Teammate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMember {
    pub id: String,
    pub team_id: String,
    pub role: TeamRole,
    pub sort_order: i32,
    pub agent_id: String,
    pub model: Option<String>,
    pub execution_context_key: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamDetail {
    #[serde(flatten)]
    pub team: Team,
    pub members: Vec<TeamMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMemberInput {
    pub id: Option<String>,
    pub role: TeamRole,
    pub sort_order: Option<i32>,
    pub agent_id: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateTeamInput {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub members: Vec<TeamMemberInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateTeamInput {
    pub team_id: String,
    pub name: String,
    pub description: Option<String>,
    pub members: Vec<TeamMemberInput>,
}
