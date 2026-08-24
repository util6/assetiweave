# AssetIWeave CLI 参考手册

> [!TIP]
> 如果你需要面向用户和 AI Agent 的详细使用指南、命令说明与例程速查，请参阅 [CLI 用户指南](cli-user-guide.md)。

AssetIWeave CLI 遵循类似飞书/Lark CLI 的分层设计：

```text
AI / 用户
  -> 手写快捷别名命令          source / skill / asset / profile
  -> 自动生成的 App 领域命令    app <method>（由 Rust Contract 驱动）
  -> 底层 Engine API 接口       api call <method>
  -> assetiweave-engine       基于 stdio 的 Rust JSON-RPC 协议
  -> src-tauri/src/service.rs Rust 统一业务服务门面
  -> SQLite / 文件系统 / 符号链接目标
```

Go 语言编写的 CLI 仅仅是一个客户端。它绝不直接读写 SQLite、复制 Skill 资产或创建符号链接。这些底层操作完全保留在 Rust 后端中，从而确保桌面端 GUI 和命令行 CLI 始终共享完全一致的业务规则。

## 架构对齐快照

本评估将 AssetIWeave CLI 与 `larksuite/cli`（远程参考 commit `8c3cba1`）的可复用架构进行了横向对比。其目标是实现架构对齐，而非直接复制 Lark 特有的产品领域（如云端 OAuth、OpenAPI HTTP 传输或飞书事件流）。

| 领域 | AssetIWeave 状态 | 说明 |
| --- | --- | --- |
| 命令表面分层 | 已实现 | 手写快捷别名、自动生成的 App 命令以及原始 Engine API 镜像了 Lark 的 shortcut/service/API 三层设计。 |
| 共享业务核心 | 已实现 | CLI 调用 Rust Engine 及共享的 `AppService`；绝不绕过桌面端业务规则。 |
| 命令契约代码生成 | 已实现 | Rust DTO 统一驱动 Schema、参数校验、Handler 反序列化以及 CLI 生成的命令行 Flag。 |
| 协议版本兼容性 | 已实现 | CLI 请求和 Engine 响应均包含协议版本与契约版本；`version` 保留为诊断探测命令。 |
| 运行时策略与确认门禁 | 已实现 | Engine 集中管理命令策略、高风险操作确认、参数校验、Handler 执行以及调用上下文元数据。 |
| 插件扩展平台 | 基本已实现 | 提供了 Observer、Wrapper、Lifecycle、Restrict 规则、失败策略、版本约束、清单查询和本地配置能力。 |
| 插件诊断查询 | 已实现 | `config plugins show` 展示插件清单及配置项 Key 名，绝不泄漏敏感配置内容。 |
| 用户策略修剪 | 已实现（CLI 插件） | 支持插件限制规则与父级命令聚合；Lark 的 YAML 用户策略层由 `ASSETIWEAVE_POLICY_PATH` 承载。 |
| 结构化错误契约 | 已实现 | 类型化的 `cli/errs/` 错误分类覆盖了 CLI 本地及 Engine 返回的校验、配置、策略、Hook、确认、业务、协议和内部错误。明确的传输层类型保留了面向 Agent 的 `error.type`、退出码、详细信息、修复建议以及调用元数据。 |
| 命令语法自动恢复 | 已实现 | 针对未知的根/嵌套命令、未知或放错位置的 Flag、无效的 Flag 值、缺失的必选参数等，统一返回 exit 2 的类型化校验信封并附带稳定的建议修复提示。纯命令分组保持在元数据、策略与插件调用语义之外。 |
| 引导与全局 UX 选项 | 已实现 | CLI 支持根引导、诊断旁路、预解析 `--plugin-config`、全局 `--engine`/`--policy` 覆盖、补全引导门禁，以及用于单 Profile 打包的可选 Profile 命令隐藏策略。 |
| 输出适配器 | 保持极简 | AssetIWeave 当前优先面向 AI Agent 采用 JSON 格式；未照搬 Lark 的 table/CSV/JQ/彩色格式化层。 |
| Skill 发现与获取 | 已实现首批 Provider | `skill search` 基于 Provider 进行互联网检索，尽可能将 GitHub 仓库解析为具体的 `SKILL.md` 目录；`skill acquire` 经由共享 Rust 服务下载并导入选中的候选 Skill。已确认的获取会记录远程源元数据以便后续进行漂移检测。 |
| 鉴权、凭据与传输层 | 产品特异性缺口 | 不应照搬 Lark 的 OAuth/Keychain/HTTP 传输栈。AssetIWeave 仅在桌面端有需求时提供对等的本地 workspace/profile/Engine 端点配置。 |
| 事件流运行时 | 产品特异性缺口 | Lark 的事件消费/状态/停止运行时对应飞书 Webhook。AssetIWeave 仅在桌面端暴露该工作流时才追加本地事件或会话运行时。 |
| 发布与更新提示 | 部分实现 | `version` 报告 CLI 发布出处、Engine 兼容性，并通过 `--check-updates` 提供可选的远程更新清单诊断。Release 构建在发现新版本时向 JSON 信封中注入缓存的 `_notice.update` 提示。`update --check` 解析匹配的工具发布包，`update --yes` 下载、校验哈希并替换本地 CLI 工具。`skill remote check` 提供明确的 Skill 漂移状态；主动后台通知作为后续规划。 |
| 静态架构门禁 | 已实现 | 包含契约漂移、e2e 集成测试、声明式错误子类型校验、零基线遗留错误构造防护、命令元数据检查与发布审计门禁。 |

