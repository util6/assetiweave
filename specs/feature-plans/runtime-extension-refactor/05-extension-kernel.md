# SPEC-05:Extension Kernel 共享底座(P1)

- 状态:Draft v3(v1 审计 #5/#13;v2 复审 #5/#12 修订)
- 前置:SPEC-01(RegistrySnapshot 宿主)、SPEC-03(LifecycleTask 走 TaskRuntime)
- 交付物:`backend/extension_kernel/` 模块;Conversation Adapter 与 Agent Market 两系统接入;领域 manifest 保持独立
- 关联 ADR:实施前落 `docs/decisions/ADR-009-extension-kernel-shared-primitives.md`

---

## 1. 目标与形态

把两套平行扩展系统的**重复基础设施**抽为一个共享底座;领域差异保持强类型、各归各:

```text
Extension Kernel(共享,一套)          Domain capabilities(各自,强类型)
├── PackageIdentity                    ├── ConversationAdapterManifest(card contract/NDJSON/source)
├── Compatibility                      ├── AgentPackageManifest(ACP 协议/模型发现/连接检查)
├── TrustGate(信任判定接口)            └── (预留 kind:skill-library、memory-extractor——仅注册名,不实现)
├── ProcessInvocation + ProbeSpec/ProbeResult(进程启动与探测)
├── RegistrySnapshot(原子替换)
├── LifecycleTask(安装/升级/probe/启停)
└── ExtensionError
```

**MUST NOT**:合并两个领域 manifest 为万能 schema;实现进程内热重载;在本轮实现任何新包 kind 的业务。

### 现状盘点(抽取来源)

| 能力 | Conversation Adapter(参考实现) | Agent Market(对齐对象) |
|---|---|---|
| 身份/版本 | `conversation-adapter.json`:`id`、`version`、`schema_version`、`protocol_version`;目录 `builtin-assets/adapters/<id>/`;远程 index `builtin-assets/index.json`(package_id/stable/beta/history) | `backend/agent_market/` + `builtin-assets/agent-market/`;`agent_installations` 表(migration `202608170001`) |
| 运行时启动 | manifest `runtime: {type: "node", entry, version}` | `AgentRuntimeManager` + `default_runtime_root`,进程启动(`backend/agents/process.rs`) |
| 信任 | `ConversationAdapterTrustState`(BuiltIn 等) | 安装预览/卸载预览(`AgentInstallPreview`/`AgentUninstallPreview`) |
| 注册表 | `application/conversation_script_catalog.rs`、`conversation_adapter_catalog_v2.rs`(catalog 拉取、安装、不可变运行版本) | `AgentRegistry`(atomic 持有)、`recover_startup`/`reload` |
| 生命周期任务 | `ConversationScriptInstallTaskSnapshot` 等 | AI 执行任务、市场刷新(`AgentMarketRefreshResult`) |

## 2. 共享类型规范

`backend/extension_kernel/`(`mod.rs`, `identity.rs`, `trust.rs`, `launcher.rs`, `registry.rs`, `lifecycle.rs`, `error.rs`):

```rust
/// 包身份。kind 决定该包由哪个领域子系统解释其 manifest。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub(crate) struct PackageIdentity {
    pub kind: PackageKind,            // ConversationAdapter | Agent(封闭枚举,新 kind 走 ADR)
    pub package_id: String,           // 反向域名式,如 "io.github.util6.codex-session"
    pub version: semver::Version,
}

#[derive(...)]
pub(crate) struct Compatibility {
    /// 该包声明的领域协议版本(adapter 的 protocol_version / agent 的协议版本)
    pub protocol_version: u32,
    /// 宿主核心可接受区间;不满足 → 拒绝安装/启用,错误 Incompatible
    pub core_requirement: Option<semver::VersionReq>,
}

/// 修订(审计 #13):现有 ConversationAdapterTrustState 为 BuiltIn/Trusted/Changed/Untrusted,
/// 其中 Changed(受信内容哈希漂移)是不可有损映射的安全状态。因此 kernel **不做统一枚举**;
/// 各领域保留自己的 trust 枚举与 DB 取值,kernel 只定义判定接口:
pub(crate) trait TrustGate: Send + Sync {
    fn can_enable(&self) -> bool;            // 是否允许启用/运行
    fn needs_confirmation(&self) -> bool;    // 启用/升级是否需用户确认
    fn integrity_changed(&self) -> bool;     // 内容与受信基线是否漂移(adapter 的 Changed)
}
// Conversation 与 Agent 各自为其 trust 状态实现 TrustGate;UI 与启用门禁只消费该接口。

/// 修订(v1 审计 #5 + v2 复审 #5):形状取现有 ConversationAdapterRuntime 与 Agent 执行
/// 需求的**超集**——运行时枚举 Node/Python/Bash/Executable、args、env、工作目录;
/// 新类型 MUST 无损表达全部现有合法 manifest 与 Agent 进程定义。
#[derive(...)]
pub(crate) struct ProcessInvocation {
    pub kind: RuntimeProgramKind,     // Node | Python | Bash | Executable(对齐 ConversationAdapterRuntimeKind)
    pub entry: String,                // 包内相对路径
    pub args: Vec<String>,
    pub env: Vec<EnvEntry>,           // Agent 执行所需(v2 复审 #5)
    pub working_dir: Option<PathBuf>,
    pub version_req: Option<String>,
    pub immutable_install_dir: PathBuf, // 不可变运行版本目录(沿用 adapter 现机制)
}
/// 探测契约与结果分离(v2 复审 #5:仅有结果类型承载不了"怎么探测")。
/// ProbeSpec 对齐现有 AgentCommandDefinition(命令覆盖/args,agents/types.rs 的
/// availability_probe)与 AiExecutionLimits 的超时、输出上限语义;
/// ProbeKind 区分 availability 与 model-discovery 两类探测。
pub(crate) struct ProbeSpec {
    pub program: Option<String>,      // 命令覆盖;None = 用 ProcessInvocation 的解析结果
    pub args: Vec<String>,
    pub env: Vec<EnvEntry>,
    pub timeout: Duration,
    pub output_limit: usize,
    pub kind: ProbeKind,              // Availability | ModelDiscovery
}
/// 探测结果(字段对齐 ConversationAdapterRuntimeStatus)。
pub(crate) struct ProbeResult {
    pub program: String, pub available: bool,
    pub version: Option<String>, pub required_version: Option<String>,
    pub error: Option<String>, pub hint: Option<String>,
}

/// 泛型注册表快照:锁外构建完整 T,ArcSwap 原子替换;读方拿 Arc<T> 零锁。
pub(crate) struct RegistrySnapshot<T> { inner: arc_swap::ArcSwap<T> }

/// 生命周期操作统一为 TaskRuntime 任务(SPEC-03)。
/// dedup 规则(修订,v2 复审 #12):资源键 = 完整 PackageIdentity(kind + package_id + version),
/// 请求键 = 资源键 + LifecycleOp;仅**完全相同的活动操作**去重返回既有快照;
/// 不同但互斥的操作(如 Install 进行中收到 Remove)按冲突矩阵返回 AppError::Conflict,
/// MUST NOT 返回另一操作的快照。冲突矩阵随实现落地为单元测试。
pub(crate) enum LifecycleOp { Install, Upgrade, Remove, Enable, Disable, Probe }

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExtensionError {
    #[error("...")] ManifestInvalid { package_id: String, reason: String },
    #[error("...")] Incompatible { package_id: String, need: String, have: String },
    #[error("...")] TrustRejected { package_id: String, state: String },
    #[error("...")] LaunchFailed { package_id: String, reason: String },
    #[error("...")] ProbeFailed { package_id: String, reason: String },
}
// 并入 AppError::Extension(SPEC-01):impl From<ExtensionError> for AppError。
```

领域侧接口(各领域实现,kernel 只认接口):

```rust
pub(crate) trait DomainPackageSystem: Send + Sync {
    fn kind(&self) -> PackageKind;
    /// 解析并校验领域 manifest;kernel 不理解其内容。
    fn inspect(&self, dir: &Path) -> Result<InspectedPackage, ExtensionError>;
    /// 领域侧安装后处理(登记 card contract / agent 能力等)。
    fn on_installed(&self, pkg: &InspectedPackage) -> Result<(), ExtensionError>;
    fn on_removed(&self, id: &PackageIdentity) -> Result<(), ExtensionError>;
}
```

## 3. 抽取式迁移步骤(行为等价是硬约束)

1. 落 kernel 类型与单测(纯新增)。
2. **Conversation Adapter 先接**(参考实现,机制最全):
   a. manifest 解析处产出 `PackageIdentity`/`Compatibility`/`ProcessInvocation` 与 `ProbeSpec`(字段来自现 `conversation-adapter.json`,不改文件格式;四种运行时与 args/env/探测语义 MUST 全部可表达);
   b. 为 `ConversationAdapterTrustState` 实现 `TrustGate`(DB 列值与枚举不变,仅加接口实现);
   c. 安装/升级任务改挂 TaskRuntime(SPEC-03 步骤 3 顺带);
   d. catalog 注册表改用 `RegistrySnapshot<ConversationAdapterCatalog>`。
   验收:adapter 安装、升级、probe、启停、同步全链路行为与基线一致(现有 `conversations/tests.rs` 不改断言通过)。
3. **Agent Market 对齐**:`AgentRuntimeManager` 内部注册表替换为 `RegistrySnapshot<AgentRegistry>`;安装/卸载预览与执行改走 LifecycleTask;`agent_installations` 表结构不动,代码层引入 `PackageIdentity`。验收:市场刷新、安装、连接检查、模型发现行为一致。
4. 清理两侧被替代的私有实现;`check:boundaries` 增加规则:领域系统 MUST NOT 绕过 kernel 自建注册表/安装流程(grep 关键私有符号清单,抽取完成时确定)。

## 4. 验收标准

- 两个领域系统的全部现有测试不改断言通过;`pnpm cli:contract` 无非预期 diff。
- 新增测试:kernel 单测(semver 兼容判定、`TrustGate` 判定覆盖含 `Changed` 在内的全部领域状态、`ProcessInvocation`/`ProbeSpec` 对四种运行时与 Agent availability-probe(命令覆盖/env/超时/输出上限)的无损表达、RegistrySnapshot 并发读写、LifecycleTask 去重与冲突矩阵);等价性测试(同一 adapter 包在改造前后 inspect 结果一致——用固定样本包)。
- 度量:`backend/agent_market/` + `application/conversation_script_catalog.rs` 合计行数下降(重复实现移除),记录在 PR 描述,不设硬指标。
- 文档:`specs/design.md` 增补 kernel 结构图(本 SPEC §1 图),并声明"新增市场型模块 = 新 PackageKind + DomainPackageSystem 实现 + seam,MUST NOT 新建垂直系统"。

## 5. 风险

- 最大风险是"抽取顺手改行为"。对策:每步 PR 附"行为等价声明",列出唯一允许的可见变化(如任务快照 kind 字段值);评审按声明核对。
- 远程 catalog 拉取(GitHub)网络行为不动,仍走现有实现,仅登记面收敛。
