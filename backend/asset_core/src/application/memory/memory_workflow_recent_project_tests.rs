use super::*;
use crate::backend::domain::memory::{
    MemoryTreeNode, NodeStatus, ProjectMemoryState, RecentMemoryState, TopicTree,
};
use uuid::Uuid;

#[test]
fn test_project_path_to_slug() {
    assert_eq!(project_path_to_slug("/code/my-project"), "code-my-project");
    assert_eq!(
        project_path_to_slug("C:\\Users\\admin\\repo"),
        "c-users-admin-repo"
    );
    assert_eq!(project_path_to_slug("///"), "unspecified");
    assert_eq!(project_path_to_slug("   "), "unspecified");
}

#[test]
fn test_is_valid_anchor_item_filter() {
    // 合法单行锚点项
    assert!(is_valid_anchor_item(
        "- [73c20273#T2](aiwc://session/73c20273/turn/2): 落地 ADR-0018 Markdown 优先架构"
    ));
    assert!(is_valid_anchor_item(
        "- [s1#T1](https://example.com/item): some valid technical conclusion"
    ));

    // 非法情况 1: 缩进子列表或缩进解释
    assert!(!is_valid_anchor_item(
        "  - [73c20273#T2](aiwc://session/73c20273/turn/2): 缩进的子项应该被过滤"
    ));
    assert!(!is_valid_anchor_item("  - 说明: 这是一段缩进的补充说明"));
    assert!(!is_valid_anchor_item("\t- [s#T1](aiwc://s/1): tab缩进"));

    // 非法情况 2: 无锚点链接的普通列表或解释
    assert!(!is_valid_anchor_item("- 说明: 仅供参考的说明"));
    assert!(!is_valid_anchor_item("- 备注: 不需要记录"));
    assert!(!is_valid_anchor_item("- 纯文本无链接列表项"));
    assert!(!is_valid_anchor_item("这是自由文本段落，不是列表"));

    // 非法情况 3: 格式残缺的 Markdown 链接
    assert!(!is_valid_anchor_item("- [仅有方括号无圆括号]: 说明"));
    assert!(!is_valid_anchor_item("- [](aiwc://s/1): 空锚点名"));
    assert!(!is_valid_anchor_item("- [s#T1](): 空目标链接"));
    assert!(!is_valid_anchor_item("- [s#T1](aiwc://s/1)")); // 无技术描述
    assert!(!is_valid_anchor_item("- [s#T1](aiwc://s/1): ")); // 仅冒号无描述

    // 非法情况 4: 占位符过滤
    assert!(!is_valid_anchor_item("- 无待探索分支"));
    assert!(!is_valid_anchor_item("- 无已完成任务"));
    assert!(!is_valid_anchor_item(
        "- [s#T1](aiwc://s/1): 暂无待探索分支"
    ));
}