近期规划将在 AssetIWeave 接入更多外部 Skill 状态源后，补充更丰富的 Provider/排序 Hook 以及 UI 漂移徽标。只有在桌面端具备相应工作流时，才会引入对应的 Lark 领域能力。

## 编译构建

```bash
pnpm engine:build
pnpm cli:contract
pnpm cli:build
pnpm cli:test:e2e
```

或者通过包装脚本一次性构建本地二进制：

```bash
pnpm cli:install
```

## 快捷别名命令

`aiwc` 与 `assetiweave-cli` 一同安装和构建；两个命令名称均执行完全相同的二进制文件。原有的长命令保持兼容，同时提供了命令别名和单字符 Flag 缩写，以减少交互式输入成本：

```bash
# assetiweave-cli source list
aiwc src ls

# assetiweave-cli source add --name LocalSkills --path ./skills --dry-run
aiwc src a -n LocalSkills -p ./skills -d

# assetiweave-cli skill acquire --url <github-url> --yes
aiwc sk acq -u <github-url> -y

# assetiweave-cli conversation session get <session-id>
aiwc c ses g <session-id>

# 从 ~/.assetiweave/conversation-adapters 提升可编辑适配器
aiwc c ad upgrade

# 从本仓库的 builtin-assets/adapters 提升适配器
aiwc c ad upgrade -d

# 从任意目录提升适配器工作区
aiwc c ad upgrade ./path/to/codex

# assetiweave-cli memory recall preview --query <question> --current-project
aiwc m rec pv -q <question> -c
```

常用的一级命令别名包括：

| 标准命令 | 别名 |
| --- | --- |
| `overview` | `ov`, `o` |
| `source` | `src` |
| `skill` | `sk` |
| `conversation` | `c`, `conv` |
| `memory` | `m`, `mem` |
| `harvester` | `hv` |
| `profile` | `p`, `prof` |
| `tenant` | `t`, `tn` |
| `version` | `v`, `ver` |

常用的嵌套子命令别名包括 `list -> ls`、`create -> cr`、`get -> g`、`search -> s`、`update/upgrade -> up`、`delete/remove -> rm`、`preview -> pv` 以及 `run -> r`。每个可见选项在其命令内部都拥有唯一的缩写；可通过 `aiwc <command> --help` 查看具体映射。稳定的常用 Flag 缩写包括 `--name/-n`、`--path/-p`、`--query/-q`、`--limit/-l`、`--dry-run/-d` 以及 `--yes/-y`。注意：在 `conversation adapter upgrade` 命令中，`-d` 专门表示 `--developer`；预览操作请使用 `--dry-run/-r`。

在开发期间，`assetiweave-cli` 按以下优先级顺序查找 Engine：

1. `ASSETIWEAVE_ENGINE` 环境变量
2. `PATH` 中的 `assetiweave-engine`
3. `target/debug/assetiweave-engine`

对于隔离测试或冒烟检查，可将 `ASSETIWEAVE_DB_PATH` 和 `HOME` 设置为临时目录。

`pnpm cli:run -- <args>` 会运行 `scripts/run.js`，该脚本会自动定位本地 `assetiweave-cli` 二进制，并在本地 Engine 二进制可用时自动注入 `ASSETIWEAVE_ENGINE`。

