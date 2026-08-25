# SPEC-06:能力 Seam——ActionId/策略类、中立 Availability、Detector、TargetProvider(P2)

- 状态:Draft v3(v1 审计 #6/#7;v2 复审 #6 修订)
- 前置:SPEC-01;§4(TargetProvider)的 descriptor 分发依赖 SPEC-05 仅在"打包为内置资产"层面,数据文件先行不受阻
- 本篇含四个相互独立的子规范,可各自成 PR 流

Seam 三角色总表(全篇约束:Consumer MUST 只依赖 Definition,MUST NOT import Provider):

| Seam | Definition(接口) | Provider(实现) | Consumer(使用方) |
|---|---|---|---|
| AgentProvider | `AgentExecutionRuntime`(已存在) | ACP backends、native | 翻译、Memory 提取/Dream、未来打标/摘要 |
| ConversationProvider | adapter 协议(已存在) | 各 adapter 包 | 同步、卡片、搜索 |
| TargetProvider | `TargetProfileDescriptor`(本篇新增) | descriptor 数据文件 | planner/executor/defaults |
| AssetDetector | `AssetDetector` trait(本篇新增) | 内置 detectors | scanner 引擎 |

---

## A. ActionId 与 ExecutionPolicyClass

### A.1 现状

- `configured_agent_capability(service_id: &str)`(`backend/ai_execution/mod.rs`)直接翻 settings JSON(`aiRuntime.cli`、`agentCapabilityAssignments`),硬编码 `"gemini"`/`"opencode"` 回退,存在 `legacy_gemini.rs`。
- `AiExecutionPurpose`(`ai_execution/types.rs`)= {Translation, ConnectionTest, ModelDiscovery},把"做什么"与"按什么策略执行"混在一个枚举。

### A.2 规范

```rust
/// 能力消费点标识。开放集合(String newtype),注册表控制合法值。
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ActionId(String);   // "translation" | "memory.extraction" | "memory.dream" | ...

/// 执行策略类。封闭枚举,Core 定义;决定超时/并发/预算档位。
pub(crate) enum ExecutionPolicyClass {
    InteractiveAssist,     // 用户在等:短超时、可取消、单并发/会话
    BatchTransform,        // 批处理:中超时、限并发、进度上报
    BackgroundAnalysis,    // 后台:长超时、低优先、可推迟
    Diagnostics,           // 连接测试/模型发现:极短超时
}

pub(crate) struct ActionRegistration {
    pub id: ActionId,
    pub policy: ExecutionPolicyClass,
    pub description: &'static str,
}
/// 编译期注册表:actions() -> &'static [ActionRegistration]。
```

规则(MUST):
1. **未知 ActionId 关闭失败**:`resolve_action(id)` 查无注册 → `AppError::Validation`,不回退、不猜测。
2. `AiExecutionPurpose` 迁移:Translation → ActionId("translation") + InteractiveAssist;ConnectionTest/ModelDiscovery → Diagnostics 类的保留 action。类型保留一版做兼容包装后删除。
3. **settings 迁移(修订,审计 #6)**:启动时(bootstrap)读 `agentCapabilityAssignments`,按映射表规范化;旧键**扇出**是强制的——`"memory"` 同时被 `memory_extraction.rs` 与 `memory_dream.rs` 消费,MUST 扇出为 `"memory.extraction"` **与** `"memory.dream"` 两条,否则现有用户的 Auto-Dream 断链;此后用户对任一新键的显式配置覆盖迁移值。前端 settings schema 同步扩展为分键配置。无法识别的键 → 该分配置为 disabled 并写一条 operation log 警告,MUST NOT panic、MUST NOT 静默沿用。`aiRuntime.cli` legacy 回退逻辑与 `legacy_gemini.rs` 在迁移落地后删除。迁移幂等:重复启动不重复扇出(以新键已存在为准)。
4. 组合解析集中一处:`backend/ai_execution/composition.rs` 的 `resolve_agent_for(action: &ActionId) -> Result<(AgentId, Option<String>), AppError>`,是 `configured_agent_capability` 的替代者;全部消费者(含 `memory_extraction.rs`、`memory_dream.rs` 的两个现调用点)改调它。

## B. Provider 中立 Availability API

### B.1 现状

`check_opencode_translation_availability(runtime)`(`backend/card_translation.rs`,经 `application/card_translation.rs` 透传)——函数名与语义绑定 OpenCode。

### B.2 规范

```rust
/// 面向 action 的可用性检查:解析组合配置 → 检查被指派 agent 的安装/连接。
pub(crate) fn check_action_availability(
    runtime: &dyn AgentExecutionRuntime, action: &ActionId,
) -> ActionAvailability;   // { available, agent_id, installed, version, error }
```

步骤:新函数落地 → 调用方(翻译前端命令、Engine method)切换 → 旧函数标记 `#[deprecated]` 一个版本 → 删除。对外 DTO 字段名去 provider 化(`opencode_*` 字段若存在于契约,新增中立字段并保留旧字段一版,契约再生)。验收:`grep -rn "opencode" src-tauri/src/backend/card_translation.rs src-tauri/src/backend/application/card_translation.rs` 为空(测试样本除外)。

## C. AssetDetector 编译期注册表

### C.1 现状

`scanner/classifier.rs` 的 `classify_asset` 为硬编码 `lower.contains(...)` 串;`dispatcher.rs` 已有私有 `AssetScanner` trait(Skill/Mixed 两实现)。

### C.2 规范

```rust
pub(crate) struct DetectionCtx<'a> {
    pub source: &'a Source, pub path: &'a Path,
    pub relative_path: &'a str, pub format: AssetFormat,
}
pub(crate) struct Detection { pub kind: AssetKind, pub confidence: u8 /*0-100*/ }

pub(crate) trait AssetDetector: Send + Sync {
    fn id(&self) -> &'static str;         // "builtin.prompt" 等
    fn version(&self) -> u32;             // 规则变更时递增
    fn priority(&self) -> i32;            // 高者先
    fn detect(&self, ctx: &DetectionCtx) -> Option<Detection>;
}
```

裁决规则(MUST,写进引擎并测试):
1. 稳定序:priority desc → confidence desc → id asc;同输入 MUST 同输出(重扫稳定性)。
2. 全部未命中 → 沿用现有回退链(source.default_kind → md=Custom → Unclassified),回退链留在引擎,不做 detector。
3. Provenance:资产记录 `detector_id` 与 `detector_version`(assets 表加两列,迁移文件;历史行回填 `"legacy.classifier"/1`)。
4. v1 为**编译期注册表**(静态 slice),无卸载语义;外部 provider 化留待后续 ADR,本轮 MUST NOT 实现。
5. 现有 classifier 规则逐条转为内置 detectors(prompt/rule/memory/agent/workflow/command/mcp 各一个,priority 按现 if 顺序递减),行为等价:**用现仓库真实源目录构造快照测试,改造前后分类结果全等**。

## D. TargetProviderId 与 TargetProfileDescriptor

### D.1 现状

`AppKind` 12 变体(`models/assets.rs`);`app_paths.rs::default_skill_target` 15 处 match;`defaults.rs::default_profiles` 12 元组硬编码。新增一个目标 App 需改核心枚举 + 两张硬编码表。

### D.2 规范

```rust
/// 开放目标标识。与 AppKind 并存;AppKind 保留为兼容分类,MUST NOT 新增变体,
/// 也 MUST NOT 把新 App 塞进 AppKind::Custom。
pub(crate) struct TargetProviderId(String);   // "codex" | "claude" | ... | 未来任意

#[derive(Serialize, Deserialize, JsonSchema)]
pub(crate) struct TargetProfileDescriptor {
    pub id: String,                     // TargetProviderId 值
    pub name: String,
    pub app_kind_compat: Option<AppKind>,   // 旧数据桥;新 App 为 None
    pub default_targets: Vec<TargetPathRule>,   // { asset_kind, path } path 支持 ~ 与 @config 前缀(沿用 host_paths 解析)
    pub supported_kinds: Vec<AssetKind>,
    pub deployment_strategy: DeploymentStrategy,
    pub icon: Option<String>,
}
```

数据文件:`builtin-assets/targets/<id>.json`,启动时(bootstrap)加载进 `RegistrySnapshot<TargetCatalog>`;12 个内置 App 的现硬编码知识**逐字迁移**为 12 个 json。

边界(MUST):planner/executor 的不变量——单跳 symlink、source 只读、managed file 判定、覆盖安全规则、回滚——**留在 Core,不进 descriptor**。descriptor 只声明"路径长什么样",不声明"怎么安全执行"。

迁移步骤:
1. 类型 + 加载器 + 12 个 json(与硬编码表比对测试:`default_skill_target(kind)` 与 descriptor 输出全等)。
2. `defaults.rs::default_profiles` 改为从 catalog 生成;`app_paths.rs` match 收缩为兼容 shim(内部查 catalog),随后删除。
3. **模型迁移(修订,v1 审计 #7 + v2 复审 #6)**:现 `TargetProfile.app_kind` 为必填 `AppKind`(`models/assets.rs`),与"不新增变体、不用 Custom"两条约束叠加会让新 provider 在类型层面无法构造;且 profiles 表为 `(tenant_id, id, payload)`——`TargetProfile` 整体 JSON 序列化进 payload(`migrations/202606270002`),**不存在可加的列**。因此:
   - Rust 类型:`app_kind` 改 `Option<AppKind>`(兼容分类,新 provider 为 `None`),新增 `target_provider_id: String` 并加 `#[serde(default)]`,加载时缺省值由 `app_kind` 推导——历史 payload 反序列化 MUST 不失败;
   - **payload 迁移**:启动时 Rust 迁移(优先于 SQLite JSON1——payload 的反序列化规则本就在 Rust):加载旧 payload → 推导 `target_provider_id` → 重新持久化;MUST 幂等;
   - `Source.origin_provider_id` 按普通数据库列迁移(sources 为列式存储);
   - DTO/serde:`app_kind` 可空、旧字段名保留,前端与 CLI 容忍 null(契约再生);
   - 测试:旧 payload 样本升级测试、迁移中断后重启的幂等测试、回滚兼容测试(新写 payload 可被上一版本读取,新增字段被忽略)。
4. 验收:**新增一个虚构 App(测试内 json)全程不改任何 Rust 源码**即可出现在默认 profile、完成一次 symlink 挂载 e2e(临时目录)。

## 验收总表(四子规范)

- A:`grep -rn "configured_agent_capability" src-tauri/src` 仅剩定义与 deprecated 包装;未知 action 测试 `resolve_action_unknown_fails_closed`;settings 迁移测试(合法键规范化、非法键 disabled+日志)。
- B:provider 名 grep 归零;翻译可用性 UI/CLI 行为与基线一致。
- C:快照等价测试通过;assets 表 provenance 列回填完成;`scanner/tests.rs` 不改断言通过。
- D:比对测试 + 零代码新增 App e2e;27 处 match 消除(`grep -c "AppKind::" backend/app_paths.rs backend/defaults.rs` 归零或仅剩兼容 shim 声明处)。
- 全部:`pnpm cli:contract` 契约再生并提交;`check:boundaries` 增加"consumer 不得 import 具体 provider 模块"的 grep 规则(按各 seam 列出禁点)。
