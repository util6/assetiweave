use super::memory_workflow_global::GlobalMemoryWorkflowReport;
use super::memory_workflow_recent_project::RecentAndProjectWorkflowReport;
use super::session_branch_workflow::SessionBranchWorkflowReport;
use crate::backend::application::AppResult;
use crate::backend::application::AppService;
use crate::backend::infrastructure::agent_execution::AgentExecutionRuntime;
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
        runtime_override: Option<&dyn AgentExecutionRuntime>,
        storage_override: Option<&MemoryStorage>,
    ) -> AppResult<MemoryMarkdownPipelineReport> {
        let owned_storage;
        let storage = match storage_override {
            Some(s) => s,
            None => {
                owned_storage = self.memory_storage();
                &owned_storage
            }
        };

        // 节点 1：单会话认知提炼与多叉树演进
        let session_report = self
            .run_session_branch_workflow(runtime_override, Some(storage))
            .await?;

        // 节点 2：Recent 汇总与 Project 状态同步
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