`pnpm cli:contract` 会将 Rust 命令注册表导出到 `cli/internal/schema/contract.json`。Go CLI 会内嵌此文件以生成 App 领域命令层。若提交的代码契约与 Rust 注册表发生漂移，Rust 单元测试将会报错失败。

命令契约 v2 会从绑定到每个已注册方法的 Rust 请求 DTO 中自动推导参数类型、必填字段、枚举值和嵌套对象 Schema。注册表仅叠加命令风险等级、说明文案、CLI 暴露状态、允许的别名以及可执行的 Handler。相同的 `command!` 宏声明同时驱动 Schema 生成、预检参数校验和类型化 Handler 反序列化。Engine 分发不再进行基于字符串的二次方法匹配，从而防止注册的 Schema 被静默路由到其他 Handler 或请求类型。

每个 CLI 请求都附带支持的 Engine 协议版本与命令契约版本。每个 Engine 响应都在 `meta` 中返回实际版本。普通命令在分发前会拒绝不兼容的 Engine 二进制；`version` 保留为诊断探测探针：

```bash
assetiweave-cli version
```

Engine 对每个注册的方法应用统一的集中式运行时处理管道：

```text
命令注册表查找
  -> 命令策略校验
  -> 高风险操作确认
  -> 运行时参数契约校验
  -> 注册的类型化 Handler 执行
  -> 共享 AppService 业务操作
  -> 后置 Hook 与调用元数据记录
```

无论是成功还是错误响应信封，均会完整保留 Engine 元数据。`meta.invocation` 包含请求方法、规范方法名、风险等级、暴露状态、执行结果、应用的运行时 Hook 以及耗时时长。这使得被拒绝和无效的请求在无需打开数据库或进入业务分发前即可被准确观测。

Go CLI 还具备一套类 Lark CLI 的插件扩展平台：

```text
已注册的插件
  -> 通过 extension/platform.Registrar 完成原子安装
  -> 可选的插件 Restrict 限制规则
  -> 命令树策略拒绝桩代码
  -> Observer / Wrapper 命令 Hook
  -> Startup 和 Shutdown 生命周期 Hook
```

Shell 自动补全通过轻量级的引导路径运行。`completion`、`__complete` 和 `__completeNoDesc` 构建命令树与元数据，但会跳过插件配置加载、插件安装、生命周期 Hook、命令策略修剪以及更新检查通知，以确保自动补全输出不会被运行时诊断或网络 I/O 干扰损坏。

单 Profile 构建版本可设置 `ASSETIWEAVE_CLI_HIDE_PROFILES=1`，从帮助信息和 Shell 补全中隐藏 `profile` 命令组，同时依然保留如 `assetiweave-cli profile list` 等显式命令的可执行性，以便进行诊断和自动化脚本调用。

CLI 语法解析错误采用与 Engine 和插件错误完全相同的类型化 JSON 错误契约。输入错误的嵌套命令不再静默以 exit 0 打印帮助信息，Cobra 解析错误也不会被包装为内部故障。错误详情中包含 `command_path`、未识别的 Token、可用的命令或 Flag 列表，以及基于编辑距离排序的匹配建议。

插件需声明其失败策略。安装失败的 `FailOpen` 插件会被跳过并打印警告；`FailClosed` 插件则会中断启动并抛出结构化的 `plugin_install` 错误。任何提供了 `Registrar.Restrict` 规则的插件必须显式声明 `Capabilities.Restricts=true` 且策略为 `FailClosed`；配置不一致将默认 fail-closed，以防止策略插件缺失导致安全边界被静默移除。外部插件可以直接实现 `platform.Plugin` 接口，也可以使用 `platform.NewPlugin(...).Observer(...).Wrap(...).On(...).Restrict(...).Build()` 链式构建。插件还可以声明 `Capabilities.RequiredCLIVersion` 或 `Builder.RequireCLI`；未满足版本约束时遵循插件失败策略，格式错误的约束则直接作为无效插件能力报错。

已安装的插件元数据在启动引导时进行快照，并通过以下命令暴露查看：

```bash
assetiweave-cli config plugins show
```

插件清单包含插件名称、版本、声明的能力、注册的 Observer/Wrapper/生命周期 Hook 以及 `Restrict` 规则。该诊断命令在 CLI 插件策略下始终允许执行，即使在常规业务命令被限制时，运维人员依然可以检查受限的插件状态。

