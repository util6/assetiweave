# SPEC-BA-04：Agent Market / ACP 安装更新恢复与发布闭环

- 状态：Proposed v1
- 优先级：P0
- 前置：可独立先修复当前故障；最终错误和进程语义分别服从 SPEC-BA-01/03
- 用户场景：在 Agent/ACP 市场点击安装、更新、重装、刷新、卸载
- 当前事故码：`core_incompatible`

## 1. 最终产品决策（覆盖本文旧版兼容门禁设计）

Agent Market / ACP 与 Conversation Script Market 采用不同版本策略：

1. `catalogVersion`、Agent `version`、运行时探测版本只作为当前观测值和诊断信息保存。
2. AssetIWeave core 版本、Catalog revision、Agent version equality 不得阻断 install/update/reinstall。
3. 不维护 ACP 版本变更历史；安装记录只保存当前观测版本。
4. 同版本 update 允许执行，语义等同于重新物化并原子替换当前安装。
5. 真正门禁只保留：平台/运行时可用性、分发内容、完整性、信任确认、ACP protocol conformance。
6. `coreCompatibility` 仅作为 v1 wire/catalog 的兼容字段保留；可缺省、不可用于按钮或后端门禁。
7. preview token 绑定 Agent id、action 和分发内容，不绑定纯版本元数据。

后文若出现“core incompatible 必须禁用”“当前 core 必须落入 Catalog range”等旧要求，均由本节覆盖。
Conversation Script Market 的 runtime/core compatibility 规则不受本决策影响。

## 2. 本次失效根因

已确认四个独立问题：

1. 后端 preview 与 lifecycle worker 都执行 core range 门禁；旧 cache 的 `0.5.x` range 会直接返回
   `core_incompatible`，请求甚至不会进入下载。
2. Rust 主机名为 `macos`，生产 Catalog target 为 `darwin`；未归一化导致 OpenCode binary
   被误判 `distribution_unsupported`。
3. preview/start 对 catalogVersion、agentVersion 做精确相等校验，并拒绝同版本 update；这把
   观测元数据错误提升为生命周期一致性条件。
4. Go CLI preview 后调用 `agent.install.run` 时漏传必需的 `action`，确认安装稳定失败为
   `invalid_params`。

## 3. 恢复目标

- 旧 cache、core 升级或仅版本元数据变化不影响 ACP install/update/reinstall。
- macOS/Apple Silicon 能正确选择 `darwin/aarch64` binary。
- 下载仍校验 HTTPS、size、SHA256、archive layout 和 executable path。
- preview token 在 URL/hash/package/bin/launch args 等分发内容变化时失效，纯版本展示字段变化时不失效。
- GUI 与 CLI 复用同一 AppService/lifecycle workflow。
- 安装与同版本 update 均完成 materialize → ACP conformance → atomic activation → runtime reload。

## 4. ACP 版本语义

### 4.1 观测而非约束

`AgentInstallation.agent_version` 与 `catalog_version` 保存当前安装的来源信息；
`ProcessInvocation.version_req` 对 Agent 必须为 `None`。运行时 ready 的权威判据是可执行文件存在、
probe 可运行且 ACP handshake 通过。

### 4.2 Catalog cache

active Catalog 只按合法 revision、新旧顺序和同 revision 内容碰撞规则选择，不按 core range 过滤。
纯版本元数据不得要求重新生成发布证据；分发内容或完整性变化仍必须更新证据。

### 4.3 前端

`coreCompatible/coreCompatibility` 是 v1 兼容字段，前端不得据此禁用生命周期按钮；
只有没有 selectable distribution 或存在真实 lifecycle conflict 时才禁用。

## 5. Catalog 版本与选择算法

### 5.1 Catalog revision

`catalogVersion` MUST 使用可比较的固定格式：

```text
YYYY.MM.DD.N
```

解析为：

```rust
struct CatalogRevision {
    date: NaiveDate,
    sequence: u32,
}
```

`latest`、空值、不可解析字符串必须被拒绝。不得用普通字符串大小比较替代解析。

### 5.2 候选来源

```rust
enum CatalogOrigin {
    Bundled,
    Cache,
    Remote,
}

struct CatalogCandidate {
    origin: CatalogOrigin,
    catalog: CatalogService,
    revision: CatalogRevision,
}
```

### 5.3 active catalog 选择

每次选择必须执行：

