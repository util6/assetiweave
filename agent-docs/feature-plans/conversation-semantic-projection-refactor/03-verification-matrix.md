# Conversation 重构：验证矩阵

## 1. 测试接缝

优先从最高层稳定边界证明行为：

- 纯 parser/projector：局部 Rust 单元测试；
- membership、merge/split、同步和迁移：临时 SQLite 上的 repository/AppService 集成测试；
- Tauri 与 Engine：公开 adapter parity/contract 测试，不复制领域算法；
- CLI：生成契约、Go 单测和 CLI–Engine e2e；
- 前端：service/provider/component/page 组合测试；
- 可见交互：browser/Tauri 手工验证，记录 fixture 与结果。

测试应断言稳定身份、可见行为和持久副作用，不以私有调用次数或结构存在为主要证据。

## 2. 领域不变量矩阵

| 编号 | 行为 | 必须证明 |
|---|---|---|
| Q-01 | Turn 唯一归属 | 同一 Turn 不会同时属于两个有效 Question |
| Q-02 | session 完整性 | 跨 tenant/session membership 被拒绝或审计 |
| Q-03 | 稳定顺序 | Question 内 Turn 顺序不依赖查询偶然顺序 |
| Q-04 | 全量/增量等价 | 相同来源事实得到相同 Question 与 membership |
| Q-05 | reconciliation 幂等 | 重跑不增加 Question、关系或正文 |
| Q-06 | merge | 只迁移关系和引用，不复制 Turn/Part |
| Q-07 | split | Turn 子集迁移后引用定位正确 |
| Q-08 | 人工 fence | 自动同步不覆盖人工归组 |
| P-01 | 投影可追溯 | 每个 Content Node 可定位源 Turn/Part |
| P-02 | 一对多投影 | 一个 Part 可稳定生成多个节点与片段 ID |
| P-03 | 短 ID 稳定 | Question merge/split 后 Part 短 ID 不变 |
| S-01 | 原始执行 | 一次 Shell Execution 只保存一个命令 Part |
| S-02 | 展示保持 | 多命令仍逐条显示 |
| S-03 | printf 过滤 | 分隔命令不显示，标签提炼稳定 |
| S-04 | 命令边界 | 引号、here-doc、多行、管道和重定向无损 |
| S-05 | 结果归属 | result 不按展示节点重复存储 |

## 3. 消费者矩阵

| 消费者 | 验收重点 |
|---|---|
| Search / FTS | 从关系与事实重建；多节点精确命中；无孤立索引 |
| Block Locator / 深链 | 同时携带 Question、Turn、Part 和片段身份 |
| Export | raw 事实与 rendered 投影明确分离 |
| Engine | 使用 AppService canonical workflow；无 Card 语义旁路 |
| CLI | 使用生成契约；请求消费者专用最小载荷 |
| Frontend | 唯一层级 DTO；无运行时 fallback；现有视觉行为不变 |
| Browser mock | 与公开契约同步，不成为第二规则引擎 |

## 4. 历史迁移矩阵

至少准备以下脱敏 fixture：

1. 单 Turn 单 Question；
2. “问题 → 继续”形成多 Turn Question；
3. 中断后恢复；
4. 人工合并与拆分；
5. 旧 Question 快照与 Part 一致；
6. 旧 Question 快照与 Part 不一致；
7. 旧拆分 Shell Execution；
8. 一个 execution 内含多个 `printf` 分割标记；
9. 会话 `48d4ef52` 场景的最小脱敏复现；
10. 空库与大规模会话。

每个 migration fixture 验证 dry-run、backup、apply、重复 apply、verify 和 rollback。

## 5. 性能与响应性

在 #16 固定并记录基线，至少包含：

- Question 列表与详情在小/大 Session 下的查询数量和延迟；
- 全量同步、等价增量同步与 reconciliation；
- FTS 全量重建和增量更新；
- 一个 Part 投影大量命令节点时的载荷与渲染；
- 历史审计和修复的吞吐、内存与进度更新；
- 后台任务运行时导航、筛选和设置仍可用；
- 持久化 Part 数量按真实执行单元计，不随展示节点数量线性放大。

不预设脱离当前基线的绝对阈值。首个相关工单记录施工前基线，#16 以同一 fixture 对比并解释差异。

## 6. 质量门禁

按改动范围运行，最终 #16 必须运行完整集合：

```bash
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:contract
pnpm cli:test:e2e
```

`pnpm cli:contract` 可能改变生成物；执行者必须先确认本工单是否改变公开 Engine 契约，再提交对应生成结果。
每条完成评论记录逐条命令、结果和未运行原因，不用“测试通过”概括全部证据。

Memory 相关测试只作为仓库全量门禁的既有回归存在，不作为本轮 Conversation 重构的
功能验收证据；本轮不新增或修改 Memory fixture。
