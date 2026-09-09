# E03: 执行合同与 Recipe 快照 (Execution Contract & Recipe Snapshots)

## 1. 切片元数据

- **切片身份**: E3
- **前置依赖 (Blocked by)**: E2 (Commit `d3d6dcf`)
- **主要验收用例**:
  - E09: 读取中修改来源或 Recipe，再让旧任务晚到：旧结果不覆盖新目标；同 fingerprint 重放不重复提交。
  - E10: 运行中 cancel、lease 过期、退出、重启：重开数据库后状态可恢复，任务具有明确终态，预算及 coverage 不无界重置。
  - E15: Recipe 或证据要求扩权、跨范围、访问外网、递归生成：产品固定合同生效，后端能力不足明确失败，不靠提示词宣称隔离。

## 2. 架构决策与设计细节

1. **Memory Recipe 模型与快照**:
   - 定义 `MemoryRecipe`: 包含 `id`, `revision`, `name`, `focus_areas`, `ignored_topics`, `terminology`, `custom_instructions`。
   - 提供内置默认 balanced recipe (`MemoryRecipe::default_builtin()`)。
   - 提供不可变快照 `MemoryRecipeSnapshot` 与内容哈希 `content_hash()`（SHA256）。
2. **执行工单 `MemoryExecutionWorkOrder`**:
   - 绑定执行范围 (`session_id`, `source_id`, `source_revision`, `source_fingerprint`)。
   - 绑定合同与策略版本: `contract_version` ("memory.contract.v1"), `budget_policy_version` ("budget.v1"), `budget_policy: BoundedMemoryBudgetPolicy`。
   - 绑定 Recipe 快照 (`recipe: MemoryRecipeSnapshot`)。
   - 计算不可变输入指纹 `input_fingerprint` = SHA256(`source_fingerprint` + `recipe_hash` + `contract_version` + `budget_version`)。
3. **数据库与持久化层拓展**:
   - Migration `20260909000002_session_memory_job_work_order.sql`:
     - `session_memory_jobs`: 添加 `recipe_id`, `recipe_revision`, `recipe_content_hash`, `budget_policy_version`, `work_order_json`。
     - `session_memories`: 添加 `recipe_id`, `recipe_content_hash`, `work_order_json`。
   - `session_memory_repo`:
     - 读写支持上述新字段。
     - `persist_session_memory_sqlx` 增加后到防覆盖校验 (E09): 如果目标 session 已有更大 source_revision 或更新时间戳的 active memory，旧任务自动转为 `skipped` (或 `last_error = 'superseded_by_newer_target'`)，且不破坏现有最新 memory。
     - 重放保护 (E09): 对同一 memory_id 或 fingerprint 幂等防重。
     - 明确取消与恢复终态 (E10): `cancel_session_memory_job_sqlx` 标记 `canceled`，重开连接/数据库时终态持久保留。
4. **安全边界与固定合同防御 (E15)**:
   - 验证无论 Recipe 中如何包含 Prompt Injection 或提权指令（如要求外网访问、跨 Tenant 读取），WorkOrder 固定合同中工具权限始终受限于产品白名单，隔离属性由后端强类型与数据库层保障。

## 3. 验收记录

- [x] Red 测试编写：覆盖 E09（修改 Recipe/版本后旧任务晚到不覆盖新目标、同 fingerprint 重放幂等）、E10（任务取消与持久化终态恢复）、E15（Recipe 越权提示词无法突破固定合同边界）。
- [x] Minimal 实现：模型与 repo 更新，后到防覆盖逻辑生效。
- [x] 验证通过：运行 `cargo test --manifest-path src-tauri/Cargo.toml bounded_evidence_baseline_tests` 全部 6/6 通过。

