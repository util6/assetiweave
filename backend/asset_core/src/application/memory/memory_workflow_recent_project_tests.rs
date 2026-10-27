use super::*;

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
fn test_leaf_ablation_logic() {
    let existing_leaves = vec![
        "- [s1#T1](aiwc://session/s1/turn/1): explore redis caching options".to_string(),
        "- [s1#T2](aiwc://session/s1/turn/2): explore grpc benchmark".to_string(),
    ];
    let completed_tasks = vec![
        "- [s2#T1](aiwc://session/s2/turn/1): implemented redis caching and verified".to_string(),
    ];
    let new_leaves =
        vec!["- [s2#T2](aiwc://session/s2/turn/2): check sqlite wal mode performance".to_string()];

    let remaining = ablate_and_merge_leaves(&existing_leaves, &new_leaves, &completed_tasks);

    // Redis caching leaf should be ablated (removed) because it's now completed!
    assert!(!remaining.iter().any(|l| l.contains("redis caching")));
    // GRPC benchmark was not touched, should remain
    assert!(remaining.iter().any(|l| l.contains("grpc benchmark")));
    // New WAL leaf should be added
    assert!(remaining.iter().any(|l| l.contains("sqlite wal mode")));
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

    // 验证待探索悬空叶子：严格只有 2 条合法项，所有解释、子列表、无关小节均被防御忽略
    assert_eq!(parsed.unexplored_branches.len(), 2);
    assert_eq!(
        parsed.unexplored_branches[0],
        "- [xyz#T1](aiwc://session/xyz/turn/1): 探索 SQLite WAL 模式在高并发下的读写表现"
    );
    assert_eq!(
        parsed.unexplored_branches[1],
        "- [xyz#T2](aiwc://session/xyz/turn/2): 调研 Tantivy 分面索引在资产搜索中的剪枝潜力"
    );

    // 验证已完成任务：严格只有 2 条合法项，验证子项、备注和占位符均被防御过滤
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
async fn test_real_sample_node2_node3_workflow_simulation() {
    use uuid::Uuid;

    let base = std::env::temp_dir().join(format!("assetiweave-real-sample-{}", Uuid::new_v4()));
    let db_dir = base.join("db");
    let mem_dir = base.join("memories");
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_dir.join("app.db"))
        .await
        .unwrap();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), "default");

    let proj_path = "/Users/util6/code-space/assetiweave";
    let proj_slug = project_path_to_slug(proj_path);

    // 1. 初始化既有项目记忆文件 projects/<slug>.md，含有历史遗留叶子
    let initial_project_md = format!(
        r#"---
project_path: {proj_path}
updated_at: 2026-10-23T10:00:00Z
---

# 项目记忆: {proj_path}

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [old#T1](aiwc://session/old/turn/1): 确认 Ticket 03 细粒度重构拆分可行性
- [old#T2](aiwc://session/old/turn/2): 探索 Tantivy 分面索引在资产过滤中的性能表现
"#
    );
    storage
        .write_markdown_atomic(format!("projects/{proj_slug}.md"), &initial_project_md)
        .unwrap();

    let now = chrono::Utc::now();
    let sess1_updated_at = (now - chrono::Duration::hours(2)).to_rfc3339();
    let sess2_updated_at = (now - chrono::Duration::hours(12)).to_rfc3339();

    // 2. 模拟由节点 1 提炼并写入的真实会话产物 sessions/2026-10-26_5ab333d1.md
    //    包含合法的锚点条目，同时混入自由多行文本和说明子列表进行解析防御压力测试
    let sess1_md = format!(
        r#"---
session_id: conversation-session-5ab333d1
short_id: 5ab333d1
project_path: {proj_path}
updated_at: {sess1_updated_at}
turn_count: 15
has_signal: true
---

# 会话 5ab333d1 思维分叉与任务进展

这是一段会话背景概览。

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [5ab333d1#T11](aiwc://session/5ab333d1/turn/11): 评估 Ticket 03 读路径拆分为详情列表与定位导出两个子任务
  - 说明: 细分拆分可降低单票认知负担
- [5ab333d1#T11](aiwc://session/5ab333d1/turn/11): 评估 web 镜像表族是否独立拆为镜像票串行推进
- 备注: 这是杂音文本，不应收录

## ✅ 已完成的闭环任务 (Completed Tasks)
- [5ab333d1#T12](aiwc://session/5ab333d1/turn/12): 确认 Ticket 03 细粒度重构拆分方案
  - 效果: 用户已明确拍板拆分 03
"#
    );
    storage
        .write_markdown_atomic("sessions/2026-10-26_5ab333d1.md", &sess1_md)
        .unwrap();

    // 3. 模拟真实会话产物 sessions/2026-10-24_b75930a3.md
    let sess2_md = format!(
        r#"---
session_id: conversation-session-b75930a3
short_id: b75930a3
project_path: {proj_path}
updated_at: {sess2_updated_at}
turn_count: 2
has_signal: true
---

# 会话 b75930a3 思维分叉与任务进展

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [b75930a3#T1](aiwc://session/b75930a3/turn/1): 调研纯服务器无 GUI 环境下独立安装和运行 CLI 服务的打包分发方案

## ✅ 已完成的闭环任务 (Completed Tasks)
- [b75930a3#T2](aiwc://session/b75930a3/turn/2): 确定无 GUI 环境下 CLI 二进制独立运行与客户端捆绑安装的双轨定位
"#
    );
    storage
        .write_markdown_atomic("sessions/2026-10-24_b75930a3.md", &sess2_md)
        .unwrap();

    // 4. 执行节点 2 工作流：Recent 窗口聚合与 Project 悬空叶子消融
    let node2_report = service
        .run_recent_and_project_workflow_at(&storage, now)
        .await
        .expect("run node 2 workflow");

    assert_eq!(node2_report.session_count, 2);
    assert_eq!(node2_report.project_count, 1);

    // 验证 recent/latest.md 产物
    let recent_latest = storage
        .read_markdown("recent/latest.md")
        .unwrap()
        .expect("recent/latest.md must exist");
    assert!(recent_latest.contains(&format!("## 项目: {proj_path}")));
    assert!(recent_latest.contains("评估 web 镜像表族"));
    assert!(recent_latest.contains("调研纯服务器无 GUI 环境下独立安装"));
    assert!(recent_latest.contains("确认 Ticket 03 细粒度重构拆分方案"));
    assert!(!recent_latest.contains("- 备注: 这是杂音文本"));
    assert!(!recent_latest.contains("细分拆分可降低单票认知负担"));

    // 验证归档文件 recent/<date>.md 同样存在
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let archive_path = format!("recent/{today}.md");
    let recent_archive = storage
        .read_markdown(&archive_path)
        .unwrap()
        .expect("recent archive file must exist");
    assert_eq!(recent_latest, recent_archive);

    // 验证 projects/<slug>.md 经过语义消融后的状态：
    let project_md_after = storage
        .read_markdown(format!("projects/{proj_slug}.md"))
        .unwrap()
        .expect("project markdown must exist");

    // 1) 旧叶子 old#T1 因与 5ab333d1#T12 (确认 Ticket 03 细粒度重构拆分) 闭环而被消融剔除！
    assert!(
        !project_md_after.contains("old#T1"),
        "old#T1 must be ablated because ticket 03 split was resolved"
    );

    // 2) 未触及的旧叶子 old#T2 (Tantivy 分面索引) 必须安全保留！
    assert!(
        project_md_after.contains("old#T2"),
        "old#T2 must remain in project leaves pool"
    );

    // 3) 本轮新增的叶子应当合入
    assert!(project_md_after.contains("5ab333d1#T11"));
    assert!(project_md_after.contains("b75930a3#T1"));

    // 4) 严格单行锚点，不含二级缩进与非锚点杂音
    assert!(!project_md_after.contains("细分拆分可降低"));
    assert!(!project_md_after.contains("- 备注:"));

    // 5. 执行节点 3 工作流：Global MEMORY.md 长期记忆沉淀
    let node3_report = service
        .run_global_memory_workflow(&storage)
        .await
        .expect("run node 3 workflow");
    assert!(node3_report.updated);

    let global_mem = storage
        .read_markdown("MEMORY.md")
        .unwrap()
        .expect("MEMORY.md must exist");
    assert!(global_mem.contains("# 全局长期记忆与工程准则 (Global Memory)"));
    assert!(global_mem.contains("通用工程规范与架构准则"));
    assert!(global_mem.contains("近期研发沉淀事实与决策焦点"));
    // 包含提炼出来的最新闭环事实
    assert!(global_mem.contains("确认 Ticket 03 细粒度重构拆分方案"));

    // 验证文件体积在合理基线内 (< 3KB)
    assert!(recent_latest.len() < 3000);
    assert!(project_md_after.len() < 3000);
    assert!(global_mem.len() < 3000);

    drop(service);
    std::fs::remove_dir_all(base).ok();
}

#[tokio::test]
async fn test_recent_window_hours_skips_expired_sessions() {
    let tmp = std::env::temp_dir().join(format!("aiwc_test_window_{}", uuid::Uuid::new_v4()));
    let db_dir = tmp.join("db");
    let mem_dir = tmp.join("memories/default");
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_dir.join("app.db"))
        .await
        .unwrap();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), "default");

    let now = chrono::Utc::now();
    let proj_path = "/Users/util6/code-space/assetiweave";

    // 1. 窗口内的活跃会话 (2小时前，符合 48h 窗口)
    let recent_time = (now - chrono::Duration::hours(2)).to_rfc3339();
    let recent_sess = format!(
        r#"---
session_id: sess-recent
short_id: recent01
project_path: {proj_path}
updated_at: {recent_time}
turn_count: 5
has_signal: true
---

# 最近活跃会话

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [recent01#T1](aiwc://session/recent01/turn/1): 探索流式传输背压机制

## ✅ 已完成的闭环任务 (Completed Tasks)
- [recent01#T2](aiwc://session/recent01/turn/2): 实现基础流水线事件通道
"#
    );
    storage
        .write_markdown_atomic("sessions/2026-10-27_recent01.md", &recent_sess)
        .unwrap();

    // 2. 超期历史会话 (120小时前，远超 48h / 72h 窗口)
    let expired_time = (now - chrono::Duration::hours(120)).to_rfc3339();
    let expired_sess = format!(
        r#"---
session_id: sess-expired
short_id: expire01
project_path: {proj_path}
updated_at: {expired_time}
turn_count: 3
has_signal: true
---

# 远古过期历史会话

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [expire01#T1](aiwc://session/expire01/turn/1): 远古时代遗留的待探索分支

## ✅ 已完成的闭环任务 (Completed Tasks)
- [expire01#T2](aiwc://session/expire01/turn/2): 远古时代已完成的任务
"#
    );
    storage
        .write_markdown_atomic("sessions/2026-10-20_expire01.md", &expired_sess)
        .unwrap();

    // 3. 执行 Recent 聚合
    let report = service
        .run_recent_and_project_workflow_at(&storage, now)
        .await
        .expect("run recent workflow");

    // 验证：超期会话被跳过，只有 1 个会话被纳入统计
    assert_eq!(report.session_count, 1);

    // 验证 recent/latest.md 产物
    let recent_latest = storage
        .read_markdown("recent/latest.md")
        .unwrap()
        .expect("recent/latest.md must exist");

    // 窗口内的会话内容必须存在
    assert!(recent_latest.contains("探索流式传输背压机制"));
    assert!(recent_latest.contains("实现基础流水线事件通道"));

    // 窗口外的超期会话内容严禁混入！
    assert!(
        !recent_latest.contains("远古时代遗留的待探索分支"),
        "Expired session unexplored branch must NOT be in recent/latest.md"
    );
    assert!(
        !recent_latest.contains("远古时代已完成的任务"),
        "Expired session completed task must NOT be in recent/latest.md"
    );

    drop(service);
    std::fs::remove_dir_all(tmp).ok();
}
