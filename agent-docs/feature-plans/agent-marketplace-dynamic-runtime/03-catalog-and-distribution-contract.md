# 目录与分发契约 (Catalog and Distribution Contract)

| 字段 | 值 |
|---|---|
| 状态 | Proposed |
| Catalog Schema | `assetiweave.agent-market/v1` |
| 上游兼容 | ACP Registry `binary`、`npx`、`uvx`；AssetIWeave 扩展 `system` |

## 1. 设计目标

本契约解决三个独立问题：

1. **Catalog**：哪些 Agent/版本经过 AssetIWeave 精选并可展示。
2. **Distribution**：该 Agent 如何在当前平台被绑定或物化。
3. **Resolved Runtime**：安装完成后，执行器实际启动哪个本地 program 和 argv。

三层不得合并。尤其禁止把 `npm package` 当作所有 Agent 的统一模型，也禁止执行器直接解释远程 distribution。

## 2. Catalog 来源和优先级

### 2.1 数据链

```text
Official ACP Registry / Vendor docs
        ↓ curator sync + validation + smoke evidence
AssetIWeave Curated Index（固定版本）
        ↓ release
Bundled Catalog + Remote Curated Catalog
        ↓ schema validate + ETag cache
Client Market View
```

客户端数据优先级：

1. 当前进程内已验证 catalog。
2. 磁盘上的最后有效 remote cache。
3. 随应用发布的 bundled catalog。

网络刷新失败、HTTP 304、Schema 不兼容或内容无效均不得清空当前有效 catalog。

### 2.2 Catalog 缓存

建议路径：

```text
~/.assetiweave/cache/agent-market/catalog-v1.json
~/.assetiweave/cache/agent-market/catalog-v1.meta.json
```

`meta` 至少保存：

- `etag`
- `fetched_at`
- `source_url_id`（稳定别名，不保存带 token URL）
- `schema_version`
- `catalog_version`

写入顺序：下载到临时文件 -> 大小限制 -> JSON/schema/semantic validate -> fsync/close -> 原子 rename。缓存本身不需要递归 hash 或 trust 状态。

## 3. 顶层 Schema

### 3.1 规范性 JSON 示例

```json
{
  "schema": "assetiweave.agent-market/v1",
  "catalogVersion": "2026.08.16.1",
  "generatedAt": "2026-08-16T00:00:00Z",
  "source": {
    "kind": "assetiweave_curated",
    "upstream": "agentclientprotocol/registry",
    "upstreamRevision": "GIT_REVISION"
  },
  "items": [
    {
      "id": "opencode",
      "displayName": "OpenCode",
      "description": "ACP Agent",
      "protocol": "acp",
      "version": "PINNED_VERSION",
      "coreCompatibility": {
        "min": "APP_CORE_MIN",
        "maxExclusive": "APP_CORE_MAX"
      },
      "capabilities": {
        "purposes": ["card_translation", "memory", "prompt_optimization"],
        "textPrompt": true,
        "modelDiscovery": true
      },
      "verification": {
        "status": "tested",
        "testedAt": "2026-08-16T00:00:00Z",
        "evidenceId": "CI_EVIDENCE_ID"
      },
      "upstream": {
        "registryId": "opencode",
        "homepage": "UPSTREAM_URL",
        "license": "UPSTREAM_LICENSE"
      },
      "distributions": [
        {
          "id": "system-opencode",
          "type": "system",
          "priority": 10,
          "commandCandidates": ["opencode"],
          "versionArgs": ["--version"],
          "versionRange": "SUPPORTED_RANGE",
          "launchArgs": ["acp"]
        },
        {
          "id": "binary-darwin-arm64",
          "type": "binary",
          "priority": 20,
          "target": { "os": "darwin", "arch": "aarch64" },
          "archive": "tar.gz",
          "url": "ARTIFACT_URL",
          "sha256": "ARTIFACT_SHA256",
          "size": 12345678,
          "executable": "opencode",
          "launchArgs": ["acp"]
        }
      ]
    }
  ]
}
```

示例中的占位值由 catalog 发布流程填充；客户端不得接受空占位值。

### 3.2 顶层验证

客户端 MUST 拒绝：

