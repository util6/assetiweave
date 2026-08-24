我最近看了一个非常热门的项目。 deepseek-ai/deepseek-harness,其设计非常精湛，我参考它的架构。DeepSeek Harness 建立在 Cordis 上，插件的 registrations 都属于 effects，插件卸载以后这些 effect 会 unwind，因此 hot reload 能成为整个架构的一等能力

看看能否优化本软件的架构，以下是我和GPT的一些讨论

> **AssetIWeave Core 决定“资产是什么、事实是什么、关系意味着什么”；Capability 决定“用什么方式获得、加工、消费这些资产”；Composition 决定“当前运行时选哪些能力、怎么组合”。**

这比简单地说“核心小一点、其他都插件化”更适合 AssetIWeave。

你现在仓库其实已经有基础：`backend/models` 承载 Asset / Conversation 等共享模型，`application` 负责工作流，`scanner/planner/executor/store/conversations` 各有职责；Conversation Adapter 又已经拥有独立源码、升级、probe、不可变运行版本机制。

------

# 一、我会给 AssetIWeave 留两个“不可插件化的核”

不是一个 Core，而是两个非常小的 Kernel：

```text
                 AssetIWeave
                      │
        ┌─────────────┴──────────────┐
        │                            │
   Domain Kernel              Extension Kernel
   资产世界的法律                插件世界的法律
        │                            │
        └─────────────┬──────────────┘
                      │
               Capability Layer
                      │
               Composition Layer
                      │
                   Plugins
```

## Domain Kernel：绝对稳定

它回答：

```text
什么是 Asset？
什么是 Conversation？
什么是 Source？
什么是 Artifact？
什么是 Message / Event？
什么叫来自哪里？
什么叫属于谁？
什么叫挂载？
什么叫版本？
什么叫同一个资产？
```

例如：

```text
AssetId
AssetType
Source
Provenance
Conversation
ConversationEvent
Artifact
Relation
Profile
MountRelation
```

以及它们之间的 invariant：

```text
Asset 必须有稳定 identity

Conversation 从 Codex 导入一次
和再次增量同步
不能变成两条 Conversation

删除 Source
≠ 删除真实文件

Mount
≠ Copy

Provider-specific metadata
不能污染 canonical identity

同一 Conversation
不能因为换了 parser
就变成另一条资产
```

这些东西**任何插件都不能重新定义**。

这就是你要从 Pi 学的 **Stable Domain Core**。

------

# 二、Extension Kernel 也不能插件化

这是我认为容易漏掉的一层。

如果你引入 DSH 的 composition 思想，那么必须有一个东西定义：

```text
Plugin 是什么？
Capability 是什么？
Provider 怎么注册？
Provider 怎么卸载？
谁依赖谁？
权限是什么？
生命周期是什么？
事件怎么订阅？
配置怎么加载？
版本兼容怎么算？
```

例如：

```rust
PluginManifest
CapabilityId
ProviderId
PluginContext
PluginLifecycle
Permission
EventBus
ServiceRegistry
```

这一层本质上相当于：

```text
DSH 的 Cordis
```

它本身当然不能又是插件。

所以真正不可替换的是：

```text
AssetIWeave

Domain Kernel
+
Extension Kernel
```

一个定义**数据世界规则**，一个定义**扩展世界规则**。

------

# 三、从这里往外，才应该大量 Capability 化

比如 Conversation。

我不会把：

```text
Conversation
```

插件化。

但是：

```text
“怎么得到 Conversation”
```

应该彻底 Capability 化。

于是：

```text
             ConversationSource
                     │
       ┌─────────────┼─────────────┐
       │             │             │
     Codex        Claude        OpenCode
   Provider      Provider       Provider
```

你现在 `builtin-assets/adapters/codex`、`claude-code`、`opencode`、`antigravity`、Web adapters 等，实际上已经非常接近这种 Provider 模型了。

Core 只认识：

```rust
trait ConversationSource {
    fn discover(...) -> ...
    fn sync(...) -> ...
}
```

它完全不知道：