插件可以在 `Install` 期间通过 `Registrar.Config()` 读取本地配置。CLI 默认从 `$HOME/.assetiweave-cli/plugins.json` 加载配置；可通过设置 `ASSETIWEAVE_CLI_PLUGIN_CONFIG` 覆盖该路径：

```json
{
  "plugins": {
    "audit": {
      "endpoint": "https://example.com",
      "enabled": true,
      "batch_size": 50
    }
  }
}
```

公开的配置 API 暴露了复制后的原始 JSON，并提供强类型的 `String`、`Bool` 和 `Int` 辅助读取方法。`config plugins show` 仅报告已配置的 Key 名称，绝不输出具体数值。若插件配置文件损坏，将在安装任何已注册插件前 fail-closed 报错；若没有注册任何插件，该文件将被忽略，内置 CLI 命令正常运行。

`Restrict` 限制规则针对由 Rust 命令契约生成的 Cobra 命令元数据进行评估。友好命令和自动生成的 App 命令继承契约中的风险等级；底层的 `api call` 则被统一视为 `high-risk-write`，因为具体调用的 Engine 方法只有在运行时才能获知。被拒绝的命令会在调用 Engine 之前直接返回 `command_denied`。Observer 仍可监听到被拒绝的调用尝试，但 Wrapper 会在被拒绝的命令上自动绕过，以防止插件篡改或压制拒绝判定。如果父级命令组下的所有可运行命令均被拒绝，则父命令组也会被标记为 `all_children_denied` 拒绝；这避免了在帮助输出和 Shell 补全中展示空命令组。

此 Go 层策略仅作为增量约束使用。它可以使特定构建二进制或嵌入环境下的 CLI 更加严格，但绝无法越权放行会被 Rust 策略、确认门禁或参数校验所拒绝的 Engine 方法。

设置 `ASSETIWEAVE_POLICY_PATH` 可启用 fail-closed 命令策略：

```json
{
  "version": 1,
  "name": "read-mostly-agent",
  "allow": ["overview.*", "profile.*", "skill.*", "schema.*", "system.*"],
  "deny": ["skill.delete", "source.remove"],
  "max_risk": "write"
}
```

策略 Glob 规则同时匹配请求的方法名及其规范方法名。Deny 规则优先评估，随后评估白名单 Allow 列表和最大风险等级限制 `max_risk`。损坏的策略文件会阻止常规命令的执行。诊断方法 `system.version`、`schema.list`、`schema.get` 以及 `doctor.run` 始终保持可用，以便排查和修复无效策略。

Release 构建会自动注入 CLI 产品版本号。`pnpm cli:test:e2e` 会同时启动编译后的 CLI 与 Engine，并验证产品版本对齐、协议兼容性、生成的 App 命令、运行时参数校验、命令策略、调用元数据以及高风险确认流程。

`assetiweave-cli version --check-updates` 会读取远程 Tauri 更新清单，并在 `data.update` 下输出非阻塞的诊断信息。成功的检查会刷新 `$HOME/.assetiweave-cli/update-state.json`。常规的 Release 命令会读取该缓存，并在发现存在更新的 CLI 版本时，向成功与错误 JSON 信封中注入结构化的 `_notice.update` 对象。在 Dev 构建和 CI 环境下该提示会被自动抑制；在自动化脚本中可通过设置 `ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER=1` 予以抑制。测试和隔离环境可以覆盖 `ASSETIWEAVE_UPDATE_STATE_PATH` 和 `ASSETIWEAVE_UPDATE_MANIFEST_URL`。

`assetiweave-cli update --check` 使用相同的更新清单来解析 GitHub Release 下载地址及当前平台对应的 CLI 工具归档包（如 `assetiweave-tools-v0.1.1-macos-arm64.tar.gz`）及其 `.sha256` 校验和。`assetiweave-cli update --yes` 会下载这两个文件，校验 SHA256，解压到临时目录，并原子替换当前运行可执行文件所在目录下的 `assetiweave-cli` 和 `assetiweave-engine`，安装失败时自动回滚。

## 命令参考

