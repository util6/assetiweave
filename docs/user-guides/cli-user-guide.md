# AssetIWeave CLI (`aiwc`) 完整用户指南与使用手册

`assetiweave-cli`（简称 `aiwc`）是 AssetIWeave 本地 AI 文件资产与对话记录管理器的命令行客户端。它通过轻量级 stdio JSON-RPC 协议直接与本地 Rust Engine 交互，与桌面端 (Tauri GUI) 共享相同的底层业务逻辑、SQLite 数据库以及挂载规则。

---

## 目录

1. [快速入门与安装配置](#1-快速入门与安装配置)
2. [快捷别名 (Aliases) 与短 Flag 速查](#2-快捷别名-aliases-与短-flag-速查)
3. [所有命令详情与使用方法](#3-所有命令详情与使用方法)
   - [3.1 系统概览、诊断与配置 (`overview`, `doctor`, `settings`, `version`)](#31-系统概览诊断与配置-overview-doctor-settings-version)
   - [3.2 资产源 (Source) 与资产 (Asset) 管理 (`source`, `asset`)](#32-资产源-source-与资产-asset-管理-source-asset)
   - [3.3 Skill 扩展管理与独占式控制 (`skill`)](#33-skill-扩展管理与独占式控制-skill)
   - [3.4 对话记录 (Conversation) 与网页采集器 (`conversation`, `harvester`)](#34-对话记录-conversation-与网页采集器-conversation-harvester)
   - [3.5 渐进式 Memory 与 Dream 语义记忆系统 (`memory`)](#35-渐进式-memory-与-dream-语义记忆系统-memory)
   - [3.6 强类型 App 命令与底层 API 调用 (`app`, `api call`, `schema`)](#36-强类型-app-命令与底层-api-调用-app-api-call-schema)
   - [3.7 自更新与版本维护 (`update`)](#37-自更新与版本维护-update)
4. [核心设计理念与底层架构](#4-核心设计理念与底层架构)
5. [Agent 自动化集成与安全策略](#5-agent-自动化集成与安全策略)

---

## 1. 快速入门与安装配置

### 1.1 编译与安装

在仓库根目录下，可以使用 `pnpm` 脚本快速编译与更新：

```bash
# 编译 Engine 与 CLI 并构建 Contract 模式
pnpm engine:build
pnpm cli:contract
pnpm cli:build

# 一键安装二进制文件到本地 PATH (构建 aiwc 与 assetiweave-cli)
pnpm cli:install

# 运行自动化 E2E 测试
pnpm cli:test:e2e
```

在开发测试阶段，也可通过代理脚本运行：
```bash
pnpm cli:run -- overview
```

### 1.2 关键环境变量表

| 环境变量 | 作用说明 |
| --- | --- |
| `ASSETIWEAVE_ENGINE` | 显式指定 `assetiweave-engine` 可执行文件的绝对路径 |
| `ASSETIWEAVE_DB_PATH` | 覆盖默认数据库路径，常用于沙盒隔离测试 |
| `ASSETIWEAVE_POLICY_PATH` | 指定 CLI 访问策略 JSON 文件路径，限制可运行的方法与风险等级 |
| `ASSETIWEAVE_CLI_PLUGIN_CONFIG` | 指定 CLI 插件配置文件路径 (默认 `~/.assetiweave-cli/plugins.json`) |
| `ASSETIWEAVE_CLI_HIDE_PROFILES` | 设置为 `1` 时隐藏 `profile` 相关的帮助输出与补全 (适合单 Profile 部署) |
| `ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER` | 设置为 `1` 时禁止在 JSON Envelope 中注入新版本更新提示 |
| `GITHUB_TOKEN` / `GH_TOKEN` | 用于 `skill search` 与 `skill acquire` 鉴权，突破 GitHub API 无鉴权速率限制 |

---

## 2. 快捷别名 (Aliases) 与短 Flag 速查

为提高命令行交互效率，CLI 提供了丰富的命令别名与单字母短 Flag：

### 2.1 常用顶层别名

| 标准命令 | 快捷别名 | 说明 |
| --- | --- | --- |
| `overview` | `ov`, `o` | 展现全局数据统计 |
| `source` | `src` | 资产源管理 |
| `skill` | `sk` | Skill 技能扩展管理 |
| `conversation` | `c`, `conv` | 对话记录与转译 Adapter 管理 |
| `memory` | `m`, `mem` | 渐进式语义 Memory 与 Dream 管理 |
| `harvester` | `hv` | 网页对话采集引擎管理 |
| `profile` | `p`, `prof` | 目标 Profile 挂载管理 |
| `tenant` | `t`, `tn` | 租户管理 |
| `version` | `v`, `ver` | 版本与 Engine 兼容性诊断 |

### 2.2 常用嵌套子命令缩写

- `list` -> `ls`
- `create` -> `cr`
- `get` -> `g`
- `search` -> `s`
- `update` / `upgrade` -> `up`
- `delete`/`remove` -> `rm`
- `preview` -> `pv`
- `acquire` -> `acq`

### 2.3 稳定通用短 Flag

- `-n`: `--name` (名称)
- `-p`: `--path` / `--plugin` (路径/插件)
- `-q`: `--query` (查询关键词)
- `-l`: `--limit` (数量限制)
- `-d`: `--dry-run` (试运行/预览)
- `-y`: `--yes` (高风险确认)
- `-C`: `--plugin-config` (插件配置路径)
- `-P`: `--policy` (安全策略路径)
- `-E`: `--engine` (引擎路径)

*示例*:
```bash
# 全称命令
assetiweave-cli source add --name LocalSkills --path ./skills --dry-run

# 快捷别名等价命令
aiwc src a -n LocalSkills -p ./skills -d
```

---

## 3. 所有命令详情与使用方法

### 3.1 系统概览、诊断与配置 (`overview`, `doctor`, `settings`, `version`)

#### 查看应用总览
显示当前 Catalog、源、Profile、Skill 以及 Memory 的状态概要：
```bash
aiwc overview
# 或使用短别名
aiwc ov
```

#### 系统环境诊断 (Doctor)
检查 CLI 与 Rust Engine 通信、SQLite 连通性以及版本匹配情况：
```bash
aiwc doctor
```

#### 版本检查
```bash
# 查看本地 CLI 及绑定的 Engine 版本
aiwc version

# 检查远端是否有新版本发布
aiwc version --check-updates
```

#### 应用配置查看与修改
```bash
# 查看当前配置
aiwc settings show

# 保存配置修改
aiwc settings save --json '{"density":"compact"}'
```

---

### 3.2 资产源 (Source) 与资产 (Asset) 管理 (`source`, `asset`)

资产源 (Source) 是资产的产生地（如本地 Skill 目录、代码仓库、文档库等）。

```bash
# 列出所有已登记的资产源
aiwc source list
# 缩写
aiwc src ls

# 试运行添加资产源 (预览效果)
aiwc source add --name "MySkills" --path ./skills --dry-run
# 缩写
aiwc src a -n "MySkills" -p ./skills -d

# 正式添加资产源
aiwc source add --name "MySkills" --path ./skills --yes
# 缩写
aiwc src a -n "MySkills" -p ./skills -y

# 手动触发指定类型的资产源扫描
aiwc source scan --kind skill

# 移除资产源登记
aiwc source remove --id <source-id> --yes
# 缩写
aiwc src rm --id <source-id> -y

# 浏览索引到的资产列表
aiwc asset list --kind skill
```

---

### 3.3 Skill 扩展管理与独占式控制 (`skill`)

Skill 是 AssetIWeave 的核心管理对象之一。CLI 支持本地 Skill 导入、网络远程搜索与获取、状态检查、挂载到指定目标 Profile 以及 Group 独占控制。

#### 本地 Skill 导入与备份
```bash
# 查看本地 Skill 列表
aiwc skill list
aiwc sk ls

# 导入本地 Skill 目录到 AssetIWeave 备份库
aiwc skill import --from ./downloaded-skill --name downloaded-skill

# 备份指定的 Skill
aiwc skill backup <asset-id>

# 删除备份库中的 Skill (附带取消挂载)
aiwc skill delete <asset-id> --unmount --yes
```

#### 远程 Skill 搜索与安全获取 (Acquire)
AssetIWeave 内置通过 GitHub API 的 Skill 搜索与获取链。**获取到的 remote skill 会被安全隔离放入库中，不会自动信任或执行**。

```bash
# 在 GitHub 上搜索带有 SKILL.md 的远程技能
aiwc skill search --query "browser automation skill" --provider github --limit 5
# 缩写
aiwc sk s -q "browser automation skill" --provider github -l 5

# 试运行获取远程 Skill (分析路径、生成安全提示，不写入文件)
aiwc skill acquire --url https://github.com/user/repo/tree/main/skills/my-skill --dry-run
# 缩写
aiwc sk acq -u https://github.com/user/repo/tree/main/skills/my-skill -d

# 正式获取远程 Skill 并写入本地库
aiwc skill acquire --url https://github.com/user/repo/tree/main/skills/my-skill --yes
# 缩写
aiwc sk acq -u https://github.com/user/repo/tree/main/skills/my-skill -y

# 列出所有远程获取的 Skill 及其远端 Git Hash
aiwc skill remote list

# 检查远程 Skill 是否有更新/漂移
aiwc skill remote check [asset-id]
```

#### Skill 挂载与解挂
将 Skill 软链接挂载到指定的 AI 工具目标配置（如 Codex、Claude Code）中：

```bash
# 将 Skill 挂载到 codex 配置
aiwc skill mount my-skill --profile codex

# 从 codex 配置解挂 Skill
aiwc skill unmount my-skill --profile codex
```

#### Skill Group (分组) 与独占式挂载 (Exclusive Mount)
按目录或通配符将 Skill 分组，并支持**独占式挂载**（挂载该组的同时自动解挂冲突组）：

```bash
# 列出与查看 Skill 分组
aiwc skill group list
aiwc skill group show <group-id>

# 创建基于路径通配符的分组
aiwc skill group create --name Frontend --path-glob 'frontend/**'

# 显式将资产加入分组
aiwc skill group members set <group-id> --asset <asset-id>

# 批量挂载分组
aiwc skill group mount <group-id> --profile codex

# 预览独占式挂载（查看哪些旧 Skill 会被解挂）
aiwc skill group exclusive preview --group <group-id> --profile codex

# 正式应用独占式挂载
aiwc skill group exclusive apply --group <group-id> --profile codex --yes
```

---

### 3.4 对话记录 (Conversation) 与网页采集器 (`conversation`, `harvester`)

AssetIWeave 将各类本地 AI 工具对话（Session）以及网页 AI 对话（Web Record）统一归一化存储。

#### 3.4.1 浏览与导出 App 会话记录
```bash
# 浏览会话列表
aiwc conversation session list
# 缩写
aiwc c ses ls

# 获取具体 Session 及其 Turns
aiwc conversation session get <session-id>
# 缩写
aiwc c ses g <session-id>

# 查看单个 Block 块内容
aiwc conversation block get <block-id>
# 缩写
aiwc c block g <block-id>

# 跨 Session 搜索对话卡片
aiwc conversation search --query "login issue"
# 缩写
aiwc c s -q "login issue"
```

#### 3.4.2 更新与注册 Conversation Adapter

`~/.assetiweave/conversation-adapters/<cli-name>` 是用户可编辑的 Adapter
第一存储现场。`upgrade` 会先生成快照并执行 probe，成功后再将不可变运行副本
提升到 `packages`；失败时继续使用此前的可用版本。

每个 `<cli-name>` 目录至少包含 `conversation-adapter-package.json`、
`conversation-adapter.json` 和 manifest 声明的运行入口；目录名必须与 Adapter
`id` 相同，package 与 Adapter 的版本号也必须一致。

```bash
# 更新并注册默认目录下的全部 Adapter
aiwc c ad upgrade

# 开发者：使用当前代码仓库的 builtin-assets/adapters
aiwc c ad upgrade -d

# 更新并注册任意位置创建的单个 Adapter 目录
aiwc c ad upgrade ./path/to/my-app

# 只校验并预览提升位置
aiwc c ad upgrade ./path/to/my-app --dry-run

# 手动触发指定 Adapter 同步
aiwc conversation sync --adapter my-app
```

这里的 `-d` 固定表示 `--developer`，本命令的试运行使用 `--dry-run/-r`。

#### 3.4.3 Web Harvester 与网页 AI 对话
支持 ChatGPT、Qwen、Gemini 等网页 AI 的登录态管理与对话抓取。

```bash
# 网页采集器列表与诊断
aiwc harvester list
# 缩写
aiwc hv ls
aiwc harvester doctor <harvester-id>

# 检测与校验网页 Cookie 登录态
aiwc conversation web auth-detect ~/.assetiweave/harvesters/chatgpt-web --domain chatgpt.com --credential cookie
aiwc conversation web auth-check ~/.assetiweave/harvesters/chatgpt-web

# 查看导出的网页对话记录
aiwc conversation web-record list
```

---

### 3.5 渐进式 Memory 与 Dream 语义记忆系统 (`memory`)

AssetIWeave 具备可独立检索的本地语义记忆、增量归纳 (Dream) 以及基于证据的 Recall 系统。

```bash
# 查看本地 Memory 系统总览
aiwc memory overview
# 缩写
aiwc m ov

# 浏览与管理形式化记忆条目
aiwc memory item list

# 审查记忆候选集 (Candidates)
aiwc memory candidate list

# 触发增量 Dream 记忆归纳算法
aiwc memory dream run --yes

# 两阶段语义 Recall 检索（构建带溯源证据的上下文 Bundle）
aiwc memory recall bundle --query "如何在项目中处理跨域配置？"
# 预览当前项目的记忆 Recall
aiwc memory recall preview --query "数据库重构约定" --current-project
# 缩写
aiwc m rec pv -q "数据库重构约定" -c

# 验证特定 Memory 证据的时效性与关联源码状态
aiwc memory verify --id <memory-id>
```

---

### 3.6 强类型 App 命令与底层 API 调用 (`app`, `api call`, `schema`)

除了面向用户的快捷指令外，`assetiweave-cli` 提供了完全覆盖桌面端能力的双层 API 访问：

#### 3.6.1 自动生成的 App Typed 命令 (`app`)
根据 Rust Command Registry 声明的契约自动生成参数类型与标志：

```bash
# 列出当前配置 Profiles
aiwc app list-profiles

# 通过 JSON 文件输入参数创建 Profile
aiwc app create-profile --input @profile.json

# 删除数据源
aiwc app delete-source --id <source-id> --yes
```

#### 3.6.2 底层 API 显式调用 (`api call`)
可以像前端使用 `invoke` 一样，直接向 Engine 发送标准方法名与 JSON 参数：

```bash
# 裸调用 api
aiwc api call list_asset_mounts --json '{"assetId":null}'

# 发送复杂 JSON 结构
aiwc api call create_profile --json '{
  "input": {
    "id": "codex-test",
    "name": "Codex Test",
    "app_kind": "codex",
    "target_paths": ["/tmp/codex-skills"],
    "supported_kinds": ["skill"],
    "deployment_strategy": "symlink_to_source",
    "enabled": true
  }
}'
```

#### 3.6.3 查询 Engine 方法 Schema
```bash
# 列出所有可调用的 Engine 方法与参数结构
aiwc schema

# 查看具体方法的详细 JSON Schema
aiwc schema skill.import
```

---

### 3.7 自更新与版本维护 (`update`)

```bash
# 检查远端是否有新版本 CLI 发布
aiwc update --check

# 下载最新版本包、校验 SHA256 并替换安装
aiwc update --yes
```

---

## 4. 核心设计理念与底层架构

### 4.1 双二进制名称与等价执行
编译后系统提供两个可执行文件名，两者执行完全相同的代码逻辑：
- `assetiweave-cli`: 标准全称，适合脚本与规范文档。
- `aiwc`: 快捷简称（Asset I Weave Client），适合命令行交互与 Prompt 调用。

### 4.2 读写分离与状态共享架构

```text
AI Agent / 用户
  ├── 1. 快捷便利指令 (Shortcuts)   --> aiwc skill / source / conversation / memory / profile
  ├── 2. 强类型 App 命令 (App)     --> aiwc app <method> (依据 Rust Contract 自动生成)
  └── 3. 底层 API 裸调用 (API)      --> aiwc api call <method> --json '<params>'
                │
                ▼ (stdio JSON-RPC 协议)
        assetiweave-engine (Rust 核心引擎)
                │
                ▼
        SQLite / 文件系统 / Symlink 挂载目标
```

CLI 本身是轻量级的 Go 客户端，不直接读写 SQLite、复制 Skill 或修改软链接。所有的指令最终均提交给 Rust Engine 统一处理，确保 CLI 操作与桌面 GUI 的业务约束 100% 对齐。

### 4.3 Engine 解析优先级
CLI 启动时会按以下顺序定位后台 `assetiweave-engine` 引擎：
1. 环境变量 `ASSETIWEAVE_ENGINE` 指定的路径。
2. 系统 `PATH` 环境变量上的 `assetiweave-engine`。
3. 相对路径 `target/debug/assetiweave-engine`。

---

## 5. Agent 自动化集成与安全策略

### 5.1 JSON Response Envelope 规范

无论成功与否，所有 CLI 命令在 JSON 模式下均输出标准 Envelope 结构。AI Agent 在解析时应读取以下字段：

- **`status`**: `"ok"` 表示成功；`"error"` 表示失败。
- **`data`**: 命令执行结果负载。
- **`error`**: 失败时的错误详情，包括 `type`、`message`、`details` 以及 `hints`（修补建议列表）。
- **`meta.invocation`**: 调用的元数据，包括耗时 (`duration_ms`)、评估的风险等级 (`risk`)、执行的 Hook 等。
- **`_notice.update`**（可选）: 当检查到远端有新的 CLI 版本可升级时，自动注入的更新提示信息。

### 5.2 Agent 友好防护设计

1. **写操作预览 (`--dry-run` / `-d`)**:
   所有带有修改/写入性质的命令均支持 `--dry-run`，可提前输出计划执行的操作而不会真实修改文件或数据库。
2. **破坏性防误触 (`--yes` / `-y`)**:
   高风险或删除类操作必须带上 `--yes` 参数确认，否则会终止执行并返回提示。
3. **固定自动化退出码 (Exit Codes)**:

   | 退出码 | 含义 |
   | --- | --- |
   | `0` | **Success**: 执行成功 |
   | `2` | **Invalid Parameters**: CLI 或 Engine 参数校验失败 / 命令行语法错误 |
   | `3` | **Engine Failure**: Engine 进程失败、协议不匹配或业务执行错误 |
   | `5` | **Internal Failure**: CLI 内部错误 |
   | `6` | **Command Denied**: 被局部策略文件拒绝对该命令的执行 |
   | `10` | **Confirmation Required**: 高风险操作缺少 `--yes` 显式确认 |

### 5.3 局部安全策略控制 (`ASSETIWEAVE_POLICY_PATH`)

在为 Agent 配置沙盒运行环境时，可以通过 JSON 策略文件精确限制 CLI 的操作权限：

创建 `policy.json` 示例：
```json
{
  "version": 1,
  "name": "read-only-agent",
  "allow": ["overview.*", "profile.*", "skill.list", "skill.search", "schema.*"],
  "deny": ["skill.delete", "source.remove", "api.call"],
  "max_risk": "read"
}
```

使用策略调用 CLI：
```bash
aiwc --policy ./policy.json skill delete my-skill --yes
# 输出：退出码 6，返回 command_denied 错误
```

### 5.4 插件平台诊断 (Plugin Config)
如果有第三方安全插件挂载到 CLI 上，可使用 diagnostic 指令排查已启用的 Observer / Wrapper / Restrict 规则（不会泄漏敏感配置值）：

```bash
aiwc config plugins show
```