- 未知 major schema；
- 重复 `items[].id`；
- 重复 `distributions[].id`；
- 非固定 Agent version；
- 生成时间无法解析；
- item 没有任何合法 distribution；
- 未知 protocol/distribution type；
- compatibility 明确排除当前 core；
- 超过 5 MiB 的 catalog 响应；
- catalog 中出现 raw secret、shell string 或 lifecycle hook。

客户端 MAY 忽略当前平台不匹配的合法 distribution，但 item 仍可显示为“当前平台不可安装”。

## 4. Agent Market Item 契约

| 字段 | 类型 | 必填 | 约束 |
|---|---|---:|---|
| `id` | string | 是 | `^[a-z][a-z0-9-]{0,63}$`，稳定且不随版本变化 |
| `displayName` | string | 是 | 1–120 bytes，无 NUL |
| `description` | string | 是 | 1–500 bytes，纯展示文本 |
| `protocol` | enum | 是 | `acp` / `native` |
| `version` | string | 是 | 精确版本；禁止 `latest`、范围、tag |
| `coreCompatibility` | object | 是 | 当前 app core 必须落入范围 |
| `capabilities` | object | 是 | 仅声明核心已知 capability |
| `verification.status` | enum | 是 | MVP 为 `tested` / `experimental`；默认只推荐 tested |
| `verification.evidenceId` | string | tested 时是 | 指向发布 CI/smoke 证据 ID |
| `upstream` | object | 是 | 来源、主页、license；仅展示/追踪 |
| `distributions` | array | 是 | 至少一个；按 selection 算法处理 |

`capabilities.purposes` 只能取核心定义的 `AiExecutionPurpose` 映射。远程目录不能创造可执行的任意 capability 名称；未知值拒绝 catalog，而不是静默透传。

## 5. Distribution 联合类型

### 5.1 System

```text
SystemDistribution {
  id
  type = system
  priority
  supported_targets?[]
  command_candidates[]
  version_args[]
  version_range
  launch_args[]
  model_discovery_args?[]
}
```

约束：

- 只允许 executable name/path candidate + argv array；禁止 shell string。
- `command_candidates` 由精选索引维护，不接受 UI 输入。
- 解析必须使用现有 host executable resolver；持久化解析后的 program。
- version probe 超时默认 8 秒，stdout 1 MiB、stderr 256 KiB。
- version 无法解析或不满足 range 时不可选为 compatible System。
- System distribution ownership 固定为 `system`。
- System 不提供 artifact hash；绑定和卸载不修改其文件。

### 5.2 Binary

```text
BinaryDistribution {
  id
  type = binary
  priority
  target { os, arch }
  archive = zip | tar.gz | tgz | tar.bz2 | tbz2 | none
  url
  sha256
  size?
  executable
  launch_args[]
  model_discovery_args?[]
}
```

约束：

- `url` MUST 使用 HTTPS，并来自发布配置允许的 host。
- `sha256` MUST 是 64 位小写 hex。
- `executable` MUST 是归档内相对路径，无 `..`、绝对路径或 NUL。
- target 必须与 Rust 规范化的 OS/arch 完全匹配。
- 下载完整 artifact 后、解压前校验 SHA-256。
- 解压拒绝 path traversal、绝对路径、symlink、hardlink、device/FIFO。
- 解压完成后 program 必须是普通文件；Unix 设置最小必要 executable bit。

### 5.3 Npx

Catalog 类型沿用 ACP Registry 的 Npx 分发语义，但安装后不使用临时 `npx`：

```text
NpxDistribution {
  id
  type = npx
  priority
  package
  version
  bin
  launch_args[]
  node_range?
  lifecycle_scripts = deny
  model_discovery_args?[]
}
```

约束：

- `version` 必须与 item fixed version 或显式 package version mapping 一致。
- `package` 支持合法 npm package name，包括 scope；禁止 URL、file、git、workspace spec。
- `bin` 是包声明的固定入口名；安装后必须解析到 staging 内 `.bin` 或实际入口。
- MVP `lifecycle_scripts` 固定 `deny`。需要 install/postinstall 的新包必须先通过架构和安全评审，不得由 catalog 自行启用。
- Node/npm 必须由 host 提供并满足 `node_range`；缺少时返回 `runtime_missing`。

参考安装命令结构：

```bash
npm install \
  --prefix STAGING_ROOT \
  --save-exact \
  --omit=dev \
  --ignore-scripts \
  --no-audit \
  --no-fund \
  PACKAGE@VERSION
```

安装器 MUST：