```bash
assetiweave-cli overview
assetiweave-cli version
assetiweave-cli settings show
assetiweave-cli settings save --json '{"density":"compact"}'
assetiweave-cli update --check
assetiweave-cli update --yes
assetiweave-cli source list
assetiweave-cli source add --name LocalSkills --path ./skills --dry-run
assetiweave-cli source scan --kind skill
assetiweave-cli profile list
assetiweave-cli asset list --kind skill

assetiweave-cli skill list
assetiweave-cli skill import --from ./downloaded-skill --name downloaded-skill
assetiweave-cli skill search --query "browser automation skill"
assetiweave-cli skill acquire --url https://github.com/lackeyjb/playwright-skill/tree/main/skills/playwright-skill --dry-run
assetiweave-cli skill acquire --url https://github.com/lackeyjb/playwright-skill/tree/main/skills/playwright-skill --yes
assetiweave-cli skill remote list
assetiweave-cli skill remote check [asset-id]
assetiweave-cli skill backup <asset-id>
assetiweave-cli skill mount downloaded-skill --profile codex
assetiweave-cli skill unmount downloaded-skill --profile codex
assetiweave-cli skill delete downloaded-skill --unmount --yes

assetiweave-cli skill group list
assetiweave-cli skill group show <group-id>
assetiweave-cli skill group create --name Frontend --path-glob 'frontend/**'
assetiweave-cli skill group members set <group-id> --asset <asset-id>
assetiweave-cli skill group mount <group-id> --profile codex
assetiweave-cli skill group unmount <group-id> --profile codex --yes
assetiweave-cli skill group exclusive preview --group <group-id> --profile codex
assetiweave-cli skill group exclusive apply --group <group-id> --profile codex --yes

assetiweave-cli schema
assetiweave-cli schema skill.import
assetiweave-cli doctor
```

成功响应以 JSON 信封形式输出到 stdout；错误响应以 JSON 信封形式输出到 stderr。修改状态的命令支持 `--dry-run`；破坏性操作必须显式添加 `--yes`。

自动化脚本可依赖的稳定退出码：

| 退出码 | 含义 |
| --- | --- |
| `0` | 执行成功 |
| `2` | CLI 或 Engine 参数校验失败 |
| `3` | Engine 进程、协议或业务操作失败 |
| `5` | CLI 内部故障 |
| `6` | 命令被策略拒绝或配置的策略文件无效 |
| `10` | 需要显式的高风险操作确认（--yes） |

## 自动生成的 App 领域命令

桌面端的每个 App 命令均暴露为强类型的生成命令：

```bash
assetiweave-cli app list-profiles
assetiweave-cli app create-profile --input @profile.json
assetiweave-cli app delete-source --id <source-id> --yes
assetiweave-cli app execute-plan --plan @plan.json --action-ids '["action-id"]' --yes
```

生成的标量参数被转换为强类型 Flag。对象和数组参数支持内联 JSON、`@file` 文件引用或 `-`（从标准输入 stdin 读取）。命令注册表提供参数 Schema、说明描述、风险等级、dry-run 支持以及确认策略。

## 完整 App API 覆盖

上述快捷命令覆盖了常见的 Skill 工作流。若要调用与桌面端完全对等的全部底层功能，可使用通用的 API 调用命令：

```bash
assetiweave-cli api call <method> --json '<params>'
assetiweave-cli api call <method> --json @params.json
cat params.json | assetiweave-cli api call <method> --json -
```

原始 API 参数必须是一个 JSON 对象。高风险方法会被 Rust Engine 直接拒绝，除非请求中显式包含确认标记。建议使用 CLI 命令行 Flag 以使确认意图清晰可见：

```bash
assetiweave-cli api call delete_source --json '{"id":"source-id"}' --yes
```

`assetiweave-cli schema` 列出了所有可调用的方法。除了快捷方法外，它包含了桌面端 App 所使用的每一个 Tauri 命令：
`get_app_overview`、`list_assets`、`create_source`、`update_source`、`delete_source`、`create_profile`、`update_profile`、`delete_profile`、`update_navigation_model`、`update_app_shortcuts`、`list_asset_mounts`、`toggle_asset_mount`、`set_asset_mount`、`search_skills`、`acquire_skill`、全部 Skill 分组操作、`create_plan`、`execute_plan`、日志查询命令以及 `reveal_path`。

对于 App 领域方法，传入的 JSON 参数结构与前端使用 `invoke` 调用的格式完全一致，例如：

