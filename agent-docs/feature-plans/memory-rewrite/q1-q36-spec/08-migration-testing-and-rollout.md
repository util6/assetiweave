# Memory Q1–Q36：迁移、验证、安全与发布

## 1. 迁移策略

采用 expand–migrate–contract，任何阶段保持 current release 可启动、last-success 可读。

### Phase A：冻结目标合同

- Issue #35、ADR-0015 与本目录成为 #35 目标 Authority。
- 为新语义分配 `M35-*` Contract ID。
- 在旧合同和执行路由标注固定 72h、逐项目 Markdown、Recipe 与长期删除传播已被取代。

完成：执行 Agent 不再因读取 #20/#30 旧语义进入冲突 Stop。

### Phase B：Expand Schema/API

- 追加 Memory Item、revision、reference、Snapshot、membership、promotion observation、Recent Job 表。
- 扩展统一设置 schema，默认 48h/02:00/14:00/default Skill。
- 新增 Snapshot API，旧 Recent API 继续可用。

完成：新表为空时旧页面仍通过；AppService 可事务写入并读取 fixture Snapshot。

### Phase C：Generation Skill

- 安装系统生成 Skill；支持普通资产副本和 tenant 设置。
- `memory.contract.v2` Job 固定 Skill identity/hash。
- 旧 Recipe 任务只读兼容，不能覆盖 v2 target。

完成：修改用户 Skill 会改变新 Work Order hash；无效 Skill 在 Agent 前失败。

### Phase D：L1 Cutover

- 实现固定水位 Snapshot、reuse、carry-over 和结构化历史。
- 新 API/前端读取 Snapshot；Session Memory 仍可事件驱动。
- 当前 72h Recent Event API 标 deprecated。

完成：02:00/14:00、24/48/72、missed-watermark、reuse 和 7 天续接通过可控时钟测试。

### Phase E：L2/L3 Cutover

- 对现有成功 Session/Project/Global SQLite 数据运行受控 backfill Consolidation。
- 建立晋升观察、L2/L3 revision、source availability 和 supersede。
- Context Resolver 切换到当前有效 Item revision。

完成：新 L2/L3 成功前旧 last-success 可读；成功后不再依赖旧 Markdown 结构。

### Phase F：Projection/UI Cutover

- 发布租户级两份 Markdown；停止新逐项目/逐 Session 文件。
- Recent 页面切换时间优先 Snapshot DTO；设置与任务中心收口。

完成：磁盘删除后可重建，UI 与 Markdown 读取同一 SQLite state。

### Phase G：Contract

- 证明 active caller 已迁移。
- 移除旧 Recent API/event target、Recipe 新任务路径和逐项目文档写入。
- 旧表保留只读历史或由独立、可回滚 migration 清理；不修改已发布 migration。

完成：surface matrix、CLI contract、grep 分类和全仓测试证明无双轨依赖。

## 2. 回滚

- migration 只追加；发布前使用现有数据库备份能力。
- expand/migrate 阶段可 feature gate 回旧读路径，但不得让旧写路径覆盖新 v2 target。
- 新 Snapshot 失败时回退展示旧 last-success，不回滚 Conversation。
- L2/L3 backfill 失败时保持旧 Project/Global last-success，记录可重试 Job。
- Markdown 失败只重试投影。
- contract 阶段执行前必须有一版稳定发布和数据审计；删除旧表需要单独批准的 destructive migration 工单。

## 3. 主测试缝

唯一主要业务验收缝：

```text
AppService
  + temporary SQLite
  + controllable Clock
  + deterministic Conversation fixtures
  + Fake AgentExecutor
  + temporary Memory projection root
```

测试通过公开 AppService 行为推进完整状态，不断言内部函数调用顺序或 SQL 文本。前端只建立次级 service/component/navigation 集成缝，不复制业务状态机。

## 4. 行为验证矩阵