```text
~/.codex/session...
~/.claude/...
OpenCode SQLite...
Chrome history...
```

------

# 四、Normalization 是一个非常关键的边界

这里尤其容易设计错。

我会拆成：

```text
Codex Native Data
       │
       │ Provider-specific
       ▼
Codex Adapter
       │
       │ canonicalization
       ▼
────────────────────────────
   Domain Boundary
────────────────────────────
       │
       ▼
Conversation
ConversationEvent
Artifact
ToolCall
FileChange
...
```

所以：

**输入怎么解释**：

```text
Plugin
```

**解释以后允许产生什么标准对象**：

```text
Core
```

例如 Codex 明天把：

```json
{"type":"tool_result"}
```

改成另一种格式。

Adapter 改。

但：

```text
ToolCall
ToolResult
```

作为 AssetIWeave canonical semantic 不应该跟着变。

这就是 Stable Domain Core 真正发挥价值的地方。

------

# 五、Memory 就是最典型的 Capability

我们刚才讨论 Memory 时，这条边界尤其漂亮。

我会设计成：

```text
                     Memory Capability
                            │
          ┌─────────────────┼─────────────────┐
          │                 │                 │
     Local Memory        Mem0            OpenViking
      Provider          Provider          Provider
```

但是这里还有第二层：

如果 AssetIWeave 最终要强调：

> Memory 也是“我的 AI 资产”。

那么：

```text
MemoryAsset
MemoryItem
Memory provenance
Memory relation
```

应该进入 **Domain Core**。

而：

```text
怎么提取 Memory
怎么 embedding
怎么搜索
怎么 rerank
怎么 injection
怎么 consolidation
怎么 forget
```

全部是 Capability。

于是：

```text
Conversation
      │
      ▼
MemoryExtractor Provider
      │
      ▼
───────────────────────
     Domain Boundary
───────────────────────
      │
      ▼
MemoryAsset
      │
      ├── Search Provider A
      ├── Search Provider B
      └── Injection Strategy C
```

这非常符合你项目的定位：

> **资产属于 AssetIWeave，算法不属于 AssetIWeave Core。**

这是我认为非常重要的一条边界。

------

# 六、Agent 更应该完全放 Core 外面

未来你接：

```text
ACP
Pi
OpenCode
Codex
Gemini CLI
DeepSeek Harness
```

Core 不应该知道它们。

而应该：

```text
                 Agent Capability
                        │
                    AgentService
                        │
       ┌────────────────┼────────────────┐
       │                │                │
 ACP Provider       Pi Provider      CLI Provider
       │
   Codex / Gemini
```

比如：

```rust
trait AgentProvider {
    fn run(...);
    fn resume(...);
    fn capabilities(...);
}
```

于是：

```text
翻译
总结
Memory extraction
Asset analysis
```

都可以依赖：

```text
AgentService
```

而不是：

```text
OpenCode
```

这是 DSH Capability Seam 特别值得你借的部分。

------

# 七、LLM、Translation 也一样

例如现在如果 Translation 是：

```text
Translation
    ↓
OpenCode
    ↓
某个模型
```

架构上绑定太深。

应该变成：

```text
             Translation Capability
                      │
               TranslationService
                      │
        ┌─────────────┼─────────────┐
        │             │             │
      Agent         LLM API       Local
    Provider        Provider      Provider
```

甚至 Translation 自己都可能不是基础 Capability，而只是：

```text
Action
   ↓
依赖 Agent Capability
```

这样：

```text
Translate
Summarize
ExtractMemory
Tag
Classify
```

都成为 composition 出来的产品功能。

------

# 八、Mount 我会“只插件化一半”

这是一个非常典型的边界案例。

你现在 AssetIWeave 最重要的规则之一是：

```text
Source
 → Catalog
 → MountRelation
 → Deployment Plan
 → Target
```

并且默认是单跳 symlink、不直接破坏 source、目标存在真实文件时有安全规则。README 现在已经明确把这些作为产品行为。

这些**不能交给插件决定**：

```text
什么叫 managed file
什么时候允许覆盖
什么叫 mount relation
如何保证 Source 不被修改
如何记录 deployment state
如何 rollback
```

