use crate::backend::application::{AppError, AppResult, AppService};
use crate::backend::domain::memory::storage::MemoryStorage;
use std::collections::{BTreeMap, HashSet};
use std::fs;

pub fn project_path_to_slug(project_path: &str) -> String {
    let trimmed = project_path.trim().trim_matches('/');
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

        if trimmed.starts_with("## 🍂")
            || trimmed.contains("待探索")
            || trimmed.contains("Unexplored")
        {
            current_section = 1;
            continue;
        } else if trimmed.starts_with("## ✅")
            || trimmed.contains("已完成")
            || trimmed.contains("Completed")
        {
            current_section = 2;
            continue;
        } else if trimmed.starts_with("## ") {
            current_section = 0;
            continue;
        }

        if trimmed.starts_with("- ") {
            let item = trimmed.to_string();
            if item.contains("无待探索分支") || item.contains("无已完成任务") {
                continue;
            }
            if current_section == 1 {
                parsed.unexplored_branches.push(item);
            } else if current_section == 2 {
                parsed.completed_tasks.push(item);
            }
        }
    }

    parsed
}

fn extract_semantic_tokens(text: &str) -> HashSet<String> {
    // 移除 markdown 链接格式，如 [short#T1](...)
    let mut clean = String::new();
    let mut in_url = false;
    for c in text.chars() {
        if c == '(' {
            in_url = true;
        } else if c == ')' {
            in_url = false;
        } else if !in_url && c != '[' && c != ']' && c != ':' && c != '-' {
            clean.push(c);
        }
    }

    clean
        .split_whitespace()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s.len() >= 3 && *s != "the" && *s != "and" && *s != "for" && *s != "turn")
        .collect()
}

pub fn ablate_and_merge_leaves(
    existing_leaves: &[String],
    new_leaves: &[String],
    completed_tasks: &[String],
) -> Vec<String> {
    let completed_token_sets: Vec<HashSet<String>> = completed_tasks
        .iter()
        .map(|t| extract_semantic_tokens(t))
        .collect();

    let is_ablated = |leaf: &str| -> bool {
        let tokens = extract_semantic_tokens(leaf);
        if tokens.is_empty() {
            return false;
        }
        for comp_tokens in &completed_token_sets {
            let intersection = tokens.intersection(comp_tokens).count();
            if intersection >= 2 || (tokens.len() == 1 && intersection == 1) {
                return true;
            }
        }
        false
    };

    let mut result = Vec::new();

    // 1. 保留未被消融的旧叶子
    for leaf in existing_leaves {
        if !is_ablated(leaf) && !result.contains(leaf) {
            result.push(leaf.clone());
        }
    }

    // 2. 加入未被消融的新叶子
    for leaf in new_leaves {
        if !is_ablated(leaf) && !result.contains(leaf) {
            result.push(leaf.clone());
        }
    }

    result
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
        let sessions_dir = storage.sessions_dir();
        if !sessions_dir.exists() {
            return Ok(RecentAndProjectWorkflowReport::default());
        }

        let mut project_leaves_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut project_completed_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut session_count = 0;

        let entries = fs::read_dir(&sessions_dir)
            .map_err(AppError::from)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"));

        for entry in entries {
            let content = fs::read_to_string(entry.path()).map_err(AppError::from)?;
            let parsed = parse_session_markdown(&content);
            session_count += 1;

            let proj = parsed
                .project_path
                .unwrap_or_else(|| "unspecified".to_string());

            project_leaves_map
                .entry(proj.clone())
                .or_default()
                .extend(parsed.unexplored_branches);
            project_completed_map
                .entry(proj)
                .or_default()
                .extend(parsed.completed_tasks);
        }

        // 1. 生成 Recent 产物：recent/latest.md 与 recent/<date>.md
        let mut recent_md = String::new();
        recent_md.push_str("# 近期研发进展与待探索分支 (Recent Work & Unexplored Branches)\n\n");

        for (proj, leaves) in &project_leaves_map {
            let completed = project_completed_map.get(proj);
            recent_md.push_str(&format!("## 项目: {}\n\n", proj));

            recent_md.push_str("### 🍂 待探索的悬空叶子节点 (Unexplored Branches)\n");
            if leaves.is_empty() {
                recent_md.push_str("- 无待探索分支\n");
            } else {
                for l in leaves {
                    recent_md.push_str(&format!("{}\n", l));
                }
            }
            recent_md.push('\n');

            recent_md.push_str("### ✅ 已完成的闭环任务 (Completed Tasks)\n");
            match completed {
                Some(tasks) if !tasks.is_empty() => {
                    for t in tasks {
                        recent_md.push_str(&format!("{}\n", t));
                    }
                }
                _ => {
                    recent_md.push_str("- 无已完成任务\n");
                }
            }
            recent_md.push('\n');
        }

        storage
            .write_markdown_atomic("recent/latest.md", &recent_md)
            .map_err(AppError::from)?;

        let today_str = chrono::Utc::now().format("%Y-%m-%d").to_string();
        storage
            .write_markdown_atomic(format!("recent/{}.md", today_str), &recent_md)
            .map_err(AppError::from)?;

        // 2. 更新 Project 产物与动态消融：projects/<slug>.md
        let mut project_count = 0;
        for (proj, new_leaves) in &project_leaves_map {
            let slug = project_path_to_slug(proj);
            let proj_rel_path = format!("projects/{}.md", slug);

            let completed = project_completed_map.get(proj).cloned().unwrap_or_default();

            // 读取现有池
            let mut existing_leaves = Vec::new();
            if let Ok(Some(existing_content)) = storage.read_markdown(&proj_rel_path) {
                let parsed = parse_session_markdown(&existing_content);
                existing_leaves = parsed.unexplored_branches;
            }

            let remaining_leaves =
                ablate_and_merge_leaves(&existing_leaves, new_leaves, &completed);

            let mut proj_md = String::new();
            proj_md.push_str("---\n");
            proj_md.push_str(&format!("project_path: {}\n", proj));
            proj_md.push_str(&format!(
                "updated_at: {}\n",
                chrono::Utc::now().to_rfc3339()
            ));
            proj_md.push_str("---\n\n");
            proj_md.push_str(&format!("# 项目记忆: {}\n\n", proj));

            proj_md.push_str("## 🍂 待探索的悬空叶子节点 (Unexplored Branches)\n");
            if remaining_leaves.is_empty() {
                proj_md.push_str("- 暂无待探索分支\n");
            } else {
                for l in &remaining_leaves {
                    proj_md.push_str(&format!("{}\n", l));
                }
            }
            proj_md.push('\n');

            storage
                .write_markdown_atomic(&proj_rel_path, &proj_md)
                .map_err(AppError::from)?;

            project_count += 1;
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
