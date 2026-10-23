use serde_json::{json, Value};

use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::conversations::ConversationPartIndexStatus;
use crate::backend::infrastructure::tasks::{
    stages, ConflictPolicy, InfraError, PipelineDescriptor, TaskAdmission, TaskCapabilities,
    TaskHandle, TaskKind, TaskOutput, TaskSpec,
};
use crate::backend::store;

impl AppService {
    /// 零件级全文索引重建流水线 (Issue #111 / ADR-0017)。
    ///
    /// 索引重建可能耗时，因此整体纳管为后台流水线任务：
    /// 1. `stages::PREPARE`：把索引标记为 `building`，列表搜索立即降级为标题级；
    /// 2. `stages::PERSIST_INDEX`：按会话分事务重建零件级索引行；
    /// 3. `stages::VERIFY`：复核索引规模并把状态恢复为 `ready`。
    pub fn rebuild_conversation_part_index(&self) -> AppResult<TaskHandle<Value>> {
        let pipeline = PipelineDescriptor::builder()
            .stage(stages::PREPARE, "标记索引重建中")
            .stage(stages::PERSIST_INDEX, "逐会话重建零件索引")
            .stage(stages::VERIFY, "校验索引并恢复正文匹配")
            .build();

        let task_id = format!("conversation-part-index:{}", self.tenant_id());
        let _ = self.runtime.task_runtime().remove_terminal(&task_id);

        let mut spec = TaskSpec::new(
            TaskKind::SearchIndexRebuild,
            Some(format!("conversation-part-index-task:{}", self.tenant_id())),
        )
        .with_task_id(task_id.clone())
        .with_tenant_id(self.tenant_id().to_string())
        .with_title("重建会话搜索索引")
        .with_admission(TaskAdmission::Managed)
        .with_user_visible(true)
        .with_capabilities(TaskCapabilities {
            cancellable: true,
            retryable: true,
            clearable: true,
        })
        .with_conflict_key(format!("conversation-part-index:{}", self.tenant_id()))
        .with_conflict_policy(ConflictPolicy::Reject)
        .with_pipeline(pipeline);
        spec.detail = json!({
            "domain": "conversation_part_index",
            "tenant_id": self.tenant_id(),
        });

        let service = self.clone();
        self.spawn_pipeline_task(spec, move |cx| async move {
            let tenant_id = service.tenant_id().to_string();
            let pool = service.pool().clone();

            {
                let stage = cx.enter_stage(stages::PREPARE);
                cx.check_cancellation().map_err(InfraError::external)?;
                store::set_conversation_part_index_status_sqlx(
                    &pool,
                    &tenant_id,
                    store::PART_INDEX_BUILDING,
                    0,
                    None,
                )
                .await?;
                stage.progress(1, Some(1), Some("正文匹配已暂时降级为标题级".to_string()));
            }

            let outcome: Result<i64, InfraError> = async {
                {
                    let stage = cx.enter_stage(stages::PERSIST_INDEX);
                    cx.check_cancellation().map_err(InfraError::external)?;
                    store::rebuild_tenant_part_index_sqlx(&pool, &tenant_id, |done, total| {
                        stage.progress(
                            done as u64,
                            Some(total as u64),
                            Some(format!("已重建 {done}/{total} 个会话")),
                        );
                    })
                    .await?;
                }

                let mut stage = cx.enter_stage(stages::VERIFY);
                cx.check_cancellation().map_err(InfraError::external)?;
                let part_count = store::count_part_index_rows_sqlx(&pool, &tenant_id).await?;
                stage.record_metric("verified_parts", part_count as u64);
                stage.progress(1, Some(1), Some("索引校验完成".to_string()));
                Ok(part_count)
            }
            .await;

            match outcome {
                Ok(part_count) => {
                    store::set_conversation_part_index_status_sqlx(
                        &pool,
                        &tenant_id,
                        store::PART_INDEX_READY,
                        part_count,
                        None,
                    )
                    .await?;
                    Ok(TaskOutput::with_summary(
                        json!({ "part_count": part_count }),
                        format!("会话搜索索引重建完成，共 {part_count} 条正文可搜"),
                    ))
                }
                Err(error) => {
                    let _ = store::set_conversation_part_index_status_sqlx(
                        &pool,
                        &tenant_id,
                        store::PART_INDEX_STALE,
                        0,
                        Some(&error.to_string()),
                    )
                    .await;
                    Err(error)
                }
            }
        })
    }

    /// 启动零件级索引重建并立即返回任务快照（流水线在后台继续推进）。
    pub async fn start_conversation_part_index_rebuild(
        &self,
    ) -> AppResult<Option<crate::backend::infrastructure::tasks::task_models::TaskSnapshot>> {
        let handle = self.rebuild_conversation_part_index()?;
        Ok(self.runtime.task_runtime().get(&handle.task_id))
    }

    /// 读取零件级索引状态，供界面提示「重建中，仅按标题匹配」。
    pub async fn get_conversation_part_index_status(
        &self,
    ) -> AppResult<ConversationPartIndexStatus> {
        let state = store::load_conversation_part_index_state_sqlx(self.pool(), self.tenant_id())
            .await?
            .unwrap_or_else(|| store::ConversationPartIndexState {
                tenant_id: self.tenant_id().to_string(),
                status: store::PART_INDEX_READY.to_string(),
                part_count: 0,
                last_error: None,
                rebuilt_at: None,
                updated_at: None,
            });
        Ok(ConversationPartIndexStatus {
            is_ready: state.is_ready(),
            status: state.status,
            part_count: state.part_count,
            last_error: state.last_error,
            rebuilt_at: state.rebuilt_at,
            updated_at: state.updated_at,
        })
    }
}

#[cfg(test)]
#[path = "conversation_part_index_tests.rs"]
mod tests;
