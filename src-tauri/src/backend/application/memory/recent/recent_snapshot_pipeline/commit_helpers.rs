use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::store;
use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};

impl AppService {
    pub(crate) async fn compute_post_commit_content_fingerprint(
        &self,
        tx: &mut sqlx::SqliteConnection,
        tenant_id: &str,
        target: &WatermarkTarget,
        candidates: &[CandidateSession],
        skill_binding: &MemorySkillBinding,
        memory_settings: &crate::backend::infrastructure::app_settings::MemorySettings,
    ) -> AppResult<(String, Vec<String>)> {
        let active_rows = sqlx::query(
            "SELECT mi.id as item_id, mir.id as rev_id, mi.first_seen_at \
             FROM memory_items mi \
             JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = ?1 \
               AND mi.layer = 'l1' \
               AND mi.lifecycle = 'current' \
               AND mir.status IN ('active', 'blocked', 'waiting')",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::external)?;

        let mut carry_over: Vec<(String, String, i64)> = Vec::new();
        for r in active_rows {
            let item_id: String = r.get("item_id");
            let rev_id: String = r.get("rev_id");
            let fs: String = r.get("first_seen_at");
            let days_elapsed = match DateTime::parse_from_rfc3339(&fs) {
                Ok(dt) => {
                    (target.target_watermark_utc - dt.with_timezone(&Utc))
                        .num_seconds()
                        .max(0)
                        / 86400
                }
                Err(_) => 0,
            };
            let rem_bucket = (7 - days_elapsed).max(0);
            carry_over.push((item_id, rev_id, rem_bucket));
        }

        let carry_over_refs: Vec<(&str, &str, i64)> = carry_over
            .iter()
            .map(|(i, r, b)| (i.as_str(), r.as_str(), *b))
            .collect();

        let current_long_term_revisions: Vec<String> = sqlx::query_scalar(
            "SELECT mir.id FROM memory_items mi
             JOIN memory_item_revisions mir
               ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id
             WHERE mi.tenant_id = ?1 AND mi.layer IN ('l2', 'l3')
               AND mi.lifecycle = 'current' ORDER BY mi.id ASC",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::external)?;
        let current_long_term_revision_refs = current_long_term_revisions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        let effective_content_fp = compute_content_fingerprint(
            target.window_hours,
            candidates,
            &carry_over_refs,
            &current_long_term_revision_refs,
            skill_binding,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );

        Ok((effective_content_fp, current_long_term_revisions))
    }
}
