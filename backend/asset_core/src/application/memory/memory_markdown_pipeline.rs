use super::memory_workflow_global::GlobalMemoryWorkflowReport;
use super::memory_workflow_recent_project::RecentAndProjectWorkflowReport;
use super::session_branch_workflow::{SessionBranchExtractorAgent, SessionBranchWorkflowReport};
use crate::backend::application::AppResult;
use crate::backend::application::AppService;
use crate::backend::infrastructure::memory::storage::MemoryStorage;

#[derive(Debug, Clone, Default)]
pub struct MemoryMarkdownPipelineReport {
    pub session_report: SessionBranchWorkflowReport,
    pub recent_project_report: RecentAndProjectWorkflowReport,
    pub global_report: GlobalMemoryWorkflowReport,
}

impl AppService {
    pub async fn run_memory_full_pipeline(
        &self,
        extractor: Option<&dyn SessionBranchExtractorAgent>,
        storage_override: Option<&MemoryStorage>,
    ) -> AppResult<MemoryMarkdownPipelineReport> {
        let tenant_id = self.tenant_id();
        let owned_storage;
        let storage = match storage_override {
            Some(s) => s,
            None => {
                owned_storage = MemoryStorage::new(tenant_id);
                &owned_storage
            }
        };

        let ext = match extractor {
            Some(e) => e,
            None => {
                // 无提取器注入时，不覆写磁盘产物，防止机械复读垃圾污染
                let session_report = self
                    .run_session_branch_workflow(None, Some(storage))
                    .await?;
                return Ok(MemoryMarkdownPipelineReport {
                    session_report,
                    recent_project_report: RecentAndProjectWorkflowReport::default(),
                    global_report: GlobalMemoryWorkflowReport::default(),
                });
            }
        };

        // 节点 1：单会话思维分叉提取
        let session_report = self
            .run_session_branch_workflow(Some(ext), Some(storage))
            .await?;

        // 节点 2：Recent 汇总与 Project 悬空叶子池动态消融
        let recent_project_report = self.run_recent_and_project_workflow(storage).await?;

        // 节点 3：Global 长期记忆沉淀
        let global_report = self.run_global_memory_workflow(storage).await?;

        Ok(MemoryMarkdownPipelineReport {
            session_report,
            recent_project_report,
            global_report,
        })
    }
}

#[cfg(test)]
#[path = "memory_markdown_pipeline_tests.rs"]
mod tests;
