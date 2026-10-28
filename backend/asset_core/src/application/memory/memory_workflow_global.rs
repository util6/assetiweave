use crate::backend::application::{AppError, AppResult, AppService};
use crate::backend::infrastructure::memory::storage::MemoryStorage;

pub fn generate_global_memory_markdown(storage: &MemoryStorage) -> AppResult<String> {
    let recent_content = storage
        .read_markdown("recent/latest.md")
        .map_err(AppError::from)?
        .unwrap_or_default();

    let mut global_md = String::new();
    global_md.push_str("# 全局长期记忆与工程准则 (Global Memory)\n\n");
    global_md.push_str("## 📐 通用工程规范与架构准则\n");
    global_md.push_str(
        "- 本地优先与 Markdown 优先：所有业务记忆均落盘至本地文件，SQLite 仅作为轻量扫描水位。\n",
    );
    global_md.push_str("- 单层软链接与零跨进程开销原则。\n\n");

    global_md.push_str("## 💡 近期研发沉淀事实与决策焦点\n");
    if recent_content.trim().is_empty() {
        global_md.push_str("- 暂无近期沉淀事实\n");
    } else {
        // 从 recent_content 挑选前 10 条已完成任务或要点
        let mut count = 0;
        for line in recent_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("- ")
                && !trimmed.contains("无已完成任务")
                && !trimmed.contains("无待探索")
            {
                global_md.push_str(&format!("{}\n", trimmed));
                count += 1;
                if count >= 10 {
                    break;
                }
            }
        }
        if count == 0 {
            global_md.push_str("- 暂无活跃沉淀要点\n");
        }
    }

    Ok(global_md)
}

#[derive(Debug, Clone, Default)]
pub struct GlobalMemoryWorkflowReport {
    pub updated: bool,
}

impl AppService {
    pub async fn run_global_memory_workflow(
        &self,
        storage: &MemoryStorage,
    ) -> AppResult<GlobalMemoryWorkflowReport> {
        let new_content = generate_global_memory_markdown(storage)?;

        if let Ok(Some(existing)) = storage.read_markdown("MEMORY.md") {
            if existing == new_content {
                return Ok(GlobalMemoryWorkflowReport { updated: false });
            }
        }

        storage
            .write_markdown_atomic("MEMORY.md", &new_content)
            .map_err(AppError::from)?;

        Ok(GlobalMemoryWorkflowReport { updated: true })
    }
}

#[cfg(test)]
#[path = "memory_workflow_global_tests.rs"]
mod tests;
