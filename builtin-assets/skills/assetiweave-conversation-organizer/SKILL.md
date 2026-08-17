---
name: assetiweave-conversation-organizer
description: 通过 AssetIWeave CLI 整理或接入 AI App 对话记录时使用此 skill。适用于同步 session、检查 question group、执行 merge/split、导出 Markdown，以及通过外部 adapter 脚本接入新的 App。Codex、Claude Code、OpenCode 或外部适配器 session 都适用。
---

# AssetIWeave 对话记录整理

## 用途

当需要用 AssetIWeave 的 Conversation 模块整理对话记录时，使用此 skill。

本 skill 必须把 AssetIWeave CLI 当作事实来源。正常整理流程不要绕过 CLI 直接读取或修改第三方 session 数据库。开发新 adapter 时，只允许用只读方式分析来源数据结构，adapter 也必须只读来源数据；AssetIWeave 数据库只能由 CLI 经 Rust Engine 写入。

旧的 `codex-session-exporter` skill 继续保留为 Codex 专用兜底工具。本 skill 是基于 AssetIWeave 标准化对话记录的一般工作流。

## 核心规则

- 按 session-first 工作：先同步或列出 session，选中 session 后再检查其中的 question group。
- 把已导入的 turn 和 part 视为源对齐的不可变内容。
- 只用 merge/split 修改问题分组。
- 命令支持 dry-run 时，应用分组变更前先 dry-run。
- 不删除 session、turn、part 或第三方来源数据。
- 不让 AssetIWeave 调用外部 AI API。推理由当前 agent 完成，再通过 CLI 应用。
- 有用的 tool、command、code、subagent 上下文应保留在对应 question 下。
- 把本 Skill 和随软件发布的 adapter 视为 AssetIWeave 产品资产。只有用户明确要求产品开发时，才修改 Rust、Go、前端、内置枚举、默认 source/profile 或生成的 CLI contract。
- 普通用户扩展的新 App adapter 应进入 AssetIWeave 管理的 adapter library；需要随软件发布的官方 adapter 应通过明确的产品开发任务进入 `builtin-assets`。
- 如果现有 CLI、Engine adapter 协议或标准化模型无法表达来源数据，停止并报告具体能力缺口。只有用户另行明确授权产品开发任务后，才能修改 AssetIWeave 源码。

## CLI 工作流

### 1. 确认 CLI 和 schema

```bash
assetiweave-cli version
assetiweave-cli schema conversation.session.list
```

如果 CLI 不存在，或 schema 中没有 `conversation.*`，停止并说明需要先构建或安装 AssetIWeave。

### 2. 同步对话来源

如果用户要求谨慎，或来源是新加的，先 dry-run：

```bash
assetiweave-cli conversation sync --dry-run
assetiweave-cli conversation sync
```

同步单个来源：

```bash
assetiweave-cli conversation sync --source codex-live
```

### 3. 找到 session

```bash
assetiweave-cli conversation session list --query "<keyword>" --limit 20
assetiweave-cli conversation session get <session-id>
```

优先选择项目、标题、时间或用户描述主题匹配的 session。

### 4. 检查 question group

```bash
assetiweave-cli conversation question list <session-id> --limit 200
assetiweave-cli conversation question get <question-id>
```

重点检查：

- 应归入上一问题的短确认 turn
- `continue`、`继续`、`go ahead` 等续写提示
- 被误切开的命令、代码、工具输出
- 被过度合并的独立真实问题

### 5. 提出分组变更

执行前先明确说明计划操作：

- merge：按 session 顺序排列的相邻 question ID
- split：question ID，以及应成为新 question 起点的 turn ID

不要合并非相邻 question。不要在 question 的第一个 turn 之前 split。

### 6. Dry-run 并执行

合并：

```bash
assetiweave-cli conversation question merge <question-id-1> <question-id-2> --dry-run
assetiweave-cli conversation question merge <question-id-1> <question-id-2>
```

