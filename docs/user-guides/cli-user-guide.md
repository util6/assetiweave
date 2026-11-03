# AssetIWeave CLI (`aiwc`) 完整用户指南与命令地图手册

> **版本**：v0.7.0+ (Rust Native 纯客户端架构)  
> **二进制名称**：`aiwc`（快捷简称）与 `assetiweave-cli`（标准全称，两者指向同一可执行文件）  
> **架构定位**：AssetIWeave 是本地优先（Local-first）的 AI 文件资产与对话记录管理器。全新的 CLI 是基于纯 Rust 实现的 Native 客户端（`backend/cli`），以 **OneShot In-Process** 方式直连统一业务编排门面 `AppService`，实现微秒级响应与零跨进程管道通信开销，与桌面端 (Tauri 2 GUI) 共享完全一致的业务规则、SQLite 事实源与软链接挂载。

---

## 目录

1. [快速入门与编译安装](#1-快速入门与编译安装)
2. [全景命令树拓扑 (Topology Map)](#2-全景命令树拓扑-topology-map)
3. [全局选项、快捷别名与短 Flag 速查](#3-全局选项快捷别名与短-flag-速查)
4. [全命令域深度参考与实战](#4-全命令域深度参考与实战)
   - [4.1 系统概览、诊断与环境配置 (`overview`, `doctor`, `settings`, `version`, `update`, `completion`, `config`)](#41-系统概览诊断与环境配置)
   - [4.2 资产源、资产与 Profile 管理 (`source`, `asset`, `profile`, `tenant`)](#42-资产源资产与-profile-管理)
   - [4.3 Skill 技能扩展全生命周期 (`skill`)](#43-skill-技能扩展全生命周期)
   - [4.4 归一化对话记录与转译器系统 (`conversation`)](#44-归一化对话记录与转译器系统)
   - [4.5 渐进式 Memory 与持久化召回系统 (`memory`)](#45-渐进式-memory-与持久化召回系统)
   - [4.6 强 AI 纯引擎与 Agent 市场 (`agent`)](#46-强-ai-纯引擎与-agent-市场)
   - [4.7 本地网页对话采集器生态 (`harvester`)](#47-本地网页对话采集器生态)
   - [4.8 引擎 Schema 探测与底层 RPC 调试 (`schema`, `api`)](#48-引擎-schema-探测与底层-rpc-调试)
5. [AI Agent 自动化集成与 Token 优化策略](#5-ai-agent-自动化集成与-token-优化策略)
6. [常见排查与诊断手册](#6-常见排查与诊断手册)

---

## 1. 快速入门与编译安装

### 1.1 编译与安装

在仓库根目录下，通过 `cargo` 或 `pnpm` 脚本编译并安装二进制到本地：

```bash
# 1. 运行 CLI 单元测试
cargo test -p cli

# 2. 编译 Release 版本
cargo build -p cli --release

# 3. 安装到系统 PATH (例如 ~/.cargo/bin/aiwc)
cargo install --path backend/cli --bin aiwc --force
```

在本地开发调试阶段，也可以直接通过 cargo 运行：
```bash
cargo run -p cli -- overview
```

### 1.2 环境变量一览

| 环境变量 | 作用说明 |
| --- | --- |
| `ASSETIWEAVE_DB_PATH` | 显式覆盖默认 SQLite 数据库路径，常用于沙盒隔离与自动化测试 |
| `GITHUB_TOKEN` / `GH_TOKEN` | 用于 `aiwc skill search` 与 `aiwc skill acquire` 鉴权，突破 GitHub API 速率限制 |

---

## 2. 全景命令树拓扑 (Topology Map)

```text
aiwc [全局参数: --format table|json]
├── version (v, ver)              # 查看 CLI 与运行时版本，支持 --check-updates
├── overview (ov, o)              # 展现系统全局资产/会话/挂载指标大盘
├── source (src)                  # 资产源目录管理
│   ├── list                      # 列出已注册的资产源
│   ├── add                       # 注册新源码目录 (--path, --name, --kind, --dry-run)
│   ├── remove                    # 注销资产源 (<ID>, --yes, --dry-run)
│   └── scan                      # 扫描并同步资产变动 (--kind, --dry-run)
├── asset (as)                    # 资产总览
│   └── list                      # 列出全局或指定类型的资产 (--kind)
├── profile (p, prof)             # 目标应用 Profiles
│   └── list                      # 列举所有受支持的应用环境 (如 cursor, codex, claude 等)
├── tenant (t, tn)                # 多租户工作空间管理
│   ├── list                      # 列出所有租户
│   ├── active                    # 显示当前激活的工作租户
│   ├── switch                    # 切换当前终端空间 (<ID>)
│   └── create                    # 创建隔离租户空间 (<ID>, --name)
├── skill (sk)                    # Skill 技能全生命周期管理
│   ├── list                      # 列出本地库及已发现的所有技能
│   ├── builtin (b)               # 系统内置预置技能
│   │   ├── list                  # 列出所有内置技能
│   │   └── read                  # 读取内置技能 markdown 全文 (<NAME>)
│   ├── read                      # 直接读取内置/本地技能定义正文 (<NAME>)
│   ├── mount                     # 挂载技能到目标应用 Profile (<REF>, --profile, --dry-run)
│   ├── unmount                   # 从目标 Profile 解绑技能 (<REF>, --profile, --dry-run)
│   ├── import                    # 从本地目录导入技能到备份库 (--from, --name, --dry-run)
│   ├── backup                    # 将索引中的技能封存备份到备份库 (<ASSET_ID>)
│   ├── delete                    # 删除指定技能 (<REF>, --profile, --dry-run, --yes, --unmount)
│   ├── group                     # 技能组分组编排管理
│   │   ├── list                  # 列出所有技能组
│   │   ├── show                  # 查看指定技能组详情 (<GROUP_ID>)
│   │   ├── create                # 创建技能组 (--name, --description)
│   │   ├── delete                # 删除技能组 (<GROUP_ID>, --yes)
│   │   ├── mount                 # 批量挂载技能组到 Profile (<GROUP_ID>, --profile)
│   │   └── unmount               # 批量解绑技能组 (<GROUP_ID>, --profile)
│   ├── search                    # 全网/GitHub 技能发现检索 (--query, --provider, --limit)
│   ├── acquire                   # 远程技能下载导入与登记 (--url, --branch, --path, --dry-run, --yes)
│   └── remote                    # 远程技能源与版本漂移追踪
│       ├── list                  # 列出已登记的远程技能源
│       └── check                 # 探测远程仓库是否有上游更新漂移
├── conversation (c, conv)        # 归一化 AI 对话记录与转译器系统 (核心)
│   ├── list                      # 多维度过滤列出会话记录 (--window, --since, --range, --project, ...)
│   ├── search                    # Tantivy 全文/语义卡片检索 (--query, --kinds, --since, --timeline)
│   ├── outline                   # 会话结构列状骨架还原与降维 (<ID>, --questions, --qa, --full)
│   ├── card                      # 会话卡片精细读取
│   │   └── get                   # 精确获取单张卡片正文 (<CARD_ID>, --raw)
│   ├── sync                      # 触发本地 Adapter 增量/全量同步 (--adapter, --full, --incremental)
│   ├── adapter                   # 转译适配器管理
│   │   ├── list                  # 列出已安装的对话适配器与健康状况
│   │   ├── inspect               # 检查指定适配器元数据与配置 (<ADAPTER_ID>)
│   │   └── upgrade (up)          # 提升注册可编辑适配器并更新指纹 ([PATH], -d/--developer, --dry-run)
│   ├── source                    # 对话源目录配置
│   │   ├── list                  # 列出注册的对话来源路径
│   │   └── sync                  # 单独重放或解析特定源 (<SOURCE_ID>, --full)
│   └── data                      # 对话底层数据一致性治理
│       ├── audit                 # 审计对话实体与索引投影健康度 (--source, --record-kind)
│       ├── repair                # 修复数据孤岛与索引漂移 (--dry-run, --resync, --yes)
│       └── rollback              # 从维护快照安全回滚还原 (--backup-path, --dry-run, --yes)
├── memory (m, mem)               # 渐进式记忆与持久化召回系统
│   ├── recent                    # 近期工作状态快照
│   │   └── get                   # 获取指定日期或最新工作记忆 (--date)
│   ├── context                   # 认知上下文编译求解
│   │   └── resolve               # 求解 Prompt 记忆上下文 (--project, --current-project, --token-budget)
│   ├── project                   # 项目级别长期记忆
│   │   └── get                   # 获取指定项目或当前目录的长期项目记忆 ([PATH], --current-project)
│   ├── rebuild                   # 触发记忆重建与消融流水线 (--target, --reason, --force)
│   ├── task                      # 记忆后台流水线任务监控
│   │   ├── list                  # 列出记忆提炼长任务 (--active-only)
│   │   ├── get                   # 查看指定任务详情 (<TASK_ID>)
│   │   ├── cancel                # 取消进行中的任务 (<TASK_ID>)
│   │   └── retry                 # 重试失败的提炼任务 (<TASK_ID>)
│   └── recall                    # 持续会话交互式召回系统
│       ├── search                # 检索召回库核心事实与片段 (--query, --project, --limit)
│       ├── session               # 召回持久化 Session 管理
│       │   ├── create            # 创建新的召回交互会话 (--project, --current-project)
│       │   └── get               # 恢复或检查召回会话详情 (<SESSION_ID>)
│       └── turn                  # 召回交互回合
│           ├── send              # 向召回会话提交问题并推进思考 (<SESSION_ID>, --query)
│           └── cancel            # 中断思考回合 (<TURN_ID>)
├── agent (ag)                    # 强 AI 纵切纯引擎与 Agent 市场管理
│   ├── market                    # 官方与受控市场目录
│   │   ├── list                  # 浏览市场精选 Agent (--query, --protocol, --installed-only)
│   │   ├── inspect               # 检查市场中某个 Agent 的详细分发规格 (<AGENT_ID>)
│   │   └── refresh               # 刷新远程市场索引缓存 (--yes)
│   ├── installed                 # 本地已安装 Agent
│   │   ├── list                  # 列出所有本地托管的 Agent
│   │   └── get                   # 获取某个已安装 Agent 的详细配置 (<AGENT_ID>)
│   ├── install                   # 安装 Agent (<AGENT_ID>, --distribution, --yes)
│   ├── update                    # 更新 Agent (<AGENT_ID>, --yes)
│   ├── reinstall                 # 重新部署 Agent (<AGENT_ID>, --yes)
│   ├── uninstall                 # 卸载 Agent (<AGENT_ID>, --yes)
│   ├── enable                    # 启用已禁用的 Agent (<AGENT_ID>)
│   ├── disable                   # 禁用 Agent (<AGENT_ID>)
│   └── check                     # 探针检测 Agent 运行时健康度与沙箱连通性 ([AGENT_ID])
├── harvester (hv)                # 本地 Web 对话采集器生态
│   ├── template                  # 采集器官方打包模板
│   │   └── list                  # 列出官方附带的采集器模板 (--root)
│   ├── install                   # 安装采集器模板到 ~/.assetiweave (<ID>, --from, --force)
│   ├── update                    # 更新本地采集器模板 ([ID], --all)
│   ├── list                      # 列出已安装的 Web 采集器 (--root)
│   ├── doctor                    # 诊断采集器运行时与依赖环境 (<ID>)
│   ├── repair                    # 修复损坏的采集器脚本但保留凭据 (<ID>, --yes)
│   └── run                       # 执行指定采集脚本 (<ID>, --timeout, --yes, -- [ARGS...])
├── settings (st)                 # 应用全局配置与存储路径
│   ├── show                      # 显示当前生效配置项与持久化路径
│   └── save                      # 更新全局配置 (--json <JSON>)
├── doctor (doc)                  # 全局健康体检与依赖自诊断
├── config (cfg)                  # 客户端配置检查
│   └── plugins show              # 查看 CLI 插件清单及配置项
├── update (up, u)                # CLI 二进制自我更新与版本探测 (--check, --yes)
├── completion (comp)             # 生成 Shell 补全脚本 (bash, zsh, fish, powershell, elvish)
├── schema (sch)                  # 底层 Engine 内部 RPC 方法 Schema 探测 ([METHOD])
└── api (rpc)                     # 底层 RPC 接口直通车 (开发者高级调试)
    └── call                      # 直接发送 JSON 参数调用后端方法 (<METHOD>, --json, --yes)
```

---

## 3. 全局选项、快捷别名与短 Flag 速查

### 3.1 全局呈现格式 (`--format`)

- `--format table`（默认）：面向人类用户的整齐表格。
- `--format json`：面向脚本与 AI Agent 的标准 JSON 数据包。

### 3.2 常用顶层别名与缩写

| 标准命令 | 快捷别名 | 说明 |
| :--- | :--- | :--- |
| `overview` | `ov`, `o` | 展现全局数据与资产指标大盘 |
| `source` | `src` | 资产源管理 |
| `skill` | `sk` | Skill 技能扩展全生命周期管理 |
| `conversation` | `c`, `conv` | 归一化对话记录与转译器系统 |
| `memory` | `m`, `mem` | 渐进式 Memory 与语义召回 |
| `agent` | `ag` | Agent 市场与运行时生命周期 |
| `harvester` | `hv` | 本地网页对话采集器管理 |
| `profile` | `p`, `prof`, `target-profile` | 目标 Profile 查看 |
| `tenant` | `t`, `tn` | 隔离租户管理 |
| `settings` | `st` | 应用设置查看与保存 |
| `doctor` | `doc` | 系统与存储健康诊断 |
| `config` | `cfg` | CLI 配置与插件列表 |
| `update` | `up`, `u` | CLI 自更新检测与执行 |
| `completion` | `comp` | 生成终端自动补全脚本 |
| `schema` | `sch` | Engine 方法契约自省 |
| `api` | `rpc` | 底层 RPC 方法直调 |
| `version` | `v`, `ver` | 版本诊断探针 |

---

## 4. 全命令域深度参考与实战

### 4.1 系统概览、诊断与环境配置

#### 查看全局大盘 (`overview`)
```bash
aiwc overview
# 或
aiwc o
```

#### 健康诊断 (`doctor`)
全面检查 SQLite 连接、存储路径可写性与当前租户状态：
```bash
aiwc doctor
```

#### 查看与保存设置 (`settings`)
```bash
# 查看设置
aiwc settings show

# 通过 JSON 保存修改
aiwc settings save --json '{"density":"compact"}'
```

#### 生成 Shell 补全 (`completion`)
```bash
# 为当前终端 (如 zsh) 生成补全配置
aiwc completion zsh > ~/.zfunc/_aiwc
```

---

### 4.2 资产源、资产与 Profile 管理

#### 资产源注册与扫描 (`source`)
```bash
# 列出注册的源码目录
aiwc src list

# 注册新的技能源目录 (带预览)
aiwc src add --path "~/workspace/my-skills" --name "MySkills" --kind skill --dry-run
# 正式登记
aiwc src add --path "~/workspace/my-skills" --name "MySkills" --kind skill

# 触发资产变更扫描
aiwc src scan --kind skill
```

#### 查看目标应用 Profile (`profile`)
```bash
aiwc profile list
```

#### 多租户空间隔离 (`tenant`)
```bash
# 查看所有租户
aiwc tenant list

# 显示当前激活租户
aiwc tenant active

# 切换租户
aiwc tenant switch corp-project
```

---

### 4.3 Skill 技能扩展全生命周期

#### 本地 Skill 导入、管理与挂载
```bash
# 列出已索引的所有 Skill
aiwc skill list

# 从本地文件夹导入 Skill 到 AssetIWeave 集中备份库
aiwc skill import --from ./my-custom-skill --name "my-skill"

# 试运行挂载技能到 cursor profile
aiwc skill mount my-skill --profile cursor --dry-run

# 正式建立单层软链接挂载
aiwc skill mount my-skill --profile cursor

# 解绑技能 (安全移除软链接，绝不伤害源资产)
aiwc skill unmount my-skill --profile cursor
```

#### 内置预置技能读取 (`builtin`)
```bash
# 列出系统内置的官方技能
aiwc skill builtin list

# 直接查看某个内置技能的 Markdown 正文
aiwc skill builtin read assetiweave-memory
```

#### 远程技能搜索与安全获取 (`search`, `acquire`, `remote`)
```bash
# 在 GitHub 搜索开源技能
aiwc skill search --query "frontend test" --limit 5

# 从 GitHub 安全下载并收纳到库中
aiwc skill acquire --url "https://github.com/example/skills" --path "skills/vitest" --yes

# 检查已收纳的远程技能是否有上游更新漂移
aiwc skill remote check
```

---

### 4.4 归一化对话记录与转译器系统

AssetIWeave 的杀手级能力之一：将 Claude Code、Codex、Cursor、Antigravity、OpenCode 等外部工具的历史对话统一结构化，提供三级逐级下钻机制。

#### 4.4.1 会话列表与多维时间过滤 (`conversation list`)
```bash
# 查看最近 24 小时在当前项目工作区产生的对话
aiwc conversation list --window 24h --current-project

# 自然语言过滤时间
aiwc c list --since "yesterday" --limit 10
aiwc c list --range "2026-10-25..2026-10-30"

# 按特定转译适配器过滤 (例如只看 codex)
aiwc c list --adapter codex
```

#### 4.4.2 三级下钻骨架还原 (`conversation outline`)
在深入长对话前，**严禁无差别全量读取消息**。使用 `outline` 进行紧凑列状骨架还原：

```bash
# 1. 紧凑列状结构 (默认显示 Turns、卡片类型、参与角色与行动)
aiwc c outline <SESSION_ID>

# 2. 纯提问主线模式 (--questions): 提炼每轮的核心问题，极省 Token
aiwc c outline <SESSION_ID> --questions

# 3. 问答对概要模式 (--qa): 显示提问与回答总结
aiwc c outline <SESSION_ID> --qa

# 4. 全量结构大纲 (--full): 包含 activities 与卡片引用
aiwc c outline <SESSION_ID> --full
```
> [!TIP]
> `outline` 命令的 `<ID>` 参数支持模糊输入！你可以直接传入完整的 Session ID、Turn ID、Card ID，甚至仅传入 **8 位十六进制前缀**，系统将自动定位。

#### 4.4.3 单张卡片无损提取 (`conversation card get`)
在通过 `outline` 定位到目标卡片后，精准读取单张卡片：
```bash
# 查看卡片元数据与格式化内容
aiwc c card get <CARD_ID>

# 获取纯原始 Markdown/代码，供重定向或输入到后续管道
aiwc c card get <CARD_ID> --raw
```

#### 4.4.4 会话增量同步与健康修复 (`sync`, `data`)
```bash
# 增量同步本地所有已安装适配器的会话
aiwc c sync

# 强制全量重新同步某个适配器
aiwc c sync --adapter claude-code --full

# 审计对话数据库与 Tantivy 索引的一致性
aiwc c data audit

# 试运行修复孤岛或索引漂移
aiwc c data repair --dry-run

# 确认执行安全修复
aiwc c data repair --yes
```

#### 4.4.5 开发环境适配器一键提升与重算指纹 (`adapter upgrade`)
当在开发环境中修改或更新了对话解析脚本（如 `builtin-assets/adapters/*`）后，直接同步会因内容哈希（Fingerprint）不匹配被系统防御门禁拦截。使用 `upgrade` 即可自动完成探测校验、重算 SHA-256 指纹并不可变提升到系统运行时库：

```bash
# 开发环境核心杀手锏：一键重算本仓库 builtin-assets/adapters 的全部脚本并登记受信任指纹
aiwc conversation adapter upgrade --developer
# 极简缩写
aiwc c ad up -d

# 针对单个适配器开发工作区目录提升
aiwc c ad upgrade ./builtin-assets/adapters/codex

# 试运行预览（只校验语法与目录结构，不写运行时库）
aiwc c ad up -d --dry-run
```

---

### 4.5 渐进式 Memory 与持久化召回系统

管理 AssetIWeave 的多层次记忆（Recent Work -> Project Memory -> Global Memory）。

#### 4.5.1 近期工作记忆与项目长期记忆
```bash
# 获取今日最近的工作概要快照
aiwc memory recent get

# 获取指定日期的工作记忆快照
aiwc m recent get --date 2026-10-28

# 获取当前工作区项目的长期架构记忆
aiwc m project get --current-project
```

#### 4.5.2 上下文编译求解 (`context resolve`)
在开启新任务前，求解最匹配当前工作区的提示词记忆切片，严格控制 Token 上限：
```bash
# 在 2000 token 预算内求解上下文
aiwc m context resolve --current-project --token-budget 2000

# 携带问题针对性提取记忆上下文
aiwc m context resolve --current-project --query "数据库锁机制设计" --token-budget 1500
```

#### 4.5.3 交互式多轮 Recall 召回会话
面向复杂的历史问题推理，开启持久化 Recall 交互：
```bash
# 1. 创建召回会话
aiwc m recall session create --current-project

# 2. 发送追问回合
aiwc m recall turn send <SESSION_ID> --query "之前关于 ADR-0017 后台任务规范的决策结论是什么？"

# 3. 必要时中断进行中的推理
aiwc m recall turn cancel <TURN_ID>
```

---

### 4.6 强 AI 纯引擎与 Agent 市场

管理自主 Agent 进程的部署、沙箱与通信（支持 ACP 协议与 Native 进程）。

```bash
# 浏览精选 Agent 市场
aiwc agent market list

# 筛选支持 ACP 协议的 Agent
aiwc ag market list --protocol acp

# 检查 Agent 详细分发规格与沙箱声明
aiwc ag market inspect "agent-memory-synthesizer"

# 安装并初始化运行时环境
aiwc ag install "agent-memory-synthesizer" --yes

# 探针检测 Agent 通信健康度
aiwc ag check "agent-memory-synthesizer"

# 控制 Agent 启停
aiwc ag disable "agent-memory-synthesizer"
aiwc ag enable "agent-memory-synthesizer"
```

---

### 4.7 本地网页对话采集器生态

支持从 ChatGPT、Claude.ai 等网页平台爬取对话并无缝收录到本地存储中。

```bash
# 列出打包的官方采集器模板
aiwc harvester template list

# 安装 ChatGPT 采集器到本地 ~/.assetiweave/harvesters
aiwc hv install chatgpt-web

# 诊断采集器本地环境 (浏览器驱动、登录态等)
aiwc hv doctor chatgpt-web

# 运行采集任务
aiwc hv run chatgpt-web --timeout 300 --yes -- --export-path ~/Downloads/conversations
```

---

### 4.8 引擎 Schema 探测与底层 RPC 调试

面向高级开发者与系统测试：

```bash
# 列出后置服务暴露给客户端的所有 RPC 接口
aiwc schema

# 查看具体方法的输入输出 Schema
aiwc schema conversation.list

# 以裸 JSON 格式直接调用底层业务方法
aiwc api call list_asset_mounts --json '{"assetId":null}'
```

---

## 5. AI Agent 自动化集成与 Token 优化策略

### 5.1 JSON 标准信封消费

当在 Agent 工作流、子代理（Subagent）或自动化脚本中调用时，**务必始终添加 `--format json`**。

输出为标准的强类型 JSON 结构：
```json
{
  "status": "ok",
  "data": { ... },
  "meta": {
    "duration_ms": 12
  }
}
```

### 5.2 极致防 Token 爆炸三步法（Agent 必读）

针对超大上下文环境，AI Agent 请严格遵循以下 SOP：

1. **第一步：定位会话**  
   使用 `aiwc --format json c list --current-project --limit 5` 获取候选 Session ID。
2. **第二步：骨架筛选**  
   使用 `aiwc --format json c outline <SESSION_ID> --questions` 仅拉取问题列表，评估相关度。
3. **第三步：按需下钻**  
   仅对真正包含目标代码或方案的卡片，使用 `aiwc c card get <CARD_ID> --raw` 读取具体内容。

---

## 6. 常见排查与诊断手册

1. **终端命令找不到 `aiwc`**：  
   确保 `~/.cargo/bin` 在你的 `PATH` 中（`export PATH="$HOME/.cargo/bin:$PATH"`）。
2. **需要静默自动化脚本**：  
   凡涉及删除、重置、网络下载或代码运行的操作，必须显式附加 `--yes`（或 `-y`），配合 `--format json` 实现纯无人值守运行。
3. **发现新对话没有出现在搜索中**：  
   运行 `aiwc c sync`。若刚添加了外部对话目录，可先执行 `aiwc c source sync <SOURCE_ID> --full`。
4. **数据索引损坏**：  
   执行 `aiwc c data audit` 进行健康体检；必要时通过 `aiwc c data repair --yes` 安全修复。