1. 固定 package/version。
2. 保存生成的 `package-lock.json` 和 npm 版本。
3. 检查 lock 中目标 package 的 resolved version 和 `integrity`。
4. 将 runtime program 解析到 `STAGING_ROOT/node_modules/.bin/BIN` 或经校验的包内入口。
5. 激活后使用该本地入口，禁止 `npx -y`。

### 5.4 Uvx

`uvx` 在官方 Registry 中表示 Python tool 分发；AssetIWeave 安装必须物化为持久化 uv tool：

```text
UvxDistribution {
  id
  type = uvx
  priority
  package
  version
  command
  launch_args[]
  python_range?
  model_discovery_args?[]
}
```

约束：

- package 必须是允许的 PyPI project name；禁止 URL、git、本地路径和 editable。
- version 必须精确，运行 spec 为 `PACKAGE==VERSION`。
- host 必须已有可用 `uv`；应用不自动安装 uv/Python。
- 安装时设置 app-owned `UV_TOOL_DIR` 和 `UV_TOOL_BIN_DIR`，不得污染用户全局 uv tools。
- 激活后 program 指向 app-owned bin；禁止运行临时 `uvx`。

参考安装结构：

```bash
UV_TOOL_DIR=STAGING_ROOT/tool \
UV_TOOL_BIN_DIR=STAGING_ROOT/bin \
uv tool install PACKAGE==VERSION
```

安装器记录：uv 版本、精确 package version、tool metadata、resolved command。执行时不调用 `uv tool install` 或 `uvx`。

## 6. Catalog 到官方 ACP Registry 的映射

| ACP Registry | Curated Index | 说明 |
|---|---|---|
| agent name/id | `item.id/displayName` | ID 经过稳定性审查 |
| version | `item.version` | 固定到精选版本 |
| binary target | Binary distribution | 保留 URL/hash/target，增加资源与 host policy |
| npx package/args | Npx distribution | 增加 exact version/bin/runtime policy |
| uvx package/args | Uvx distribution | 物化为 app-owned persistent uv tool |
| 未定义 System | System distribution | AssetIWeave 扩展，来自 Vendor 官方 CLI 文档 |

同步器不得原样复制未知字段到执行 definition。未知上游字段记录为维护告警；只有 schema mapper 明确支持的字段进入精选索引。

AssetIWeave 内部 `item.id` 必须保持现有 assignment 稳定，不要求与上游目录 ID 相同；上游 ID 单独存入 `upstream.registryId`。首批至少显式映射：

| AssetIWeave ID | Upstream Registry ID |
|---|---|
| `claude` | `claude-acp` |
| `codex` | `codex-acp` |
| `pi` | `pi-acp` |

同步器不得因上游命名差异静默重命名内部 ID，否则会破坏 capability assignments 和 installation 主键。

## 7. 分发选择算法

### 7.1 输入

- Market item fixed version。
- 当前 OS/arch。
- host runtime observations（Node/npm/uv）。
- compatible System probe results。
- 用户显式 distribution choice（可空）。
- product policy 和资源限制。

### 7.2 输出

```text
DistributionCandidate[] {
  distribution_id
  type
  selectable
  recommended
  ownership
  reason_code?
  required_runtime?
  resolved_version?
  download_size?
  target_path?
}
```

### 7.3 确定性流程

```text
1. 删除 target 不匹配或 core incompatibility 的候选。
2. 对 System 执行 bounded version probe；只有版本满足 range 才 selectable。
3. 对 Npx 检查 Node/npm；对 Uvx 检查 uv；缺失则保留展示但 selectable=false。
4. 对 Binary 验证 target、URL policy、hash、size metadata。
5. 按默认 type rank：System(10) < Binary(20) < Npx(30) < Uvx(40)。
6. 同 type 按 catalog priority，再按 distribution_id 稳定排序。
7. 第一个 selectable 为 recommended。
8. 若用户显式 choice 不 selectable，返回对应错误，不自动换候选。
9. 若用户未显式 choice，UI 预选 recommended，但提交前必须展示确认预览。
```

Catalog 的 `priority` 只能在同一安全/兼容等级内排序，不能使不兼容分发可选。

## 8. 安装目录契约

默认根：

```text
~/.assetiweave/agent-runtimes/
├── .staging/
│   └── TASK_ID/
└── AGENT_ID/
    └── VERSION/
        └── DISTRIBUTION_ID/
            └── INSTALLATION_ID/
```

