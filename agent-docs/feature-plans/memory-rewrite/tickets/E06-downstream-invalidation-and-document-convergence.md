# E06: 下游、失效与文档收口 (Downstream Invalidation & Document Convergence)

## 1. 切片元数据

- **切片身份**: E6
- **前置依赖 (Blocked by)**: E5 (Commit `e94d432`)
- **主要验收用例**:
  - E11: 来源修订/删除/缺失/迁目录/排除/合同升级：Session → Project → Global、Recent、索引和投影失效一致。
  - E17: Project/Global 文档损坏或发布失败：SQLite last-success 按有效性规则可读，投影可重建，无逐 Session Markdown 依赖。

## 2. 架构决策与设计细节

1. **取消逐 Session Markdown 依赖 (Explicit Revision A)**:
   - Session 提取出的事实完整保存在 SQLite (`session_memories` 表)，不向磁盘写入 `sessions/<id>.md`。
   - Project Consolidation 直接从 SQLite `load_project_memory_inputs_sqlx` 查询成功的 Session Memory，完全不依赖任何磁盘中间文档。
2. **端到端级联失效一致性 (End-to-End Cascade Invalidation)**:
   - 当 Session 对应的 Source 被禁用 (`enabled = 0`)、会话标记为缺失 (`missing = 1`) 或被排除：
     - Session Memory 在业务视图中不可见；
     - Project Memory 查询 (`load_project_memory_latest_version_sqlx`) 自动因来源无效而失效（不返回包含无效会话的旧版本）；
     - Global Memory 查询 (`load_global_memory_inputs_sqlx`) 自动剔除该项目，级联失效；
     - Recent Memory Events 目标定位器 (`load_recent_memory_event_target_sqlx`) 自动拒绝对其导航。
3. **SQLite 作为唯一真相源，支持文档原子重建 (SQLite as Authority & Rebuild)**:
   - Project/Global 磁盘 Markdown (`MEMORY.md`, `SUMMARY.md`) 纯粹为向外部开放的只读投影。
   - 实现 `rebuild_project_memory_documents_for_tenant_at`，与 `rebuild_global_memory_documents_for_tenant_at` 一致：
     - 当磁盘文件损坏、被篡改或丢失时，可无损从 SQLite 的 `last_successful_version` 原子恢复。
     - 磁盘文件缺失不阻碍任何读操作，API 直接从 SQLite 获取 content_markdown。

## 3. 验收记录

- [x] Red 测试编写：覆盖 E11（来源禁用/缺失时 Session -> Project -> Global 级联失效）、E17（无逐 Session Markdown 依赖，磁盘文件损坏时从 SQLite 重建恢复）。
- [x] Minimal 实现：在 `project_memory.rs` 中增加 `rebuild_project_memory_documents_for_tenant_at`，并在 `rebuild_memory_scope` 与调度收口中支持原子恢复；确保下游 Consolidator 仅消费成功 Session。
- [x] 验证通过：运行 `cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application::bounded_evidence_baseline_tests` 全部通过。
