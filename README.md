<div align="center">

# AssetIWeave

**把散落在本机、Git 仓库与多个 AI App 中的文件资产，织成一套可发现、可分组、可挂载、可自动化的本地目录。**

Local-first AI asset catalog and mount manager.

[![CI](https://github.com/util6/assetiweave/actions/workflows/ci.yml/badge.svg)](https://github.com/util6/assetiweave/actions/workflows/ci.yml)
[![GitHub tag](https://img.shields.io/github/v/tag/util6/assetiweave?label=version&color=5f8fd9)](https://github.com/util6/assetiweave/tags)
[![GitHub downloads](https://img.shields.io/github/downloads/util6/assetiweave/total?color=2f9d78)](https://github.com/util6/assetiweave/releases)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-41638f)](https://github.com/util6/assetiweave/releases)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24c8db?logo=tauri&logoColor=white)](https://v2.tauri.app/)

<img src="docs/images/readme-hero.svg" alt="AssetIWeave local-first AI asset workflow" width="100%" />

</div>

> [!IMPORTANT]
> AssetIWeave `v0.6.1` 当前覆盖 **Skill 资产管理、Conversation Session 浏览、适配器运行时管理与 CLI 自动化**。桌面端与 CLI 通过 Rust Engine 共用业务规则；更新包使用 Tauri updater 签名验证，安装包仍可能显示额外的操作系统信任提示。

## 为什么需要 AssetIWeave

随着 Codex、Claude、Cursor、Gemini、OpenCode 等工具同时进入工作流，Skills、Prompts、Rules 和配置文件很容易散落在：

- 各个 App 的专属目录中；
- 多个 Git 仓库和项目目录中；
- 临时下载目录与手动备份目录中；
- 不同工具要求的不同目标路径中。

手工复制会带来重复文件、版本漂移、挂载状态不清晰和上下文膨胀。AssetIWeave 将这些文件作为统一的 **Asset** 编目，并通过可解释的挂载关系投影到目标 App。

```mermaid
flowchart LR
    Sources["本地目录 / Git 仓库 / App 目录"] --> Scan["扫描与分类"]
    Scan --> Catalog["统一资产目录"]
    Catalog --> Groups["场景分组"]
    Groups --> Mounts["挂载关系"]
    Mounts --> Apps["Codex / Claude / Cursor / Gemini / Custom"]
```

## 核心能力

### 1. 统一资产目录

在一个目录中搜索和浏览来自不同来源的 AI 文件资产，查看类型、描述、来源路径和挂载状态；通过 App 快捷入口直接控制单个 Skill 的挂载关系。

<p align="center">
  <img src="docs/images/catalog-overview.jpg" alt="AssetIWeave 统一资产目录" width="100%" />
</p>

### 2. 多来源发现与批量管理

将本地目录、Git checkout 和 App 专属目录注册为 Source。每个来源可以配置扫描规则、启停状态和优先级，并支持按来源批量挂载 Skill。

<p align="center">
  <img src="docs/images/source-management.jpg" alt="AssetIWeave 技能源管理" width="100%" />
</p>

### 3. 按工作场景组织 Skill

通过手动成员或规则匹配创建 Skill Group，例如 `Frontend`、`Research`、`Release`。可以将一个或多个场景分组批量挂载到指定 App，减少无关 Skill 带来的上下文负担。

<p align="center">
  <img src="docs/images/skill-groups.jpg" alt="AssetIWeave Skill 分组管理" width="100%" />
</p>

### 4. 面向 App 的挂载管理

集中管理 Codex、Claude、Cursor、OpenCode、Gemini、Antigravity、OpenClaw 与自定义目标。默认使用 **单跳软链接**，目标 App 直接指向真实源资产，不经过中间软链接池。

<p align="center">
  <img src="docs/images/mount-management.jpg" alt="AssetIWeave 应用挂载管理" width="100%" />
</p>

### 5. 显式备份与自动化入口

- 默认不复制或改写 Source 中的真实资产；
- 需要归档时，可将 Skill 显式复制到 `~/.assetiweave/library/skills` 备份库；
- 桌面 App 与 CLI 共用 Rust 业务规则，自动化不会绕过挂载和安全边界；
- CLI 的变更命令支持 `--dry-run`，破坏性命令要求 `--yes`。

### 6. Conversation Session 与文件变更 Diff

- 从 Codex、Claude Code、OpenCode、Antigravity 及 ChatGPT / Gemini / Qwen Web 适配器读取本地会话；Session 与 Web Record 使用独立同步任务。
- 将命令、读取、搜索、失败诊断和文件更改转换为可折叠的内容卡片，成功命令结果默认收敛为状态信息。
- 每个文件更改独立展示为 Diff 卡片，支持文件路径、增删统计和统一 Diff 阅读；仅命令执行壳不会占用主阅读区域。
- 适配器源码集中在 `builtin-assets/`，用户可编辑工作区位于 `~/.assetiweave/conversation-adapters/<cli-name>`，升级成功后才提升为不可变运行副本。

## 当前支持范围

| 能力 | 当前状态 |
| --- | --- |
| Skill 扫描、编目、搜索、描述与来源展示 | 可用 |
| Skill Source 导入、扫描规则与来源级批量挂载 | 可用 |
| Skill Group 创建、规则匹配、批量与独占挂载 | 可用 |
| 互联网 Skill 搜索、GitHub `SKILL.md` 目录识别与确认导入 | 可用 |
| 单个 Skill 快捷挂载、状态刷新与部署计划 | 可用 |
| Skill 备份库、导入与删除 | 可用 |
| Codex / Claude / Cursor / OpenCode / Gemini / Antigravity / OpenClaw / Custom Profile | 可用 |
| Conversation Session / Web Record 浏览、搜索、导出与增量同步 | 可用 |
| Conversation Adapter Catalog、版本历史与运行时切换 | 可用 |
| 独立文件 Diff 卡片与命令结果语义化展示 | 可用 |
| `aiwc` CLI 入口、命令别名与短参数 | 可用 |
| Conversation Adapter 工作区升级与 probe 校验 | 可用 |
| Prompt / Rule / Custom 基础扫描与目录展示 | 基础能力可用 |
| 应用内自动检测、下载、安装与重启更新 | 可用 |
| MCP / Agent / Command / Workflow 专用工作流 | 规划中 |
| 中文 / English 界面与多主题 | 可用 |

## 工作方式

AssetIWeave 将 **来源** 与 **目标投影** 分开处理：

```text
source repo asset
  -> AssetIWeave SQLite catalog + mount relation
  -> target app directory symlink
```

它默认不会这样做：

```text
source repo asset
  -> copied or intermediate symlink pool
  -> target app directory
```

这意味着源仓库仍是事实来源，目标 App 目录可以根据挂载关系重新生成；只有显式执行 Skill 备份时，AssetIWeave 才会复制真实文件。

## 快速开始

1. 在 **技能源管理** 中导入一个包含 `SKILL.md` 的目录。
2. 扫描 Source，将 Skills 加入统一目录。
3. 在 **分组管理** 中按工作场景组织 Skills。
4. 在 **目录总览** 或 **挂载管理** 中选择目标 App。
5. 刷新挂载状态，确认目标目录中的投影结果。

## 安装

前往 [GitHub Releases](https://github.com/util6/assetiweave/releases) 下载 `v0.6.1` 或更新版本的对应平台安装包。

| 平台 | 发布产物 |
| --- | --- |
| macOS | Apple Silicon / Intel `.dmg` |
| Windows | x64 NSIS `.exe` 安装包 |
| Linux | x64 `.AppImage` 或 `.deb` |
| CLI | 随桌面应用内置；可在设置中安装 `assetiweave-cli`、`aiwc` 与 `assetiweave-engine` 到 PATH |

### 未签名安装包说明

当前安装包尚未进行 macOS / Windows 操作系统代码签名，系统可能显示额外的信任提示。请只从本项目的 GitHub Releases 下载并核对来源。

macOS 无法打开时，可以先在 **系统设置 -> 隐私与安全性** 中选择“仍要打开”。确认下载来源可信后，也可以执行：

```bash
xattr -dr com.apple.quarantine "/Applications/AssetIWeave.app"
```

## CLI 自动化

CLI 由 Go + Cobra 提供命令入口，通过 Rust JSON-RPC engine 与桌面 App 共用同一套业务规则。安装后可用简写入口 `aiwc`；原有 `assetiweave-cli` 命令保持兼容。

```bash
aiwc doc
aiwc ov
aiwc src ls
aiwc src a -n LocalSkills -p ./skills -d
aiwc src sc -k skill

aiwc c ad upgrade
aiwc c ad upgrade -d
aiwc c ad upgrade ./path/to/codex

aiwc sk ls
aiwc sk imp -f ./downloaded-skill -n downloaded-skill
aiwc sk s -q "browser automation skill"
aiwc sk acq -u <github-repo-or-tree-url> -d
aiwc sk acq -u <github-repo-or-tree-url> -y
aiwc sk rem chk
aiwc sk b <asset-id>
aiwc sk mt downloaded-skill -p codex
aiwc sk um downloaded-skill -p codex
```

完整命令、JSON 输出约定和通用 API 调用方式见 [CLI 文档](docs/cli.md)。

适配器工作区升级有三种来源：

```bash
# 升级 ~/.assetiweave/conversation-adapters 下的用户工作区
aiwc c ad upgrade

# 从当前仓库的 builtin-assets/adapters 使用开发版本
aiwc c ad upgrade -d

# 只升级指定的适配器目录
aiwc c ad upgrade ./path/to/codex
```

升级流程会先复制 prepared 快照、校验 Manifest 和运行时 probe，成功后写入
`packages/<package_id>/versions/<version>-<content-hash>`，再通过 Engine 激活；失败时保留当前可用版本。

## 本地优先与数据安全

- **无需云端账号**：核心目录、分组、挂载与备份工作流在本机完成。
- **源目录默认只读**：删除 Source 只取消注册，不删除源目录。
- **安全挂载**：默认不覆盖、不删除目标目录中的非托管真实文件。
- **本地元数据**：Source、Asset、Profile、挂载关系与部署状态存储在本机 SQLite。
- **可控联网**：应用更新检查会访问 GitHub Releases；核心资产管理不依赖项目自建云服务。

默认数据位置：

| 数据 | 默认位置 |
| --- | --- |
| SQLite Catalog | 系统应用数据目录下的 `AssetIWeave/app.db` |
| Skill 备份库 | `~/.assetiweave/library/skills` |
| Conversation Adapter 用户工作区 | `~/.assetiweave/conversation-adapters/<cli-name>` |
| Conversation Adapter 不可变运行副本 | `~/.assetiweave/conversation-adapters/packages/<package_id>/versions/` |
| 目标投影 | 各 Profile 配置的目标目录 |

## 架构

```mermaid
flowchart TB
    Desktop["Desktop UI<br/>React + TypeScript + Tauri 2"] --> Service["Rust service"]
    CLI["Go CLI"] --> Engine["Rust JSON-RPC engine"] --> Service

    Sources["Local dirs / Git checkouts / App targets"] --> Scanner["Scanner + classifier"]
    Service --> Scanner
    Scanner --> Catalog["SQLite catalog"]
    Service --> Catalog

    Conversations["Session / Web Record adapters"] --> Normalizer["Payload policy + normalized cards"]
    Normalizer --> Catalog

    Catalog --> Planner["Mount relations + deployment plan"]
    Service --> Planner
    Planner -->|"single-hop symlink"| Targets["Codex / Claude / Cursor / OpenCode / Gemini / Custom"]
    Service --> Backup["Explicit Skill backup<br/>~/.assetiweave/library/skills"]
```

| 层 | 技术与职责 |
| --- | --- |
| Desktop | React 19、TypeScript、Vite、Tailwind CSS、Tauri 2 |
| Service / Engine | Rust，统一扫描、存储、挂载、部署计划与文件系统规则 |
| CLI | Go 1.24 + Cobra，提供友好命令与通用 API 调用 |
| Storage | SQLite + 本地文件系统 |

开发时的目录职责、文件落点和新旧架构边界见 [仓库目录与架构边界](docs/repository-structure.md)。

## 本地开发

### 前置要求

- Node.js 22
- pnpm 10
- Rust stable
- Go 1.24
- 对应平台的 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### 启动桌面应用

```bash
pnpm install
pnpm tauri:dev
```

只预览前端界面：

```bash
pnpm dev
```

### 构建

```bash
pnpm tauri build
pnpm cli:install
pnpm cli:run -- doctor
```

### 验证

```bash
pnpm typecheck
pnpm test
pnpm conversation-adapters:test
pnpm build
go vet -C cli ./...
go test -C cli ./...
cargo test --workspace
```

## Roadmap

- 深化 MCP、Prompt、Rule、Agent、Command 与 Workflow 的专用管理工作流；
- 增加带 manifest 的显式资产导出；
- 增加复用部署计划与安全规则的文件 watcher / 自动同步；
- 完善冲突解释、执行结果与回滚体验；
- 接入 macOS / Windows 代码签名与发布公证。

## 参与项目

- 通过 [Issues](https://github.com/util6/assetiweave/issues) 报告问题或提出功能建议；
- 提交改动前运行完整验证命令；
- 涉及挂载或删除行为时，请附上 `--dry-run` / 部署计划结果与测试用例。
