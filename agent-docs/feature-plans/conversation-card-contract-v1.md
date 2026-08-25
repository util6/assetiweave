# Conversation Card Contract v1 实施规格

## 目标（Objective）

将 Conversation 内容卡片从全局固定的 `answer/tool/command/code/result` 集合，迁移为由每个 App adapter 声明语义类型、由 AssetIWeave Core 提供稳定渲染能力的扩展协议。

完成后：

- 新增语义卡片类型不需要修改 Rust 枚举、前端 JSX、全局设置 schema 或搜索白名单。
- 新增真正的渲染能力仍需要 Core/前端发布，并接受安全与兼容审查。
- 旧 adapter、旧数据库及旧 `metadata_json.content_card` 继续工作。
- 搜索、导出、Memory 和 UI 消费同一份 Core 卡片投影。

## 假设与不变量（Assumptions and Invariants）

1. 一个 Conversation Part 最多投影一张内容卡片；需要多张卡片时，adapter 必须拆分为多个 Part。
2. `ConversationPartKind` 描述来源结构，Card `kind` 描述产品语义，两者不得合并。
3. Card `kind` 是开放且稳定的标识符；renderer 是 Core 支持的受控集合。
4. Adapter 不得注入 React 组件、HTML、CSS、脚本或任意 renderer 实现。
5. Parser 输出可重建；译文、注释和其他用户派生数据不可因重解析而丢失。
6. 不建立 `legacy/new/v2` 平行目录；新契约在现有 `backend/conversations`、DTO、store 和前端 Conversation 模块内逐步替换旧路径。

## 契约定义（Contract）

### Adapter Manifest 卡片类型声明

Manifest 可以通过可选字段声明该 App 使用的语义类型：

```json
{
  "card_contract_version": 1,
  "card_kinds": [
    {
      "id": "claude-code.reasoning",
      "semantic_role": "reasoning",
      "label": "Reasoning",
      "default_renderer": "markdown",
      "allowed_renderers": ["markdown"],
      "icon_hint": "brain"
    }
  ]
}
```

规则：

- `id` 必须是稳定、小写、可持久化的 `<adapter-id>.<kind>` 命名空间标识符；旧五类内置 kind 仅由兼容层读取。
- 跨 App 聚合使用可选 `semantic_role`（例如 `reasoning`），不能用无命名空间 kind 代替。
- `allowed_renderers` 限制 Part descriptor 可覆盖的 renderer；未覆盖时使用 `default_renderer`。
- 改名通过 alias/canonicalization 迁移，不直接改变已持久化 ID。
- label 和 icon 只是显示提示，不能包含 HTML/CSS。

### 规范化 Part 卡片描述符（Normalized Part Card Descriptor）

```json
{
  "role": "assistant",
  "kind": "text",
  "text": "分析过程……",
  "content_card": {
    "schema_version": 1,
    "kind": "claude-code.reasoning",
    "renderer": "markdown"
  },
  "metadata_json": {
    "source_type": "thinking"
  }
}
```

首批 renderer：

- `markdown`
- `plain`
- `json`
- `code`
- `command`
- `terminal_output`

旧字段兼容：

```text
content_card.type   -> content_card.kind
content_card.format -> content_card.renderer
contentCard         -> content_card
```

新字段与旧字段同时存在时，Core 必须验证两者语义一致；冲突时拒绝同步，不能静默选择其中之一。

### Core 输出投影（Core Output Projection）

所有内部消费者使用结构化输出，而不重新解析 `metadata_json`：

```json
{
  "card_id": "conversation-part-...",
  "part_id": "conversation-part-...",
  "adapter_id": "claude-code",
  "kind": "claude-code.reasoning",
  "semantic_role": "reasoning",
  "renderer": "markdown",
  "role": "assistant",
  "body": "分析过程……",
  "language": null,
  "cwd": null,
  "status": null,
  "exit_code": null,
  "translated_body": null,
  "legacy_anchor_ids": ["conversation-part-...-reasoning"]
}
```

`card_id` 恒等于稳定的 `part_id`，不包含 kind、renderer 或旧 suffix；重新分类不得改变 Card 身份。旧 `${part_id}-${type}`/suffix 仅作为 `legacy_anchor_ids` 深链接兼容信息。

## 模块边界（Module Boundaries）

```text
App 原始数据源
  -> adapter source parser
  -> 外部协议校验 (external protocol validation)
  -> backend/conversations card 规范化器
  -> 规范化 Session/Turn/Part
  -> SQLite
  -> 单一 Card 投影
      -> Tauri/Engine DTO
      -> 搜索/索引/聚合分面 (search/index/facets)
      -> Markdown 导出
      -> Memory
      -> 前端渲染器注册表 (frontend renderer registry)
```

