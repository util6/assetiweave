use crate::backend::application::prelude::*;

impl AppService {
    /// 触发任务历史日志自动修剪流水线 (ADR-0017 / Issue #79)
    ///
    /// 作为内置的示范流水线任务运行：
    /// 阶段划分：
    /// 1. `stages::PREPARE`：计算截止日期（默认保留 30 天，可自定义天数）；
    /// 2. `stages::PRUNE`：物理清理 `task_journal` 中过期的记录；
    /// 3. `stages::VACUUM`：收尾与指标汇总。
    pub fn prune_task_journal_pipeline(
        &self,
        retention_days: Option<i64>,
    ) -> AppResult<crate::backend::infrastructure::tasks::TaskHandle<usize>> {
        let retention = retention_days.unwrap_or(30).max(1);
        let pipeline = crate::backend::infrastructure::tasks::PipelineDescriptor::builder()
            .stage(
                crate::backend::infrastructure::tasks::stages::PREPARE,
                "准备修剪窗口",
            )
            .stage(
                crate::backend::infrastructure::tasks::stages::PRUNE,
                "执行历史日志清理",
            )
            .stage(
                crate::backend::infrastructure::tasks::stages::VACUUM,
                "收尾与指标汇总",
            )
            .build();

        let spec = crate::backend::infrastructure::tasks::TaskSpec::new(
            crate::backend::infrastructure::tasks::TaskKind::ConversationDataMaintenance,
            Some(format!("prune_task_journal:{}", self.tenant_id())),
        )
        .with_pipeline(pipeline)
        .with_title("修剪历史任务日志")
        .with_user_visible(false);

        let service = self.clone();
        self.spawn_pipeline_task(spec, move |cx| async move {
            let cutoff = {
                let stage = cx.enter_stage(crate::backend::infrastructure::tasks::stages::PREPARE);
                cx.check_cancellation()
                    .map_err(crate::backend::infrastructure::InfraError::external)?;
                let cutoff_time = chrono::Utc::now() - chrono::Duration::days(retention);
                let cutoff_rfc = cutoff_time.to_rfc3339();
                stage.progress(1, Some(1), Some("窗口计算完成".to_string()));
                cutoff_rfc
            };

            let pruned_count = {
                let mut stage =
                    cx.enter_stage(crate::backend::infrastructure::tasks::stages::PRUNE);
                cx.check_cancellation()
                    .map_err(crate::backend::infrastructure::InfraError::external)?;
                let count = crate::backend::store::prune_task_journal_sqlx(service.pool(), &cutoff)
                    .await
                    .map_err(crate::backend::infrastructure::InfraError::external)?;
                stage.record_metric("rows_pruned", count as u64);
                stage.progress(
                    count as u64,
                    Some(count as u64),
                    Some("历史记录清理完成".to_string()),
                );
                count
            };

            {
                let stage = cx.enter_stage(crate::backend::infrastructure::tasks::stages::VACUUM);
                stage.progress(1, Some(1), Some("清理完毕".to_string()));
            }

            let summary = format!(
                "修剪完成，共清理 {} 条超过 {} 天的历史任务日志",
                pruned_count, retention
            );
            Ok(
                crate::backend::infrastructure::tasks::TaskOutput::with_summary(
                    pruned_count,
                    summary,
                ),
            )
        })
    }
}

#[cfg(test)]
#[path = "task_maintenance_tests.rs"]
mod tests;