```bash
assetiweave-cli api call list_asset_mounts --json '{"assetId":null}'
assetiweave-cli api call create_profile --json '{"input":{"id":"codex-test","name":"Codex Test","app_kind":"codex","target_paths":["/tmp/codex-skills"],"supported_kinds":["skill"],"deployment_strategy":"symlink_to_source","enabled":true}}'
```

## 互联网 Skill 发现与获取

首个基于 Provider 的检索获取通道内置在共享 Engine 中，以确保桌面端、CLI 和外部 Agent 遵循完全相同的导入规则：

```bash
assetiweave-cli skill search --query "browser automation skill" --provider github --limit 5
assetiweave-cli skill search --query "browser automation skill" --provider github-code --limit 5
assetiweave-cli skill acquire --url <github-repo-or-tree-url> --dry-run
assetiweave-cli skill acquire --url <github-repo-or-tree-url> --yes
assetiweave-cli skill remote list
assetiweave-cli skill remote check [asset-id]
```

`skill search --provider github` 首先进行 GitHub 仓库搜索，随后检查每个候选仓库的文件树中是否包含 `SKILL.md`。当找到具体 Skill 时，候选 URL 会精确指向该 GitHub tree 路径，可直接传给 `skill acquire`。若文件树检查失败或仓库不含 `SKILL.md`，则回退为仓库级别的候选对象。`skill search --provider github-code` 使用带有 `filename:SKILL.md` 限定条件的 GitHub 代码搜索，直接在默认分支上查找 Skill 文件。每个候选对象都包含 `match_reason`；Provider、代码搜索或文件树检查中的问题会在 `warnings` 中返回，以便 Agent 评估结果集的置信度。

支持未经鉴权的 GitHub 请求，但公共 API 的速率限制较低。可设置 `GITHUB_TOKEN` 或 `GH_TOKEN` 环境变量以启用带鉴权的请求头。Token 直接从进程环境中读取，绝不会被写入数据库或 CLI 输出。

`skill acquire --dry-run` 会生成克隆计划、暂存路径、推导的 Skill 路径、导入名称以及 `security_notice` 安全提示，但不写入文件。确认执行的 acquire 会将仓库克隆到 AssetIWeave 暂存区，解析选定的 `SKILL.md` 目录，将其复制到 `~/.assetiweave/library/skills/downloaded`，注册 AssetIWeave 库源并重新扫描，返回导入的资产及相同的 `security_notice`，并将 GitHub 仓库、分支、Skill 路径、获取时的 Tree SHA 和本地内容哈希作为远程源元数据持久化记录。安全提示提醒调用方在导入前务必审查远程 Skill 内容；AssetIWeave 绝不会直接执行或盲目信任远程代码。

`skill remote list` 列出所有已获取 Skill 的远程源记录。`skill remote check` 获取每个记录的当前 GitHub 树状态，比较选定 Skill 目录树 SHA 与已获取的 Tree SHA，并返回 `current`、`changed`、`unknown` 或 `error` 状态。传入 asset id 则仅检查指定的单个 Skill。检查结果会持久化保存 `last_checked_at`、`latest_tree_sha`、`status` 和 `message`，以便桌面端在无需重复实现 Provider 逻辑的情况下展示更新提醒。

这并非一个中心化的托管应用市场：AssetIWeave 在 v1 版本中不进行远程包审核，也不内置 LLM API 服务。它暴露的是一个基于 Provider 的 Agent 操作链，可从 UI、CLI 或外部 AI 工作流中统一驱动。

## 新增 App 业务操作规范

当新增或修改 App 业务操作时，请遵循以下流程：

1. 将共享业务逻辑统一收口在 `AppService` 中。
2. 注册对应的 Tauri Handler。
3. 为其 Rust 请求 DTO 派生 `Deserialize` 和 `JsonSchema`。
4. 在 `src-tauri/src/command_registry.rs` 中绑定该 DTO、精确的风险元数据以及强类型的 `AppService` Handler；字段类型、必填字段、枚举、嵌套 Schema、Engine 分发以及生成的 App CLI Flag 均由该注册派生。
5. 运行 `pnpm cli:contract` 更新契约。
6. 运行 `pnpm cli:test`、`cargo test --workspace` 以及 `pnpm cli:test:e2e`。

Rust 测试会自动对比前端 invoke、Tauri Handler、可执行注册表条目与已提交的 Go 契约。任何同步缺失都会导致 CI 报错。
