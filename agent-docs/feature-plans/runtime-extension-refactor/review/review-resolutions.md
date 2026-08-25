# 审计处理记录(Review Resolutions)

- 对应审计:`review-comments.md`(v1,2026-08-18)与 `review-comments-v2.md`(v2 复审,2026-08-18)
- 处理日期:2026-08-18
- 处理结论:v1 的 15+1 条与 v2 的 8+5 条全部接受并落入分册;各轮留有选项的条目,裁决与理由记录如下。当前全套状态 **Draft v3**,待 Approved 候选复审。

## 第一轮(v1)处理记录

| # | 审计条目 | 处理 | 落点 |
|---|---|---|---|
| 1 | AppError 首步不可编译 | 重写为四步迁移:并存引入(别名不动)→ WireError 边界契约 → 逐模块迁移 → 最后翻转别名;记录 427 处直接 `Err(String)` 基线 | SPEC-01 §7 |
| 2 | 一次性 Engine 无法跨调用查任务 | 裁决**选项 3**:CLI 保持前台同步;不新增 Engine `task.*` 契约;daemon 另立 RFC;新增全局进程模型章 | SPEC-00 §3a、非目标 9;SPEC-03 全篇 |
| 3 | Shutdown 先杀 dispatcher | 采纳五阶段关闭:停准入 → dispatcher drain → 停消费者 → TaskRuntime shutdown → 关池 | SPEC-03 §4;SPEC-04 §5.4 |
| 4 | Outbox 缺跨进程所有权 | 裁决**指定宿主**:仅 ResidentHost 派发,OneShot 只追加;pull 兜底常设化;lease 留待 daemon RFC | SPEC-01 §4 角色差异;SPEC-04 §5;SPEC-07 §5 |
| 5 | RuntimeLauncher 表达力不足 | 改为 `ProcessInvocation`+`VersionProbe`,形状取现有 `ConversationAdapterRuntime` 超集(Node/Python/Bash/Executable + args + 探测) | SPEC-05 §2 |
| 6 | memory 旧键迁移断 Dream | 强制扇出 `"memory"` → `memory.extraction` **与** `memory.dream`;幂等;前端 schema 同步 | SPEC-06 A.2 规则 3 |
| 7 | 新 provider 无法构造 TargetProfile | `TargetProfile.app_kind` 改 `Option<AppKind>` + `target_provider_id` 必填列;`Source.origin_provider_id` 一并处理;含 DB/DTO 迁移 | SPEC-06 D.2 步骤 3 |
| 8 | 异步重建 vs 同事务位点 | 裁决 v1 **同步完成语义**:`handle` 返回 Ok 即业务效果已提交;委托任务的挂起协议明确为非 v1;新增 offset 未提前推进的回归测试 | SPEC-07 §1、§2 |
| 9 | canonical_method 判断反了 | **确认系本方核实错误**(统计脚本查错键名),基线更正:字段全填充、Go 读取、e2e 断言;删除废弃方案;canonical 作为矩阵聚合键 | SPEC-08 §1、§3;SPEC-00 事实表 |
| 10 | bootstrap 依赖将测试化的 Database | 签名改 async、仅收 `&SqlitePool`;阻塞物化经 `spawn_blocking`;SPEC-01 调用点同步更新 | SPEC-02 §3 步骤 2;SPEC-01 §4 步骤 5 |
| 11 | Projection 守卫弱于声明 | R4 扩为全量禁止清单(含 conversations/scanner/planner/executor/search/agents 等)+ 禁 `std::fs`/`tokio::fs`/`std::process`/adapters | SPEC-02 §3 步骤 5 |
| 12 | Retention 忽略缺失位点 | 注册与建 tenant 时初始化 offset 行;缺行视为 0;按 tenant 计算水位;三场景测试 | SPEC-04 §3 |
| 13 | TrustState 丢 `Changed` | 放弃统一枚举;领域 trust 枚举各自保留,kernel 只定义 `TrustGate` 判定接口(can_enable / needs_confirmation / integrity_changed) | SPEC-05 §2 |
| 14 | stale 粒度误伤 | 下钻到会话粒度:`changed_session_ids` 优先、超限回查 deltas,按 `record_kind + session_id` 关联;新增误伤回归测试 | SPEC-07 §3 |
| 15 | 占位消费者语义违规 | 删除该消费者;多游标隔离由 SPEC-04 §6 测试假消费者覆盖;dream_eligibility 延后至 Memory 重做且效果必须持久化 | SPEC-07 §4 |
| 补 | CommandMeta 第三份风险定义 | 采纳方案 2:Engine registry 保持唯一元数据源;新增层收缩为 `SurfaceMapping`(canonical ↔ Tauri),禁止再声明 risk/confirmation | SPEC-08 §2 |