#[test]
fn test_parse_session_markdown_robustness() {
    let raw_content = r#"---
session_id: session-xyz-12345
project_path: /Users/util6/code-space/assetiweave
updated_at: 2026-10-27T02:00:00Z
---

# 会话 session-xyz 思维分叉与任务进展

这是一段对本次会话的整体总结描述，不应该被解析进条目。

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [xyz#T1](aiwc://session/xyz/turn/1): 探索 SQLite WAL 模式在高并发下的读写表现
  - 说明: 这是一个二级说明，不应该作为独立叶子污染条目池
  - 细节: 仅需评测压力测试场景
- 说明: 这是一个非锚点的子列表项，必须被忽略
- [xyz#T2](aiwc://session/xyz/turn/2): 调研 Tantivy 分面索引在资产搜索中的剪枝潜力

## 💡 中间无关小节 (Other Section)
- [xyz#T9](aiwc://session/xyz/turn/9): 这里是无关小节的锚点，不应混入已完成或悬空
这是一些背景文字。

## ✅ 已完成的闭环任务 (Completed Tasks)
- [xyz#T3](aiwc://session/xyz/turn/3): 实现 Markdown 优先存储并落地原子写协议
  - 验证: cargo test 全绿通过
- 备注: 临时修复了测试超时
- [xyz#T4](aiwc://session/xyz/turn/4): 重构 MemoryStorage 支持 tenant 隔离路径
- 无已完成任务
"#;

    let parsed = parse_session_markdown(raw_content);

    assert_eq!(parsed.session_id, "session-xyz-12345");
    assert_eq!(
        parsed.project_path.as_deref(),
        Some("/Users/util6/code-space/assetiweave")
    );
    assert_eq!(parsed.updated_at, "2026-10-27T02:00:00Z");

    assert_eq!(parsed.unexplored_branches.len(), 2);
    assert_eq!(
        parsed.unexplored_branches[0],
        "- [xyz#T1](aiwc://session/xyz/turn/1): 探索 SQLite WAL 模式在高并发下的读写表现"
    );
    assert_eq!(
        parsed.unexplored_branches[1],
        "- [xyz#T2](aiwc://session/xyz/turn/2): 调研 Tantivy 分面索引在资产搜索中的剪枝潜力"
    );

    assert_eq!(parsed.completed_tasks.len(), 2);
    assert_eq!(
        parsed.completed_tasks[0],
        "- [xyz#T3](aiwc://session/xyz/turn/3): 实现 Markdown 优先存储并落地原子写协议"
    );
    assert_eq!(
        parsed.completed_tasks[1],
        "- [xyz#T4](aiwc://session/xyz/turn/4): 重构 MemoryStorage 支持 tenant 隔离路径"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_recent_and_project_workflow_projects_state_files() {
    let base = std::env::temp_dir().join(format!("assetiweave-proj-state-{}", Uuid::new_v4()));
    let db_dir = base.join("db");
    let mem_dir = base.join("memories");
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_dir.join("app.db"))
        .await
        .unwrap();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), "default");

    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // 1. Prepare ProjectMemoryState in .state/projects/proj-alpha.json
    let mut proj_state = ProjectMemoryState::new("proj-alpha");
    let mut tree = TopicTree::new("arch-design", "Architecture Design");
    let node1 = MemoryTreeNode {
        id: "alpha#T1".to_string(),
        parent_id: None,
        topic_id: "arch-design".to_string(),
        summary: "Decoupled MCP stdio across desktop and engine".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "sess-alpha-1".to_string(),
        turn_index: 1,
        created_at: now_str.clone(),
        updated_at: now_str.clone(),
    };
    let node2 = MemoryTreeNode {
        id: "alpha#T1.1".to_string(),
        parent_id: Some("alpha#T1".to_string()),
        topic_id: "arch-design".to_string(),
        summary: "Explore ACP streaming latency optimization".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "sess-alpha-1".to_string(),
        turn_index: 1,
        created_at: now_str.clone(),
        updated_at: now_str.clone(),
    };
    tree.insert_node(node1, true);
    tree.insert_node(node2, false);
    proj_state
        .topic_trees
        .insert("arch-design".to_string(), tree);
    proj_state
        .session_watermarks
        .insert("sess-alpha-1".to_string(), 1);

    storage
        .write_project_state_atomic("proj-alpha", &proj_state)
        .unwrap();

    // 2. Prepare RecentMemoryState in .state/recent.json
    let mut recent_state = RecentMemoryState::new();
    recent_state.upsert_project(proj_state.clone());
    storage.write_recent_state_atomic(&recent_state).unwrap();

    // 3. Run node 2 workflow
    let report = service
        .run_recent_and_project_workflow_at(&storage, now)
        .await
        .expect("run recent & project workflow");

    assert_eq!(report.project_count, 1);
    assert_eq!(report.session_count, 1);

    // 4. Verify projects/proj-alpha.md is projected
    let proj_md = storage
        .read_markdown("projects/proj-alpha.md")
        .unwrap()
        .expect("projects/proj-alpha.md must exist");
    assert!(proj_md.contains("# 项目记忆: proj-alpha"));
    assert!(proj_md.contains("Decoupled MCP stdio"));
    assert!(proj_md.contains("Explore ACP streaming latency"));

    // 5. Verify recent/latest.md and recent/<date>.md are projected
    let recent_latest = storage
        .read_markdown("recent/latest.md")
        .unwrap()
        .expect("recent/latest.md must exist");
    assert!(recent_latest.contains("proj-alpha"));
    assert!(recent_latest.contains("Decoupled MCP stdio"));

    let today_str = now.format("%Y-%m-%d").to_string();
    let recent_date = storage
        .read_markdown(format!("recent/{today_str}.md"))
        .unwrap()
        .expect("recent/<date>.md must exist");
    assert_eq!(recent_latest, recent_date);

    // 6. Verify ZERO sessions/*.md generated
    let sessions_dir = storage.sessions_dir();
    assert!(
        !sessions_dir.exists()
            || std::fs::read_dir(&sessions_dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .count()
                == 0,
        "No sessions/*.md must be created"
    );

    drop(service);
    std::fs::remove_dir_all(base).ok();
}
