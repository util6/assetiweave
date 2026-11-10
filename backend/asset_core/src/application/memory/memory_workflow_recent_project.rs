use crate::backend::application::{AppError, AppResult, AppService};
use crate::backend::infrastructure::memory::projection::{
    sync_project_markdown, sync_recent_markdown,
};
use crate::backend::infrastructure::memory::storage::MemoryStorage;
use std::fs;

/// 项目目录 → 规范项目 Slug。
///
/// 第一步恒先折叠便携路径锚点（宿主机主目录 → `~/`），保证
/// `/Users/<user>/code-space/foo` 与 `~/code-space/foo` 永远得到同一个 slug，
/// 不会因调用方传入哪种形态而分裂成两份项目状态。
pub fn project_path_to_slug(project_path: &str) -> String {
    let normalized =
        crate::backend::infrastructure::path_utils::normalize_path_for_storage(project_path)
            .unwrap_or_else(|_| project_path.to_string());
    slugify_path(&normalized)
}

/// 便携路径锚点折叠（`~/`）引入前的历史 slug 前缀，例如 `users-util6`。
///
/// 旧版本按宿主机绝对路径直接拼 slug，同一项目会落成
/// `<home-slug>-<canonical-slug>`（如 `users-util6-code-space-assetiweave`）。
/// 返回 `None` 表示当前主机主目录无法确定性解析，此时不做任何历史状态收敛，
/// 避免用后缀模糊匹配误伤其它项目的状态文件。
pub fn legacy_project_slug_prefix() -> Option<String> {
    let home = dirs::home_dir()?;
    let slug = slugify_path(&home.to_string_lossy());
    if slug.is_empty() || slug == "unspecified" {
        return None;
    }
    Some(slug)
}

/// 把已折叠的路径文本压成 slug（不做锚点折叠，纯字符串归一）。
fn slugify_path(path: &str) -> String {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return "unspecified".to_string();
    }
    let slug: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    let mut clean_slug = String::new();
    let mut last_dash = false;
    for c in slug.chars() {
        if c == '-' {
            if !last_dash {
                clean_slug.push(c);
                last_dash = true;
            }
        } else {
            clean_slug.push(c);
            last_dash = false;
        }
    }
    let res = clean_slug.trim_matches('-').to_string();
    if res.is_empty() {
        "unspecified".to_string()
    } else {
        res
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParsedSessionMarkdown {
    pub session_id: String,
    pub project_path: Option<String>,
    pub updated_at: String,
    pub unexplored_branches: Vec<String>,
    pub completed_tasks: Vec<String>,
}

/// 严格判断一行是否为合法的单行 Markdown 锚点列表项。
///
/// 防御与过滤规则：
/// 1. 忽略缩进的二级子列表或普通缩进解释行（避免子列表 "- 说明:" 等污染）；
/// 2. 行首严格以 "- [" 开头；
/// 3. 必须包含完整的 Markdown 锚点结构: - [anchor](target)；
/// 4. anchor、target 与 target 之后的描述内容必须非空；
/// 5. 过滤占位符（如 "无待探索"、"无已完成"、"暂无待探索" 等）。
fn is_valid_anchor_item(line: &str) -> bool {
    // 忽略缩进的二级子列表或普通缩进解释行
    if line.starts_with("  ") || line.starts_with('\t') {
        return false;
    }

    let trimmed = line.trim();
    if !trimmed.starts_with("- [") {
        return false;
    }

    // 检查是否包含闭合的链接结构: - [anchor](target)
    let Some(bracket_end) = trimmed.find("](") else {
        return false;
    };
    let anchor = &trimmed[3..bracket_end];
    if anchor.trim().is_empty() {
        return false;
    }

    let after_bracket = &trimmed[bracket_end + 2..];
    let Some(paren_close) = after_bracket.find(')') else {
        return false;
    };
    let target = &after_bracket[..paren_close];
    if target.trim().is_empty() {
        return false;
    }

    // 链接之后必须有技术描述内容
    let after_link = after_bracket[paren_close + 1..].trim();
    let description = after_link.strip_prefix(':').unwrap_or(after_link).trim();
    if description.is_empty() {
        return false;
    }

    // 过滤无信号占位符
    if description.contains("无待探索")
        || description.contains("无已完成")
        || description.contains("暂无待探索")
        || description.contains("暂无已完成")
    {
        return false;
    }

    true
}

pub fn parse_session_markdown(content: &str) -> ParsedSessionMarkdown {
    let mut parsed = ParsedSessionMarkdown::default();
    let mut in_frontmatter = false;
    let mut current_section = 0; // 1: unexplored, 2: completed

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }

        if in_frontmatter {
            if let Some(val) = trimmed.strip_prefix("session_id:") {
                parsed.session_id = val.trim().to_string();
            } else if let Some(val) = trimmed.strip_prefix("project_path:") {
                let p = val.trim();
                if !p.is_empty() {
                    parsed.project_path = Some(p.to_string());
                }
            } else if let Some(val) = trimmed.strip_prefix("updated_at:") {
                parsed.updated_at = val.trim().to_string();
            }
            continue;
        }

        if trimmed.contains("待探索") || trimmed.contains("Unexplored") {
            if trimmed.starts_with('#') {
                current_section = 1;
                continue;
            }
        } else if trimmed.contains("已完成") || trimmed.contains("Completed") {
            if trimmed.starts_with('#') {
                current_section = 2;
                continue;
            }
        } else if trimmed.starts_with('#') {
            current_section = 0;
            continue;
        }

        if is_valid_anchor_item(line) {
            let item = line.trim().to_string();
            if current_section == 1 {
                parsed.unexplored_branches.push(item);
            } else if current_section == 2 {
                parsed.completed_tasks.push(item);
            }
        }
    }

    parsed
}