| ID | Contract | 场景 | 必须观察 |
|---|---|---|---|
| M35-V01 | L1-01 | 新用户设置 | 48h、02:00、14:00、默认 Skill |
| M35-V02 | L1-03 | 14:00 的 48h Snapshot | 区间精确为前 48h且保持稳定 |
| M35-V03 | L1-04 | 旧创建但近期活动 Session | 进入候选；新导入旧活动不进入 |
| M35-V04 | L1-05 | 离线错过多个水位 | 只排队最新已到期目标 |
| M35-V05 | L1-06 | content fingerprint 不变 | Agent 调用 0 次，新 Snapshot 为 reused |
| M35-V06 | L1-06 | Session 退出窗口或 carry 到期 | fingerprint 改变并重新生成 |
| M35-V07 | L1-07 | 项目有工作无建议 | 0 条建议和明确空文案 |
| M35-V08 | L1-07 | Agent 返回 4 条建议/重复 rank | 输出准入失败，不截成成功 |
| M35-V09 | L1-08 | active 跨窗口 | 最长 7 天，reused 不延长 |
| M35-V10 | L1-09 | 完成事项 | 当前成功 Snapshot 展示一次，下一次退出 |
| M35-V11 | L1-10 | 上一 Snapshot 存在 | 输入使用结构化 Item，无 Markdown 读取 |
| M35-V12 | L1-11 | 长 Session 局部变化 | 首包只有变化事实/outline，可有界补读 |
| M35-V13 | L1-12 | 未归属 Session | 进入 unassigned，不产生 L2 candidate |
| M35-V14 | L2-03 | 用户明确项目决定 | 一次 generated Snapshot 后可候选 |
| M35-V15 | L2-04 | blocker 两次出现 | 两个有新证据 generated Snapshot 后候选 |
| M35-V16 | L2-04 | 中间只有 reused | 不增加也不打断观察 streak |
| M35-V17 | L2-05 | completion 重复 | 不晋升 L2 |
| M35-V18 | L2-06 | 无候选水位 | Project Agent 调用 0 次 |
| M35-V19 | L3-02 | 同一规则有两个项目证据 | 可形成 L3 candidate |
| M35-V20 | L3-02 | 两个路径实为同 project key | 只计一个项目 |
| M35-V21 | L3-04 | 删除已晋升来源 Session | L2/L3 保留，reference unavailable |
| M35-V22 | L3-05 | 后续证据推翻长期条目 | 新 revision current，旧 revision superseded |
| M35-V23 | L3-06 | Context/Markdown 读取 | 只含 current revision |
| M35-V24 | SKILL-02 | 创建用户副本并修改 | 系统模板不变，新任务绑定用户 asset |
| M35-V25 | SKILL-03 | 运行中修改 Skill | 旧任务不能覆盖新 fingerprint target |
| M35-V26 | SKILL-05 | Skill 请求网络/写库/额外工具 | capability 拒绝，无 Memory 发布 |
| M35-V27 | SKILL-06 | 设置指向无效 Skill | Agent 调用 0 次、last-success 保留、无回退 |
| M35-V28 | PROJ-01 | 多项目成功生成 | tenant 目录只有两份新 Markdown |
| M35-V29 | PROJ-03 | 删除/损坏 Markdown | 从 SQLite 重建语义一致文件 |
| M35-V30 | PROJ-04 | Snapshot 超过 30 天 | 清理 Snapshot，长期 revision 保留 |
| M35-V31 | UI-01 | 打开 Recent | 默认日期轨道且日期内按项目 |
| M35-V32 | UI-02 | 切换按项目 | Item/reference identity 集合一致，Agent 0 调用 |
| M35-V33 | UI-03 | 点击 available Session | 进入对话记录详情并选中 Session |
| M35-V34 | UI-04 | 点击 unavailable reference | 无导航，显示不可用 |
| M35-V35 | UI-05 | DOM 审计 | 无刷新/窗口/编辑/反馈/深度回忆输入 |
| M35-V36 | UI-06 | 最新水位失败 | last-success + 更新未完成；详情在任务中心 |
| M35-V37 | AUTH-04 | 跨 tenant ID | 不可见或统一拒绝，无侧信道差异 |
| M35-V38 | OPS-02 | lease 过期/取消/晚到结果 | 单一终态，旧 token 不能提交 |
| M35-V39 | OPS-03 | 内部生成 | 无 CLI/AIWC 子进程，读取语义与外部一致 |
| M35-V40 | OPS-04 | 日志与 Task snapshot | 无 Prompt、正文、tool body、secret |

