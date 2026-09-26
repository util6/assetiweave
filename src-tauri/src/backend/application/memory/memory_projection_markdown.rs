use std::collections::BTreeMap;

use crate::backend::domain::memory::{
    RecentMemoryItemView, RecentMemorySnapshotView, RecentProjectView,
    RecentSnapshotPublicationKind,
};
use crate::backend::domain::{L2ProjectMemoryView, L3MemoryItemView, MemoryItemCategory};

/// 渲染 memory_summary.md (M35-PROJ-02, 07-markdown-ui-settings.md Section 3)
pub(crate) fn render_memory_summary_markdown(snapshot: &RecentMemorySnapshotView) -> String {
    let mut out = String::new();

    out.push_str("# Recent Memory\n\n");
    out.push_str(&format!("- Window: {}h\n", snapshot.window_hours));
    out.push_str(&format!("- From: {}\n", snapshot.window_start));
    out.push_str(&format!("- To: {}\n", snapshot.window_end));
    out.push_str(&format!("- Published: {}\n", snapshot.published_at));
    out.push_str(&format!(
        "- Content generated: {}\n",
        snapshot.content_generated_at
    ));
    let mode_str = match snapshot.publication_kind {
        RecentSnapshotPublicationKind::Generated => "generated",
        RecentSnapshotPublicationKind::Reused => "reused",
    };
    out.push_str(&format!("- Mode: {}\n\n", mode_str));

    // 按日期将 items 与 project summary 聚合
    // 确定性规则:
    // 1. 日期降序
    // 2. 同日项目按 project_title, project_key 排序
    // 3. 项目摘要和建议只出现一次，在 latest_activity_at 对应日期
    // 4. 普通 Item 在 occurred_at 对应日期
    let mut items_by_date_and_project: BTreeMap<
        String,
        BTreeMap<(String, String), Vec<&RecentMemoryItemView>>,
    > = BTreeMap::new();

    let mut project_meta_by_date: BTreeMap<String, Vec<&RecentProjectView>> = BTreeMap::new();

    for proj in &snapshot.projects {
        let proj_date = proj
            .latest_activity_at
            .split('T')
            .next()
            .unwrap_or("unknown")
            .to_string();
        project_meta_by_date
            .entry(proj_date)
            .or_default()
            .push(proj);

        for item in &proj.items {
            let item_date = item
                .occurred_at
                .split('T')
                .next()
                .unwrap_or("unknown")
                .to_string();
            items_by_date_and_project
                .entry(item_date)
                .or_default()
                .entry((proj.project_title.clone(), proj.project_key.clone()))
                .or_default()
                .push(item);
        }
    }

    // 收集所有涉及的日期并降序遍历
    let mut all_dates: Vec<String> = items_by_date_and_project
        .keys()
        .chain(project_meta_by_date.keys())
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    all_dates.sort_by(|a, b| b.cmp(a)); // 降序

    for date in all_dates {
        out.push_str(&format!("## {}\n\n", date));

        // 该日期下有活动的项目
        let mut rendered_projects_for_date: BTreeMap<(String, String), Option<&RecentProjectView>> =
            BTreeMap::new();

        if let Some(projs) = project_meta_by_date.get(&date) {
            for p in projs {
                rendered_projects_for_date
                    .insert((p.project_title.clone(), p.project_key.clone()), Some(p));
            }
        }

        if let Some(item_projects) = items_by_date_and_project.get(&date) {
            for key in item_projects.keys() {
                rendered_projects_for_date
                    .entry(key.clone())
                    .or_insert(None);
            }
        }

        for ((proj_title, proj_key), proj_opt) in rendered_projects_for_date {
            out.push_str(&format!("### {}\n\n", proj_title));

            if let Some(proj) = proj_opt {
                out.push_str("#### What changed\n");
                out.push_str(proj.summary.trim());
                out.push_str("\n\n");

                // 建议下一步 (从 items 中筛选 recommendation_rank 1..=3)
                let mut suggestions: Vec<&RecentMemoryItemView> = proj
                    .items
                    .iter()
                    .filter(|item| item.recommendation_rank.is_some())
                    .collect();
                suggestions.sort_by_key(|s| s.recommendation_rank.unwrap_or(99));

                out.push_str("#### Suggested next steps\n");
                if suggestions.is_empty() {
                    out.push_str("1. 本窗口没有形成明确下一步\n\n");
                } else {
                    for (idx, s) in suggestions.iter().enumerate() {
                        out.push_str(&format!("{}. {}\n", idx + 1, s.title.trim()));
                    }
                    out.push('\n');
                }
            }

            // 渲染属于该日期的条目
            if let Some(item_projects) = items_by_date_and_project.get(&date) {
                if let Some(items) = item_projects.get(&(proj_title.clone(), proj_key.clone())) {
                    out.push_str("#### Memory items\n");
                    for item in items {
                        let category_str = match item.category.as_str() {
                            "decision" => "Decision",
                            "research" => "Research",
                            "verification" => "Verification",
                            "blocker" => "Blocker",
                            "follow_up" => "FollowUp",
                            _ => "Progress",
                        };

                        out.push_str(&format!(
                            "- **{} · {}** — {}\n",
                            category_str,
                            item.status,
                            item.title.trim()
                        ));
                        if !item.rationale.trim().is_empty() {
                            out.push_str(&format!("  - Why: {}\n", item.rationale.trim()));
                        }

                        let session_titles: Vec<String> = item
                            .session_references
                            .iter()
                            .map(|s| {
                                if s.available {
                                    format!("`{}`", s.session_title.trim())
                                } else {
                                    format!("`{}` (来源不可用)", s.session_title.trim())
                                }
                            })
                            .collect();

                        if !session_titles.is_empty() {
                            out.push_str(&format!("  - Sessions: {}\n", session_titles.join(", ")));
                        }
                    }
                    out.push('\n');
                }
            }
        }
    }

    out
}