这是 Core invariant。

但：

```text
Codex Skill 放在哪里？
Claude Skill 放在哪里？
Cursor Rule 是什么结构？
OpenCode target path 怎么算？
```

可以 Provider 化：

```text
               Target Capability

                       │

             TargetProfileProvider

        ┌──────────────┼──────────────┐
        │              │              │
      Codex          Claude         Cursor
```

于是：

```text
Core：
“安全地执行 DeploymentPlan”

Plugin：
“Codex 的目标是什么样”
```

这个边界非常干净。

------

# 九、Scanner 也应该拆成两半

你当前：

```text
backend/scanner
```

包含 Source 遍历、分类、description、Git metadata、hash 等职责。

未来我会变成：

```text
              Scan Engine
                 CORE
                   │
          file enumeration
          identity / hash
          provenance
                   │
            Detector Capability
                   │
       ┌───────────┼───────────┐
       │           │           │
 SkillDetector PromptDetector RuleDetector
```

也就是：

```text
怎么可靠扫描文件
怎么记录 provenance
怎么形成 Asset identity
```

Core。

而：

```text
“这个目录是不是 Skill？”
“Cursor Rule 长什么样？”
“Claude Command 怎么识别？”
```

Provider。

------

# 十、Store 我反而暂时不建议插件化

这是我和 DSH 差别最大的地方之一。

理论上你可以：

```text
Storage Capability
    ├── SQLite
    ├── Postgres
    └── Cloud
```

但我目前不会这么做。

因为 AssetIWeave 有一个极强的要求：

> **必须存在一个 canonical local source of truth。**

你当前明确规定 App SQLite schema 只能通过 SQLx migration 演进，前端和 CLI 也不能绕过 Rust 直接写数据库。

这其实是非常健康的。

所以短期：

```text
Canonical Catalog
      │
    SQLite
```

应该保持固定。

可以内部有：

```rust
AssetRepository
ConversationRepository
```

抽象。

但不要开放成第三方：

```text
Storage Plugin
```

否则插件甚至可能重新定义：

```text
transaction
identity
consistency
migration
ownership
```

等于把 Domain Core 打穿了。

未来真需要 cloud backend，再抽出来都来得及。

------

# 十一、Event Hook 要比 Capability 更弱

这里建议直接吸收 Pi。

例如：

```text
asset.discovered
asset.imported
asset.updated

conversation.discovered
conversation.synced

memory.created

mount.before
mount.after

agent.started
agent.completed
```

插件可以：

```text
listen
augment
trigger
observe
```

但默认不能：

```text
重新定义事实。
```

例如：

```text
conversation.synced
      ↓
Memory Plugin
      ↓
提取 Memory
```

很好。

但不要让：

```text
conversation.synced
      ↓
某插件偷偷修改 Conversation identity
```

这就越界了。

可以总结为：

```text
Capability：
“我负责完成这个能力。”

Event：
“发生了这个事情，你可以响应。”

Core：
“这个事情究竟意味着什么。”
```

这三者不要混。

------

# 十二、Composition 层真正应该负责的是“选择”，不是“定义”

例如未来：

```yaml
profile: coding

providers:
  conversation:
    - codex
    - claude
    - opencode

  agent:
    default: acp-codex

  memory:
    extractor: local-llm
    store: assetiweave
    retriever: sqlite-fts

  translation:
    provider: agent

  target:
    - codex
    - claude
```

Composition Runtime 做：

```text
找 Provider
检查 dependency
检查 permission
实例化
生命周期管理
选择 default
组合多个 Provider
```

但它不能决定：

```text
Conversation 长什么样。
```

这就是：

> **Composition controls implementation, not semantics.**

我认为可以把这句话直接当成你的架构原则。

------

# 对照你现在代码，我会这样划