约束：

- runtime root 必须由 `app_settings`/app home 统一解析；数据库可保存规范化绝对运行路径，UI 使用 `~` 显示。
- `AGENT_ID/VERSION/DISTRIBUTION_ID` 必须来自已验证字段，不能包含分隔符或 `..`。
- `INSTALLATION_ID` 是本地生成的唯一 identity，不来自 catalog。即使同版本重装也创建新目录，使旧 Registry definition 在 swap 前始终指向未改变的旧路径。
- staging 和 active 目标必须位于同一文件系统，以支持原子 rename；若配置无法满足，preflight 返回 `atomic_activation_unavailable`。
- managed installation 只能执行其 active directory 内入口。
- System installation 的 `install_dir` 为 null，`resolved_program` 为解析后的外部入口。

## 9. 完整性与变更模型

### 9.1 必须维护

| 分发 | 安装时证据 | 启动时检查 |
|---|---|---|
| Binary | artifact SHA-256、source、size | program 存在且为普通可执行文件 |
| Npx | exact spec、package-lock、目标 package integrity、npm/node version | local bin/entry 存在 |
| Uvx | exact spec、uv/tool metadata、resolved command | local command 存在 |
| System | resolved path、version output 摘要、checked_at | path 可解析/存在；显式检查时重跑 version |

### 9.2 明确禁止

- 每次启动/执行递归 hash 安装目录。
- `installed_content_hash` / `trusted_hash` 双 hash。
- `trusted` / `changed` / `untrusted` 状态。
- 把本地修改检测描述为代码签名或安全边界。

入口丢失或启动失败时设置 `broken`/health failure，并提供重装。应用不尝试推断目录中哪一个文件被用户或其他程序修改。

## 10. 环境变量和参数策略

Catalog 可以声明：

- 经过 schema 允许的固定 argv；
- 核心已知的环境引用 ID，例如 `auth.openai_api_key`；
- 非敏感常量键，且键必须在 allowlist。

Catalog 禁止声明：

- raw secret value；
- `PATH`、动态 loader、proxy、shell startup 等核心敏感变量覆盖；
- shell command/string interpolation；
- 任意安装 hook；
- 关闭 permission、MCP、workspace、日志或资源限制的选项。

运行时由核心将引用解析为当前 secret/config 值；持久化 definition 只存引用和允许的常量，不存 secret。

## 11. 首批 Catalog 映射要求

| Agent | Protocol | 首选候选 | 其他候选 | 迁移要点 |
|---|---|---|---|---|
| OpenCode | ACP | compatible System | official Binary | 单 item；args=`acp` |
| Gemini | ACP | compatible System 或 Npx（由精选结果决定） | Npx/System | 不再硬编码本地假设 |
| Kiro | ACP | System | 无/后续 | 使用经官方核对的 `kiro-cli acp` |
| Antigravity | Native | System | 无 | 保留 Native backend，不伪装 ACP |
| Claude | ACP | Npx | compatible System 可后续加入 | 安装时固定包版本 |
| Codex | ACP | Npx | compatible System 可后续加入 | Npx 包可携带平台二进制，仍按 Npx installer 管理 |
| Hermes | ACP | compatible System | Uvx | Uvx 物化为 persistent uv tool |
| Pi | ACP | Npx | 无 | 安装时固定包版本 |
| Qoder | ACP | compatible System | Npx | 当前 Vendor 命令为 `qoder --acp`，官方 Registry 另有固定 `@qoder-ai/qodercli` Npx 分发；旧 System `qodercli` 只可作为经版本探测验证的 legacy candidate |

精确版本不得从本规格抄写旧 hardcoded 值；由 catalog 发布时基于官方上游和 smoke 证据固定。

## 12. Catalog 发布质量门

每个新增/升级 item 必须：

1. 通过 JSON Schema 和 semantic validator。
2. 证明来源、license、固定版本和 core compatibility。
3. 所有 Binary target hash 可复算。
4. Npx/Uvx spec 不含范围、URL/Git/path。
5. 在支持平台完成 install + conformance + clean shutdown smoke。
6. 记录 evidence ID 和测试日期。
7. 对现有 item 做重复 ID、分发选择和 offline fixture 回归。
8. 先更新 bundled catalog，再发布同版本 remote catalog；应用不得 bundle 一个不存在的 schema major。