/// 渲染 MEMORY.md (M35-PROJ-02, 07-markdown-ui-settings.md Section 4)
pub(crate) fn render_memory_long_term_markdown(
    l2_projects: &[L2ProjectMemoryView],
    l3_items: &[L3MemoryItemView],
    published_at: &str,
    revision_hash: &str,
) -> String {
    let mut out = String::new();

    out.push_str("# Memory\n\n");
    out.push_str(&format!("- Published: {}\n", published_at));
    out.push_str(&format!("- Revision: {}\n\n", revision_hash));

    // 1. Project Memory (L2)
    out.push_str("## Project Memory\n\n");

    let mut sorted_projects = l2_projects.to_vec();
    sorted_projects.sort_by(|a, b| a.project_key.cmp(&b.project_key));

    for proj in sorted_projects {
        out.push_str(&format!("### {}\n\n", proj.project_key));

        let mut sorted_items = proj.items.clone();
        // 过滤非 current 条目 (M35-L3-06)
        sorted_items.retain(|i| i.lifecycle == "current");

        // 按 category, updated_at 降序, ID 排序
        sorted_items.sort_by(|a, b| {
            category_order(&a.category)
                .cmp(&category_order(&b.category))
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.item_id.cmp(&b.item_id))
        });

        if sorted_items.is_empty() {
            out.push_str("- *No active project memories*\n\n");
        } else {
            for item in sorted_items {
                let cat_str = match item.category {
                    MemoryItemCategory::Decision => "Decision",
                    MemoryItemCategory::Research => "Research",
                    MemoryItemCategory::Verification => "Verification",
                    MemoryItemCategory::Blocker => "Blocker",
                    MemoryItemCategory::FollowUp => "FollowUp",
                    MemoryItemCategory::Progress => "Progress",
                };

                out.push_str(&format!("- **{}** — {}\n", cat_str, item.title.trim()));
                if !item.rationale.trim().is_empty() {
                    out.push_str(&format!("  - Why: {}\n", item.rationale.trim()));
                }
                out.push_str(&format!("  - Updated: {}\n", item.updated_at));

                let avail_count = item
                    .source_references
                    .iter()
                    .filter(|r| r.available)
                    .count();
                let unavail_count = item.source_references.len() - avail_count;

                if unavail_count > 0 {
                    out.push_str(&format!(
                        "  - Sources: {} available, {} unavailable\n",
                        avail_count, unavail_count
                    ));
                } else if avail_count > 0 {
                    out.push_str(&format!("  - Sources: {} available\n", avail_count));
                }
            }
            out.push('\n');
        }
    }

    // 2. Permanent Memory (L3)
    out.push_str("## Permanent Memory\n\n");

    let mut sorted_l3 = l3_items.to_vec();
    // 过滤非 current 条目 (M35-L3-06)
    sorted_l3.retain(|i| i.lifecycle == "current");

    sorted_l3.sort_by(|a, b| {
        category_order(&a.category)
            .cmp(&category_order(&b.category))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.item_id.cmp(&b.item_id))
    });

    if sorted_l3.is_empty() {
        out.push_str("- *No active permanent memories*\n\n");
    } else {
        for item in sorted_l3 {
            let cat_str = match item.category {
                MemoryItemCategory::Decision => "Rule",
                MemoryItemCategory::Research => "Conclusion",
                MemoryItemCategory::Verification => "Constraint",
                MemoryItemCategory::Blocker => "Blocker",
                MemoryItemCategory::FollowUp => "Preference",
                MemoryItemCategory::Progress => "Pattern",
            };

            out.push_str(&format!("- **{}** — {}\n", cat_str, item.title.trim()));
            if !item.rationale.trim().is_empty() {
                out.push_str(&format!("  - Why: {}\n", item.rationale.trim()));
            }
            out.push_str(&format!("  - Updated: {}\n", item.updated_at));
        }
        out.push('\n');
    }

    out
}

pub(crate) fn category_order(cat: &MemoryItemCategory) -> usize {
    match cat {
        MemoryItemCategory::Decision => 1,
        MemoryItemCategory::Verification => 2,
        MemoryItemCategory::Research => 3,
        MemoryItemCategory::Blocker => 4,
        MemoryItemCategory::FollowUp => 5,
        MemoryItemCategory::Progress => 6,
    }
}