## 方法论修订(源自本次审计的教训)

1. 分册修订两项强制检查(跨文档一致性、进程模型对账)已写入 SPEC-00 §10。
2. 本方在核实环节犯过一次"查错字段名得出反向结论"(#9)——再次佐证 SPEC-08 的原则:计数与字段状态以生成物和直接读取为准,不以记忆或二手转述为准。

## 第二轮(v2 复审)处理记录

13 条(8 P1 + 5 P2)全部接受;核实方式:#1/#3/#4/#5/#6/#7/#8 逐条在代码与 migration 中坐实,其余由文本对照确认。留白处裁决如下。

| # | 处理 | 落点 |
|---|---|---|
| 1 | 选方案 1:每个提交 revision 的事务(批次 + 收尾)各写一行事件,与"每批 bump revision"的现实同构;`bump_*_revision` 改为返回 revision;构造在应用层、追加在持有事务的 store 路径 | SPEC-04 §4;SPEC-02 事件追加边界注记 |
| 2 | notify 保留为同进程快路径;dispatcher 增低频轮询(2–5s 起步、空闲退避 ~30s)+ 跨进程消费集成测试 | SPEC-04 §5.2 |
| 3 | 承认 38 个 async command / 35 处 spawn_blocking 的现状;`Arc<AppRuntime>` clone 进 blocking 闭包;`block_on` 禁入 async 上下文;新增嵌套 Runtime 回归测试 | SPEC-01 §6 |
| 4 | seed 拆分:通用 seed 不含 adapter;基线 4 个调用点全部改"通用 seed → 物化 → 校验 → adapter seed"两段式并各补回归测试 | SPEC-01 §4 步骤 4-5;SPEC-02 §3 步骤 2 |
| 5 | `ProbeSpec`(契约:命令覆盖/args/env/timeout/output_limit/kind)与 `ProbeResult`(结果)分离;`ProcessInvocation` 补 env/working_dir | SPEC-05 §2 |
| 6 | 承认 profiles 为 `(tenant_id,id,payload)` JSON 存储;改为 Rust 启动迁移 + `#[serde(default)]` 推导 + 重持久化;旧 payload 升级/幂等/回滚测试;sources 仍走列迁移 | SPEC-06 D.2 步骤 3 |
| 7 | 旁表对齐真实证据主键:`(tenant_id, evidence_id, stale_since_revision)` + FK 级联删除;tenant_id 为主键首列 | SPEC-07 §3.1 |
| 8 | fallback 改按 `sync_run_id` 回查 deltas(含 tenant/record_kind);missing/restored 需迁移放宽 `change_kind` CHECK 并补写 delta(CHECK 这层为本方补充发现);事件载荷含 missing/restored | SPEC-07 §3.2;SPEC-04 §4.3 |
| 9 | 选"删除承诺"分支:v1 消费者同步执行、无内部任务;留 `SpawnOrigin::{External, Internal}` 扩展点注记 | SPEC-03 §4 阶段 2 |
| 10 | `.await.map_err(map_join_error)??`,区分任务取消与闭包 panic | SPEC-02 §3 示例 |
| 11 | 计数断言改为:bootstrap/池/migration/seed/recovery 各 1 次;生产 `Database::open_initialized` **0 次** | SPEC-01 §9 |
| 12 | 资源键 = 完整 PackageIdentity(kind+id+version),请求键含 LifecycleOp;同操作去重、互斥操作按冲突矩阵返回 Conflict | SPEC-05 §2 |
| 13 | 注册协议 `InitialPosition::{GenesisZero, BackfillThenCutoff}`;两个 v1 消费者的 backfill 即各自现成 pull 全量路径,cutoff = 注册时 max(seq) | SPEC-04 §3.5;SPEC-07 §1 |

方法论追加(已写入 SPEC-00 §10):(c) 存储模型对账——涉及表结构的断言必须先读 migration(#6/#7/#8 的共同根因);(d) 现状断言必须附可采样命令(#3 的根因)。