1. 分别读取 bundled 和 cache；单个候选损坏不得阻止另一个候选使用。
2. 验证 schema、revision、每个 item、distribution 与安全字段。
3. 计算候选对当前核心版本是否至少包含一个兼容 item。
4. 淘汰“不含兼容 item”的 cache，除非调用明确请求历史/不兼容浏览模式。
5. 在剩余候选中选择 revision 最大者。
6. revision 相同时优先 cache/remote（允许更新 ETag metadata），但内容 hash 不同必须记录
   `catalog_revision_collision` 并优先 bundled，不能静默接受同版本不同内容。
7. 返回 active catalog 时附带 origin、revision 和 content hash，供 doctor/日志展示。

普通 `list_agent_market(includeIncompatible=true)` 可以列出 active catalog 内不兼容 item，
但 active catalog 本身必须适用于当前 core。

### 5.4 Remote refresh

Remote refresh 必须：

- 仅允许 HTTPS 和当前 allowlist host。
- 限制重定向最终 host、总字节数与 deadline。
- 先写临时文件，完整校验后原子 rename。
- 检测 revision rollback；低于现有 active revision 默认拒绝。
- 检测相同 revision 不同 content hash。
- 写入 cache 后重新执行 active selection，而不是假定新下载一定 active。
- 返回 `activeCatalogVersion` 与 `downloadedCatalogVersion`，两者可以不同。

ETag 只是缓存优化，不是内容可信或版本新旧的证明。

## 6. Production Catalog 准入

### 6.1 禁止字段和值

发布用 Catalog MUST NOT 包含：

```text
fixture
example.com
localhost
127.0.0.1
重复占位 hash（如全 a/全 0）
latest、*、未固定 package 版本
不存在的 evidence ID
```

测试目录必须放在 test fixture 文件中，不得复用生产
`builtin-assets/agent-market/catalog-v1.json`。

### 6.2 Distribution 真实性

每个 managed distribution 必须有：

- 固定 package/artifact 版本。
- 实际存在的 package 或 HTTPS artifact。
- 正确 bin/executable。
- 实际 ACP launch args。
- 与下载内容一致的 SHA256（binary）。
- 可验证的 size 上限（binary）。
- 支持的 OS/arch。
- 由 CI 或发布审计生成的 evidence 记录。

System-only item 可以没有下载分发，但 UI 必须显示“需要本机运行时”，不能展示“下载”语义。

### 6.3 Verification evidence

`verification.status = tested` 必须关联一个可追溯 evidence artifact，至少记录：

```text
catalog revision
core version
agent version
distribution id
OS/arch
install command outcome
ACP initialize outcome
session/new outcome
cleanup outcome
timestamp
```

没有 evidence 时只能标记 experimental。

## 7. Preview 与生命周期协议

### 7.1 Preview

`preview_agent_installation` 必须按顺序校验：

1. active catalog/item 存在。
2. 请求的 catalog/agent version 仍 active。
3. action 合法。
4. core compatibility。
5. distribution candidates。
6. 当前 installation 与 action 状态。
7. agent-in-use conflict。
8. trust/verification warning。
9. 生成绑定 catalog revision、item version、distribution、action 的 preview token。

返回错误必须为 typed `AgentMarketErrorView`，不依赖字符串解析。

### 7.2 Start

Start 必须重新验证 preview 的全部安全不变量，不能信任前端传回的数据。Preview token 只允许
对应的 action，不得像当前实现一样让 install token 被 update/reinstall 三选一匹配逻辑模糊接受。

推荐 token 输入：

```text
tenant_id\0catalog_content_hash\0item_id\0item_version\0distribution_id\0action
```

### 7.3 Update

- Update 必须要求已有 installation 且 catalog agent version 更新。
- 安装新版本到新 immutable directory。
- 新版本 conformance 成功后才切换 DB/registry active pointer。
- 切换失败保留旧版本。
- 新版本激活成功后再清理旧 managed directory；清理失败作为 warning，不回滚成功激活。

## 8. ACP conformance

Managed ACP Agent 激活前必须完成：

```text
resolve executable
→ availability/version probe
→ spawn process tree
→ ACP initialize
→ session/new（不发送真实用户 prompt）
→ optional model discovery
→ close/shutdown
→ reap process and remove temp workspace
```

每一步必须有独立错误码；权限请求、tool activity、空响应、timeout、output limit 和清理失败
均失败闭合。System-owned runtime MAY 记录 broken 状态而不删除系统文件。