## 5. 测试层级

### Repository/State 单元测试

覆盖 CHECK、unique、transaction、revision、availability、Snapshot membership、retention。使用临时 SQLite，不 mock SQL。

### AppService 集成测试

覆盖 M35-V01–V30、V37–V40。Fake Agent 按 Work Order 返回确定性结构，可记录调用次数、purpose、Skill hash、工具白名单并模拟 barrier/timeout/late output。

### Contract 测试

覆盖 Engine registry、Tauri 映射、CLI generated schema、Skill manifest capability 和 camelCase DTO。连续生成两次 contract 必须一致。

### Frontend 测试

覆盖 M35-V31–V36、设置 normalization、PillTabs、展开状态、事件 + polling、路由与 DOM 负断言。组件不直接 mock SQLite 或重写时间算法。

### Desktop smoke

验证 Auroraqua-UI、日期轨道、项目卡片、窄窗口、键盘、Session 导航、任务中心、Skill 打开/恢复和 Markdown 文件只读表现。

## 6. Gate

### 每张切片

```bash
git status --short
git diff --check
cargo fmt --all -- --check
cargo test -p assetiweave <target-filter>
pnpm typecheck
pnpm test -- <target-test>
```

测试过滤器必须实际运行至少一个测试。

### 合同变化

```bash
ASSETIWEAVE_DB_PATH=/tmp/assetiweave-contract.sqlite pnpm cli:contract
cp cli/internal/schema/contract.json /tmp/assetiweave-memory-contract.json
ASSETIWEAVE_DB_PATH=/tmp/assetiweave-contract-2.sqlite pnpm cli:contract
cmp /tmp/assetiweave-memory-contract.json cli/internal/schema/contract.json
go vet -C cli ./...
go test -C cli -race ./...
pnpm check:surface-matrix
```

### 最终

```bash
git diff --check
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
go vet -C cli ./...
go test -C cli -race ./...
pnpm check:boundaries
pnpm test:boundaries
pnpm check:surface-matrix
```

## 7. 安全与隐私审计

- Skill、Conversation、Agent 输出和工具返回均视为不可信输入。
- ACP capability set 无网络、无协作 Agent、无递归 Memory、无文件/数据库写入。
- 引用在写入前验证 tenant 和完整 membership。
- source reference 保存 locator metadata，不复制无界正文。
- secrets 在 Agent 输入前和结果持久化前各执行一次 redaction。
- 错误、日志、TaskRuntime 和事件 payload 不含正文。
- 用户排除只阻止新生成/晋升；不删除 Conversation；已晋升长期知识按 spec 保留。

## 8. 可观察性

结构化 tracing 只记录：

- tenant 的不可逆 hash；
- job/snapshot ID；
- purpose、目标水位、窗口；
- candidate project/session/item/reference counts；
- input/content fingerprint 前缀；
- Skill asset ID/revision/hash 前缀；
- Agent/tool 调用次数、预算使用、耗时；
- publication kind、终态和错误码。

不记录 Skill 正文、Prompt、Conversation 内容、工具正文、生成正文或秘密。

建议指标：Job success/reuse/failure、Agent call count、coverage failure、projection failure、stale result、平均候选 Session 数和端到端耗时。指标用于诊断，不参与产品 Authority。

## 9. 发布门

- migration 从上一正式 schema 到最新版本成功；已发布 migration 字节不变；
- expand 期旧读路径仍可用，新路径有 last-success；
- backfill dry-run 输出数量与项目范围，执行前有数据库备份；
- v2 L1/L2/L3 和两份 Markdown 至少完成一次成功生成/投影后才能关闭旧写路径；
- contract 前仓库搜索、surface matrix 和运行测试证明无 active caller；
- 桌面 smoke 与全仓 Gate 无 P0/P1；
- 回滚步骤、数据保留和旧表处置记录在发布 Handoff。