拆分：

```bash
assetiweave-cli conversation question split <question-id> --before-turn <turn-id> --dry-run
assetiweave-cli conversation question split <question-id> --before-turn <turn-id>
```

执行后重新读取 session：

```bash
assetiweave-cli conversation session get <session-id>
```

### 7. 导出 Markdown

每个 session 导出一个 Markdown 文件：

```bash
assetiweave-cli conversation session export <session-id> --output-root <directory>
```

如果用户给了目录，使用用户指定目录。未指定时，使用清晰命名的本地导出目录，例如 `~/Desktop/assetiweave-conversations`。

## 外部适配器说明

新增 App 是扩展配置任务，不是 AssetIWeave 产品开发任务。开始前记录产品仓库的 `git status --short`；完成后再次检查，结果不得因本工作流发生变化。

先只读检查新 App 的真实存储位置、schema 和少量结构样本。不要修改来源数据库。然后在产品仓库之外创建 adapter：

```bash
assetiweave-cli conversation adapter scaffold --directory <dir> --id <id> --name "<name>"
assetiweave-cli conversation adapter validate <manifest>
assetiweave-cli conversation adapter try-run <manifest> --method read_session --location <source> --yes
assetiweave-cli conversation adapter register <manifest> --yes
assetiweave-cli conversation source add \
  --id <source-id> \
  --adapter <adapter-id> \
  --name "<source-name>" \
  --kind <sqlite|file|directory|custom> \
  --location <source> \
  --dry-run
assetiweave-cli conversation source add \
  --id <source-id> \
  --adapter <adapter-id> \
  --name "<source-name>" \
  --kind <sqlite|file|directory|custom> \
  --location <source>
assetiweave-cli conversation sync --source <source-id> --dry-run
assetiweave-cli conversation sync --source <source-id>
```

外部 adapter 是可信可执行脚本。注册前检查 manifest 路径、命令和 hash。不要承诺操作系统级沙箱。

### ZCode

本 skill 已提供 ZCode adapter：

```text
scripts/zcode-conversation-adapter/conversation-adapter.json
scripts/zcode-conversation-adapter/adapter.mjs
```

它以 SQLite 只读模式读取 `~/.zcode/cli/db/db.sqlite`。使用相对本 `SKILL.md` 的真实绝对路径替换 `<skill-dir>`：

```bash
MANIFEST="<skill-dir>/scripts/zcode-conversation-adapter/conversation-adapter.json"
assetiweave-cli conversation adapter validate "$MANIFEST"
assetiweave-cli conversation adapter try-run "$MANIFEST" \
  --method read_session \
  --location ~/.zcode/cli/db/db.sqlite \
  --yes
assetiweave-cli conversation adapter register "$MANIFEST" --yes
assetiweave-cli conversation source add \
  --id zcode-live \
  --adapter zcode \
  --name "ZCode local sessions" \
  --kind sqlite \
  --location ~/.zcode/cli/db/db.sqlite \
  --dry-run
assetiweave-cli conversation source add \
  --id zcode-live \
  --adapter zcode \
  --name "ZCode local sessions" \
  --kind sqlite \
  --location ~/.zcode/cli/db/db.sqlite
assetiweave-cli conversation sync --source zcode-live --dry-run
assetiweave-cli conversation sync --source zcode-live
assetiweave-cli conversation session list --adapter zcode --limit 20
```

## 完成检查

- 相关来源已经同步，或已明确跳过。
- 已用具体 ID 识别 session。
- 合适时已预览 merge/split 操作。
- 已重新读取并检查最终 session 分组。
- 如果用户要求导出，已执行 session Markdown 导出命令并报告输出路径。
- 新 App 接入使用了外部 adapter 和 CLI source，没有修改 AssetIWeave 产品源码。
- 已确认来源数据库在 adapter 执行前后未被修改。
