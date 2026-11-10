use chrono::{DateTime, Utc};

use crate::backend::application::memory::recent::{
    RecentConversationSessionListParams, RecentConversationView,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::conversations::ConversationSessionListItem;
use crate::backend::store;
use std::collections::HashMap;

/// 单页拉取条数：未处理过滤必须在分页之前完成，因此内部按页扫描。
const UNPROCESSED_SCAN_PAGE_SIZE: usize = 200;
/// 单次扫描的会话上限，防止极端库规模下无限翻页。
const UNPROCESSED_SCAN_CAP: usize = 20_000;

/// 未处理会话发现参数。
#[derive(Debug, Clone, Default)]
pub struct UnprocessedSessionScanParams {
    /// 基础过滤条件（适配器 / 来源 / 查询 / 项目 / 时间窗口）。
    pub list: ConversationSessionListParams,
    /// 过滤完成后返回的条数。
    pub limit: usize,
    /// 过滤完成后再跳过的条数（用于翻页）。
    pub offset: usize,
}

impl AppService {
    /// 检查当前租户是否存在待处理的 Memory 任务或事实
    pub async fn has_pending_memory_work(&self, now: DateTime<Utc>) -> AppResult<bool> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let watermarks = store::load_watermarks_sqlx(pool, tenant_id).await?;
        let sessions = self
            .list_recent_conversation_sessions_at(
                RecentConversationSessionListParams {
                    view: RecentConversationView::Time,
                    window_hours: None,
                    limit: Some(1000),
                    offset: Some(0),
                },
                now,
            )
            .await?;

        for item in sessions {
            let session = item.session;
            let updated_at = session.updated_at.as_deref().unwrap_or_default();
            match watermarks.get(&session.id) {
                Some(wm) => {
                    if wm.source_updated_at != updated_at {
                        return Ok(true);
                    }
                }
                None => {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// 列出仍有未处理轮次（`watermark < turn_count`）的会话。
    ///
    /// 未处理判定发生在分页**之前**：先按基础条件逐页扫描，再用项目记忆水位过滤，
    /// 最后才按 `offset`/`limit` 截断。否则小 `limit`（如默认 15 条）会在水位比对
    /// 之前就截断结果，让最需要提炼的历史积压会话永远浮不上来。
    pub async fn list_unprocessed_conversation_sessions(
        &self,
        params: UnprocessedSessionScanParams,
    ) -> AppResult<Vec<ConversationSessionListItem>> {
        let watermarks = self.memory_session_watermark_index().await?;
        let mut collected = Vec::new();
        let mut scanned = 0usize;
        let mut page_offset = 0usize;

        while scanned < UNPROCESSED_SCAN_CAP && collected.len() < params.offset + params.limit {
            let mut page_params = params.list.clone();
            page_params.limit = Some(UNPROCESSED_SCAN_PAGE_SIZE);
            page_params.offset = Some(page_offset);
            let page = self.list_conversation_sessions(page_params).await?;
            let page_len = page.len();
            scanned += page_len;
            collected.extend(
                page.into_iter()
                    .filter(|item| is_unprocessed(item, &watermarks)),
            );
            if page_len < UNPROCESSED_SCAN_PAGE_SIZE {
                break;
            }
            page_offset += page_len;
        }

        Ok(collected
            .into_iter()
            .skip(params.offset)
            .take(params.limit)
            .collect())
    }
}

fn is_unprocessed(item: &ConversationSessionListItem, watermarks: &HashMap<String, u32>) -> bool {
    if item.turn_count == 0 {
        return false;
    }
    let short_id = crate::backend::domain::conversation_id_fragment(&item.session.id);
    watermarks.get(&short_id).copied().unwrap_or(0) < item.turn_count as u32
}