## 9. Frontend 契约

### 9.1 DTO 不得丢字段

`AgentCatalogItem` 必须增加：

```ts
coreCompatible: boolean;
coreCompatibility: {
  min: string;
  maxExclusive: string;
};
installability: "installable" | "runtime-required" | "core-incompatible" | "unsupported";
```

`marketItemToCatalogItem` 必须完整映射。

### 9.2 操作按钮

| 状态 | Install | Update | Reinstall | Uninstall |
|---|---:|---:|---:|---:|
| core incompatible | disabled | disabled | disabled | enabled（若已安装） |
| no selectable distribution | disabled | disabled | disabled | enabled |
| system runtime available | enabled/attach | enabled（有新版） | enabled | enabled |
| managed distribution available | enabled | enabled（有新版） | enabled | enabled |
| lifecycle running | conflicting actions disabled | disabled | disabled | disabled |

不兼容提示必须本地化，展示当前 core 和要求区间。不得只显示原始英文后端错误。

### 9.3 刷新后状态

刷新 Catalog 后：

- 重新加载 market list。
- 保留当前筛选和滚动位置。
- 若 active catalog 未变化，明确显示 downloaded-but-not-active 原因。
- 不因一次刷新失败清空当前可用 bundled/cache catalog。

## 10. Release gate

新增 `scripts/check-agent-catalog-release.*`，并接入 CI/发布审计：

```text
CAT-01 三处应用版本一致
CAT-02 bundled catalog revision 可解析
CAT-03 bundled production item 全部兼容当前 core
CAT-04 不含 fixture/placeholder
CAT-05 每个 managed distribution 固定版本
CAT-06 binary URL/size/hash 合法且非占位
CAT-07 evidence ID 可解析并存在
CAT-08 至少一个 ACP item 在受支持 CI host 上有 selectable distribution
CAT-09 Catalog DTO schema/Engine contract 已同步
CAT-10 remote publication dry-run 与 bundled content hash 一致
```

涉及真实网络存在性的 CAT-06/07/08 可在 release job 执行；普通 PR CI 至少执行静态校验和
本地 fixture conformance。

## 11. 缓存迁移

修复版本首次启动时：

1. 读取旧 cache，不立即删除。
2. 若 cache 对当前 core 无兼容 item，标记 meta `inactiveReason=core_incompatible`。
3. 选择 compatible bundled catalog。
4. 后台尝试 remote refresh。
5. 新 remote 通过校验后原子替换 cache。
6. doctor 展示 active origin/version 和被拒 cache 的原因。

不得把“让用户手工删除 `~/.assetiweave/cache`”作为正式迁移方案。

## 12. 测试要求

### 后端

1. `bundled_catalog_items_support_current_core_version`
2. `version_0_6_1_matches_0_6_to_0_7_half_open_range`
3. `incompatible_cache_does_not_mask_compatible_bundled_catalog`
4. `newer_compatible_cache_beats_bundled_catalog`
5. `same_revision_different_hash_fails_closed`
6. `remote_revision_rollback_is_rejected`
7. `production_catalog_rejects_fixture_and_placeholder_values`
8. `preview_token_is_bound_to_exact_action`
9. `failed_update_preserves_previous_active_installation`
10. `successful_update_switches_registry_after_conformance`

### 前端

1. `market_mapping_preserves_core_compatibility`
2. `incompatible_agent_disables_install_update_and_reinstall`
3. `incompatible_agent_still_allows_uninstall`
4. `catalog_refresh_preserves_current_catalog_on_failure`
5. `core_incompatible_message_shows_current_and_required_versions`

### 端到端

使用本地可执行 fixture，不访问公网：

```text
install → preview → confirm → task progress → conformance → ready
update success → new active version
update failure → old version remains ready
cancel download/install → staging removed
restart → installation recovered into Agent registry
```

## 13. 验收标准

- 当前 `0.6.1` 不再把 bundled ACP item 全部判为 incompatible。
- 本机已有旧 cache 不会遮蔽 compatible bundled/remote catalog。
- production catalog 不含 fixture、example.com 或占位 hash。
- 前端不再允许点击已知不兼容的安装/更新按钮。
- 至少一个真实 ACP managed distribution 通过 release conformance evidence。
- 安装、更新、重装、卸载和恢复路径均有成功及失败端到端测试。