推荐落位：

- `src-tauri/src/backend/models/conversation_card.rs`：仅放置契约/值对象类型。
- `src-tauri/src/backend/conversations/cards.rs`：负责校验、遗留兼容与投影逻辑。
- `frontend/src/components/conversations/`：渲染器注册表及渲染器组件。
- 现有 Repository 直接调用 Core 投影；严禁自行实现卡片元数据解析器。

## 校验与安全性（Validation and Security）

外部 adapter 输出属于不可信数据，即便其可执行程序是受信任的。

- 在外部 adapter 边界严格校验描述符形态、schema 版本、kind 标识符、命名空间归属、renderer 以及可选字段长度。
- 新产生的未声明 kind 视为一致性契约错误（conformance error）。
- 不支持的 renderer 在包校验/运行时门禁阶段直接拒绝；`min_core_version` 必须保护较新的 renderer。
- 历史数据库中包含未知 kind 的旧数据行通过通用的安全渲染器展示，并生成诊断信息。
- 渲染内容保持 React 原生转义；adapter 无法请求渲染原始 HTML。
- 卡片元数据大小必须保持在现有 adapter 输出尺寸限制之内。

## 持久化与派生数据（Persistence and Derived Data）

迁移过程采用增量式“规范写入/双向读取”（Canonical-Write/Dual-Read）机制：

1. Core 投影初期直接读取旧版元数据，无需改动数据库 Schema。
2. 仅在投影行为验证无误后，才新增可空的规范 `content_card_json` 列；`conversation_adapters` 同步持久化 `card_kinds_json`。
3. Card Contract v1 Adapter 的新导入仅写入规范 `content_card_json`；普通来源 metadata 继续保留，但不再复制 Card 描述符。历史 Adapter 与历史数据库仍走 legacy metadata 回退；不执行危险的一次性 JSON SQL 回填。
4. 读取优先采用规范数据，缺失时回退到旧版元数据。
5. 在所有消费方和第一方 adapter 完成迁移至少两个发布周期前，不得删除旧字段。

Adapter 升级必须参与 Hydration 身份计算。观测状态必须明确区分：

- 源文件版本（source version）；
- Adapter 内容哈希（adapter content hash）；
- Core 卡片契约版本（Core card contract version）；
- 脏状态/需重解析状态（dirty/reparse state）。

Rehydration 必须完整保留用户派生数据。短期内，通过 upsert 维持稳定 Part ID，当源文本哈希不变时译文得以保留。长期来看，译文将移出可替换的 Part 行，转入带 `source_body_hash` 的专用派生表，从而能够准确识别过期的译文。

## 消费方迁移（Consumer Migration）

### 前端（Frontend）

- 按 `renderer` 进行渲染，而非按语义 `kind` 渲染。
- 已知 kind 使用国际化标签/图标；自定义 kind 使用 manifest 定义或安全的人性化回退展示。
- 颜色配置迁移为经过校验的 `Record<string, string>`，附带内置默认值与通用回退。
- 可见性过滤器基于当前作用域中实际存在的卡片动态派生，而非硬编码的全局列表。

### 搜索（Search）

- 搜索 Card kind 改为经过校验的字符串/Newtype，而非封闭的静态枚举。
- Question 文档保持为独立的搜索文档变体，不再伪装成一种 Card kind。
- 聚合分面（Facets）从已索引/已注册的 kind 动态派生。
- 迁移期间现有的字符串过滤器和 Block ID 保持兼容。

### 导出与 Memory（Export and Memory）

- Core 拥有默认的 Markdown 导出器，并渲染有序的 Card 流。
- Adapter 的 `export_markdown` 仅作为临时的兼容覆盖选项。
- 新的 Memory/搜索逻辑消费通用的有序 Card 流，而不是依赖 `answer_text/code_text/command_text`。
- 旧版聚合列持续维护，直至其活跃消费方计数归零。

## 测试策略（Testing Strategy）

### 单元测试（Unit）

- 正确解析全部旧版五类 kind 和对应格式。
- 接受包含支持 renderer 的自定义 kind。
- 完整保留 `json` 数据格式，不将其降级为 Markdown。
- 拒绝无效标识符、冲突的新旧描述符以及不支持的 renderer。
- 对未知的历史 renderer/kind 数据安全回退至 Plain 渲染。

### 集成测试（Integration）