| 当前模块                        | 未来定位                                         |
| ------------------------------- | ------------------------------------------------ |
| `backend/models`                | **Domain Core**                                  |
| `backend/store`                 | **Core Infrastructure，暂不开放插件**            |
| `backend/application`           | **Core orchestration**                           |
| `backend/scanner`               | Core scanner + Detector Providers                |
| `backend/planner`               | **Core invariant / planning**                    |
| `backend/executor`              | **Core safety / transaction**                    |
| `backend/conversations`         | Core sync/model + ConversationSource Capability  |
| `builtin-assets/adapters/*`     | **Provider Plugins**                             |
| `backend/capabilities`          | 应逐步演化为真正 Capability contracts / services |
| ACP / Pi / OpenCode             | Agent Providers                                  |
| Memory                          | Memory Capability                                |
| Translation                     | Action / Capability                              |
| MCP                             | Provider/Bridge                                  |
| UI Renderer                     | Resource Extension                               |
| Skill / Prompt / Rule detectors | Asset Detector Providers                         |

这里有一个需要特别指出的地方：

你现在目录已经叫：

```text
backend/capabilities/
```

但文档描述它目前包含 catalog、sources、profiles、groups、mounts、fs utils 等“可复用能力”。

**它和 DSH 所说的 Capability Seam 不是一个概念。**

以后如果真走这个架构，最好不要因为名字一样就直接把当前目录等同于：

```text
Service Definition / Provider
```

这是两个层次。

------

# 我最终会把 AssetIWeave 画成这样

```text
┌──────────────────────────────────────────────┐
│                 Product Layer                │
│                                              │
│ Conversation UI / Memory UI / Skill UI       │
│ Translation / Automation / Agent Actions     │
└──────────────────────┬───────────────────────┘
                       │
┌──────────────────────▼───────────────────────┐
│              Composition Runtime             │
│                                              │
│ Profiles / Provider Selection / Dependencies │
│ Config / Lifecycle / Permissions             │
└──────────────────────┬───────────────────────┘
                       │
┌──────────────────────▼───────────────────────┐
│               Capability Layer               │
│                                              │
│ ConversationSource   AgentProvider            │
│ MemoryProvider       LLMProvider              │
│ AssetDetector        TargetProvider           │
│ ExportProvider       SearchProvider           │
└──────────────────────┬───────────────────────┘
                       │
              Stable Capability API
                       │
┌──────────────────────▼───────────────────────┐
│                Domain Kernel                 │
│                                              │
│ Asset       Conversation      MemoryAsset     │
│ Source      Artifact          Provenance      │
│ Identity    Relation          MountRelation   │
│ Event       Version           Invariants      │
└──────────────────────┬───────────────────────┘
                       │
┌──────────────────────▼───────────────────────┐
│             Canonical Infrastructure         │
│                                              │
│ SQLite / Migration / Transaction             │
│ Safe Filesystem / Operation Log              │
└──────────────────────────────────────────────┘
```

而在旁边还有一个很小的：

```text
Extension Kernel
│
├── Plugin Manifest
├── Registry
├── Lifecycle
├── Dependency
├── Permission
├── Event Bus
└── Compatibility
```

它负责把上面的世界组装起来。

------

## 最重要的判断标准

以后每增加一个功能，可以问三个问题：

> **如果替换这个实现，会不会改变已有资产的含义？**
> 会 → Core；不会 → 倾向 Capability。

> **如果删掉这个功能，已有资产还能不能完整、无歧义地读取？**
> 不能 → Core；能 → 倾向 Plugin。

> **这个模块是在定义事实，还是在处理事实？**
> 定义事实 → Core；处理事实 → Capability。

例如：

```text
Conversation schema       → Core
Codex parser              → Plugin

MemoryAsset schema         → Core
Memory extraction          → Plugin

MountRelation              → Core
Codex target resolver      → Plugin

Asset identity             → Core
Skill detector             → Plugin

Plugin lifecycle           → Extension Kernel
Translation                → Plugin
ACP                        → Plugin
LLM                        → Plugin
```

所以我认为 AssetIWeave 最理想的边界不是 DSH 的 **Everything is a Plugin**，而是：

> **Everything that changes should be a Plugin; everything that gives assets stable meaning should not be.**

这句话基本可以作为整个下一代 AssetIWeave 架构的总原则。