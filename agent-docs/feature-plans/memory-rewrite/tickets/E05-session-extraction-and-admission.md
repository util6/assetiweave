# E05: Session 提取与准入切换 (Session Extraction & Admission Cutover)

## 1. 切片元数据

- **切片身份**: E5
- **前置依赖 (Blocked by)**: E4 (Commit `5232308`)
- **主要验收用例**:
  - E02: 用户先提议后否决，Agent 最后仍复述旧方案：输出不把旧方案记作已确认；修正与理由可追溯。
  - E03: Agent 声称通过，但没有验证输出或有失败结果：保留声称/验证的区别，不生成虚假通过结论。
  - E05: 提交未知、跨 tenant、跨 Session、旧 revision 或未读正文引用：明确失败，不持久化成功 Memory，不泄漏范围外存在性。
  - E08: 提取有效事实并投影到摘要/事件：必需语义可读且事实不重复生成，引用可跳转；无内容不制造事件。

## 2. 架构决策与设计细节

1. **统一 Work Order 与精简首包流水线**:
   - `execute_session_memory_agent` 切换为使用 E4 的 `build_bounded_evidence_initial_pack` 生成受限首包。
   - 首包严格限定在 32,000 字符与 32 个节点预算内，避免向模型暴露未脱敏或重复的环境信息与巨型 diff。
2. **一次提取事实，多可读投影 (Single Fact Set -> Multiple Projections)**:
   - 提取模型输出包含结构化事实（含 `category`, `status`, `text`, `source_refs`）。
   - 程序端统一负责将这组事实组织为 Session Memory 字段（`summary`, `goal`, `result`, `decisions`, `verification`, `blockers`, `follow_up`）与 `RecentMemoryEvent`，消除模型在多处重复描述的不一致。
3. **空内容终态处理**:
   - 会话中无有效可提取内容时（如全空白或全部为不可用节点），标记为明确无内容状态，不生成虚假 Memory 或近期事件，避免污染近期工作区。
4. **严格准入校验 (Admission Verification)**:
   - 校验所有模型引用的短引用（`ref_key`），确保属于当前会话与有效已读范围。非法引用或跨会话引用直接拦截并拒绝持久化。
   - 严格区分“用户确认决定”与“已否决提案”，区分“证据支持的验证”与“无证据声称”。

## 3. 验收记录

- [x] Red 测试编写：覆盖 E02（否决方案不记为已确认）、E03（声称通过与实际验证分离）、E05（非法短引用准入拦截）、E08（有效事实投影到摘要与事件，空内容不造事件）。
- [x] Minimal 实现：重构 `session_memory.rs` 中的 Agent 执行与准入校验流水线。
- [x] 验证通过：运行 `cargo test --manifest-path src-tauri/Cargo.toml` 包含所有 E01–E10, E15 验收用例通过。
