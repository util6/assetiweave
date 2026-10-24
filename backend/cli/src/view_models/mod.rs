use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionViewModel {
    pub cli_version: String,
    pub engine_version: String,
    pub compatible: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewViewModel {
    pub source_count: usize,
    pub asset_count: usize,
    pub profile_count: usize,
    pub last_scan_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSessionViewModel {
    pub short_id: String,
    pub session_id: String,
    pub updated_at: Option<String>,
    pub updated_at_human: String,
    pub latest_user_prompt: Option<String>,
    pub turn_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_turn_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_start_turn_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_start_user_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_end_turn_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_end_user_prompt: Option<String>,
    pub project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub adapter_id: String,
}

impl ConversationSessionViewModel {
    pub fn from_item(
        item: &asset_core::domain::conversations::ConversationSessionListItem,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let short_id = asset_core::domain::conversation_id_fragment(&item.session.id);
        let updated_at_human = format_relative_time(
            item.session
                .updated_at
                .as_deref()
                .or(Some(&item.session.created_at)),
            now,
        );
        let effective_path = item
            .session
            .project_path
            .as_deref()
            .or(item.session.last_cwd.as_deref());
        let project_name = effective_path
            .map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
                    .to_string()
            })
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "(global)".to_string());

        Self {
            short_id,
            session_id: item.session.id.clone(),
            updated_at: item.session.updated_at.clone(),
            updated_at_human,
            latest_user_prompt: item.latest_user_prompt.clone(),
            turn_count: item.turn_count,
            window_turn_count: item.window_turn_count,
            window_start_turn_index: item.window_start_turn_index,
            window_start_user_prompt: item.window_start_user_prompt.clone(),
            window_end_turn_index: item.window_end_turn_index,
            window_end_user_prompt: item.window_end_user_prompt.clone(),
            project_name,
            project_path: item
                .session
                .project_path
                .clone()
                .or_else(|| item.session.last_cwd.clone()),
            adapter_id: item.session.adapter_id.clone(),
        }
    }
}

pub fn format_relative_time(time_str: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(ts) = time_str.map(str::trim).filter(|s| !s.is_empty()) else {
        return "—".to_string();
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").map(|naive| {
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc)
            })
        });
    let Ok(dt) = parsed else {
        return ts.to_string();
    };
    let diff = now.signed_duration_since(dt);
    if diff.num_seconds() < 0 {
        return "just now".to_string();
    }
    if diff.num_seconds() < 60 {
        return format!("{}s ago", diff.num_seconds().max(1));
    }
    if diff.num_minutes() < 60 {
        return format!("{}m ago", diff.num_minutes());
    }
    if diff.num_hours() < 24 {
        return format!("{}h ago", diff.num_hours());
    }
    if diff.num_days() < 30 {
        return format!("{}d ago", diff.num_days());
    }
    dt.format("%Y-%m-%d").to_string()
}