#[derive(Debug, Clone, Default)]
pub struct RecentAndProjectWorkflowReport {
    pub session_count: usize,
    pub project_count: usize,
}

impl AppService {
    pub async fn run_recent_and_project_workflow(
        &self,
        storage: &MemoryStorage,
    ) -> AppResult<RecentAndProjectWorkflowReport> {
        self.run_recent_and_project_workflow_at(storage, chrono::Utc::now())
            .await
    }

    pub async fn run_recent_and_project_workflow_at(
        &self,
        storage: &MemoryStorage,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<RecentAndProjectWorkflowReport> {
        // 1. 同步 Recent 记忆状态与 Markdown 投影
        let mut session_count = 0;
        let recent_state = storage.read_recent_state().map_err(AppError::from)?;
        if let Some(recent) = recent_state {
            sync_recent_markdown(storage, &recent).map_err(AppError::from)?;
            let today_str = now.format("%Y-%m-%d").to_string();
            let recent_md =
                crate::backend::infrastructure::memory::projection::recent_state_to_markdown(
                    &recent,
                );
            storage
                .write_markdown_atomic(format!("recent/{}.md", today_str), &recent_md)
                .map_err(AppError::from)?;
            session_count = recent
                .projects
                .values()
                .map(|p| p.session_watermarks.len())
                .sum();
        }

        // 2. 遍历 .state/projects/<slug>.json 同步各项目的 Project 记忆投影
        let projects_dir = storage.state_projects_dir();
        let mut project_count = 0;
        if projects_dir.exists() {
            let entries = fs::read_dir(&projects_dir)
                .map_err(AppError::from)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"));

            for entry in entries {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if let Some(slug) = file_name.strip_suffix(".json") {
                    // 统一走「读取即收敛」入口，历史未折叠 slug 会在此被并入规范状态。
                    if let Ok(Some(project_state)) =
                        super::state_loader::load_project_state(storage, slug)
                    {
                        sync_project_markdown(storage, &project_state).map_err(AppError::from)?;
                        project_count += 1;
                    }
                }
            }
        }

        Ok(RecentAndProjectWorkflowReport {
            session_count,
            project_count,
        })
    }
}

#[cfg(test)]
#[path = "memory_workflow_recent_project_tests.rs"]
mod tests;