- 覆盖完整的 Adapter -> 协议 -> 规范化 Part -> Repository -> DTO 投影链路。
- 验证 Session 与 Web 记录 Repository 生成完全一致的 Card 语义。
- 搜索能够正确索引、检索自定义 kind 并返回动态分面。
- 当 Parser/Adapter 哈希变化时，能够对未变更的源会话进行重新 Hydration。
- 文本内容未变时的重新分类能够正确保留原有译文。

### 前端测试（Frontend）

- 未知的语义 kind 能够根据其声明的 renderer 和通用展示元数据正常渲染。
- 过滤器与颜色配置无需修改全局类型定义即可正常工作。
- 现有的五种内置 kind 保持原有的视觉呈现；新 DOM ID 使用稳定的 `card_id`，同时兼容旧的深度链接 ID。

### 一致性探针（Conformance Probe）

增加一个测试夹具类型（如带有 `semantic_role: reasoning` 的 `claude-code.reasoning`）。端到端支持该类型必须不需要修改 Rust kind 枚举、前端 kind 联合类型或硬编码的搜索列表。

## 验证命令

```bash
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
pnpm conversation-adapters:check
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
```

仅在 Engine 暴露的类型或方法发生变化时运行 `pnpm cli:contract`；严禁手动编辑生成的契约文件。

## 分阶段实施任务（Phased Tasks）

### 阶段 1：契约与 Core 投影（Contract and Core Projection）

- 优先编写契约测试与遗留兼容测试。
- 引入统一的 Rust 卡片投影模块。
- 消除 Repository 与搜索索引中的重复解析逻辑，保持外部可见行为不变。

### 阶段 2：增量式 Adapter 协议（Additive Adapter Protocol）

- 新增可选的结构化 `content_card` 与 manifest `card_kinds`。
- 在 `try-run`、注册与同步期间校验卡片。
- 保持对旧版元数据的双向读取。

### 阶段 3：结构化 DTO 与渲染器注册表（Structured DTO and Renderer Registry）

- 在 Conversation 详情 DTO 中以增量方式暴露投影后的 Card。
- 将前端渲染迁移为带有遗留回退的渲染器注册表。
- 将颜色与可见性配置转为动态 Map。

### 阶段 4：动态消费方改造（Dynamic Consumers）

- 迁移搜索类型过滤器与聚合分面。
- 将默认 Markdown 导出逻辑迁移至 Core。
- 将 Memory 消费逻辑迁移至有序 Card 流。

### 阶段 5：持久化与 Hydration（Persistence and Hydration）

- 新增规范卡片持久化列并提供数据回填。
- 增加感知 Adapter 版本的 Hydration/重解析状态。
- 保护译文及其他用户派生记录。

### 阶段 6：第一方迁移与旧逻辑退役（First-Party Migration and Retirement）

- 更新第一方 adapter，并从单一事实来源生成打包副本。
- 统计旧版读取频次，仅在活跃使用归零后才移除重复的解析器/导出器。
- 每次同步在结果与 `conversation.sync` 操作日志中聚合 `legacy_cards_upgraded`；每次 Session/Web/dry-run 导出在结果与 `conversation.export` 操作日志中记录 `legacy_adapter_exporter_used`。
- 旧 `content_types` 参数、元数据双读、历史锚点和 legacy exporter 至少保留两个发布周期；仅在随后连续一个完整发布周期聚合计数为零时退役。历史数据库读取兼容层等待独立数据迁移，不随代码清理删除。

## 成功标准（Success Criteria）

- 新增 `claude-code.reasoning + semantic_role=reasoning + markdown` 仅需修改 adapter manifest/parser 和测试用例。
- 新增 renderer 仅需修改统一的渲染器注册表路径，并声明兼容的最低 Core 版本。
- 未知的历史卡片内容绝不静默丢失。
- 格式错误的新描述符在 adapter 边界被拦截并返回精确的错误信息。
- 搜索、导出、Memory 与 UI 中不包含任何针对五类卡片的硬编码白名单。
- Adapter 代码变更能够自动重新解析历史记录，即使源文件内容未变。
- 卡片重新分类不会丢失已有的有效译文。
- 现有的 Adapter、数据库、Block ID 以及五种内置卡片行为完全保持兼容。

## 架构红线与原则（Boundaries）

- **必须遵守**：行为变更前必须有契约测试；采用增量迁移；使用参数化 SQL；严格进行外部输入校验；批处理后只执行一次统一的搜索/目录刷新。
- **需先沟通**：引入新依赖；执行任意插件 UI 代码；删除遗留字段；破坏性数据迁移。
- **严禁发生**：创建平行的 `v2` 实现目录树；手工编辑生成的契约文件；静默丢弃未知 kind；在解析器刷新时误删用户派生数据。
