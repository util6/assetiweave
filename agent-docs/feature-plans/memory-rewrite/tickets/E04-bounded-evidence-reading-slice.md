# E04: 精简证据读取垂直切片 (Bounded Evidence Reading Vertical Slice)

## 1. 切片元数据

- **切片身份**: E4
- **前置依赖 (Blocked by)**: E3 (Commit `4c519ab`)
- **主要验收用例**:
  - E01: 同事实附带大量重复环境与源码输出后生成：模型可见首包和读取均有界，原 canonical 事实不变。
  - E04: 长会话含多个 Question，关键纠正在中段，单项超长：索引/分段覆盖不偏向首尾；分片可定位且没有静默截断。
  - E05: 提交未知、跨 tenant、跨 Session、旧 revision 或未读正文引用：明确失败，不持久化成功 Memory，不泄漏范围外存在性。
  - E06: 连续读取、重复读取、重试或跨分段耗尽预算：总量上限生效，原因可见，不无限重试，不把部分结果提升成功。
  - E07: Adapter 已丢弃正文、来源缺失、或证据包为空：明确区分不可用、未读、无内容，不回读私有日志或伪造历史。
  - E15: 固定合同防御：产品限制生效，工具补读受限于白名单和本次 WorkOrder 范围。

## 2. 架构决策与设计细节

1. **证据模型与数据结构 (`models/evidence.rs`)**:
   - `ShortEvidenceRef`: 执行内短引用（`ref_key`, `question_id`, `turn_id`, `node_id`, `role`, `status`），隐藏内部 DB id。
   - `BoundedEvidenceNode`: 单项精简节点（按单项预算截断，经过脱敏处理）。
   - `EvidenceIndexEntry`: 会话内未展开内容的大纲与补读索引。
   - `EvidenceCoverage`: 覆盖度跟踪（包含 total_questions, total_turns, read, indexed, truncated, unavailable 等）。
   - `BoundedEvidenceInitialPack`: 精简首包（受限于 32,000 字符与 32 个节点预算）。
2. **确定性首包生成器 (`evidence/pack_builder.rs`)**:
   - 输入为 `NormalizedConversationSession` 和 `MemoryExecutionWorkOrder`。
   - 过滤重复环境注入、不展开完整源码和巨型 diff。
   - 提取任务边界、用户意图与纠正修正、结果和验证证据。
   - 脱敏处理并按单项上限裁剪，生成短引用映射与未读索引。
3. **受限补读上下文与工具调用器 (`evidence/reader_session.rs`)**:
   - 实现 4 个固定白名单工具：`get_session_outline`, `search_session_content`, `read_question_content`, `read_content_node`。
   - 严格在内存映射中做短引用校验，跨 Tenant / 跨 Session / 不存在引用明确拦截，防止侧信道探测。
   - 严格累加字符数与调用次数，单次超限截断，累计超限终止，标记预算耗尽。
   - 缺失或空内容标记为 `Unavailable`，不伪造历史。

## 3. 验收记录

- [x] Red 测试编写：覆盖 E01（超大环境输出下首包严格受限于 32k/32 nodes）、E04（长会话多 Question 覆盖中段纠正与可定位分片）、E05（越界/未知/跨会话引用被拦截）、E06（多次读取耗尽预算强制熔断）、E07（内容缺失区分不可用与无内容）、E15（越权工具拒绝）。
- [x] Minimal 实现：实现证据模型、首包生成器和受限阅读会话。
- [x] 验证通过：运行 `cargo test --manifest-path src-tauri/Cargo.toml` 相关测试全部通过（12 passed）。
