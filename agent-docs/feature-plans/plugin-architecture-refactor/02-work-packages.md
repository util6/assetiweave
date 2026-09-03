# 任务二：工作包与删除目标

这些是已确认的施工范围与验收结果，不是绕过 B01/B02 的编码指令。源码路径为任务一之前的定位参考；B00 重新解析到验收后的实际位置，B02 将每个工作包拆成可单独运行测试的卡。

| 包 | 接缝（相对 src-tauri/src/backend） | 必须交付 | 删除/保留标准 |
|---|---|---|---|
| W1 共享包与生命周期 | `extension_kernel/{identity,lifecycle,registry,launcher,trust}.rs`、`agents/registry.rs`、`agent_market/`、`conversations/package.rs` | 内置/独立包身份、兼容验证、安装/升级/启停/回退共享机制；runtime 选型由 B01 锁定 | 删除两市场重复机制；保留 Agent/Conversation 各自业务校验，不以一个巨型 PluginManager 替代两个巨型模块 |
| W2 能力注册与资产扫描 | `models/assets.rs`、`scanner/{dispatcher,detector,asset_builder}.rs`、`target_catalog.rs`、`targeting.rs` | 一个独立测试插件提供已定义扫描能力，新增资产种类/元数据按协议进入同一 catalog；宿主不重编译 | 消除必须改核心 match/固定 Detector 列表才能扩展的路径；SQLite/来源只读与 DTO 校验保留；不把 Custom enum 误认动态注册 |
| W3 部署策略边界 | `planner/{builder,mod}.rs`、`executor/{deployment,mod}.rs`、mount 相关 application workflow | 当前 symlink 作为同契约内置策略，plan/check/apply/revoke 责任清楚；测试替身能被注入 | 删除上层对 symlink 具体实现的重复分支，保留路径/租户/冲突/锁和 asset_mounts authority；不激活未经定义的新部署行为 |
| W4 Conversation 治理接入 | `conversations/{external,package,readers}.rs`、`application/conversation_adapters.rs`、前端可信卡片注册 | 现有独立包继续安装/解析/声明卡片，经共享生命周期治理，不另造解析/卡片协议 | 删除重复安装治理 glue；保留 ADR0013 内容节点身份、membership、可信 renderer 与已有包兼容策略 |
| W5 ACP/Agent 协作接缝 | `ai_execution/`、`agents/`、Team application workflow | Agent protocol transport 可替换，编排能力通过已冻结契约接入；同样的取消/事件/错误/租户语义 | 删除协议与 Team 编排的非必要耦合；保留真实协作业务与 MCP 权限，不把 ACP 握手视作完整 Team 插件 |
| W6 收口与回退演练 | 各包生产入口、Engine/Tauri/CLI、migrations、现有测试 | 生产真的使用新能力路径，独立安装无需发布宿主，全部旧路径删除或明确兼容期到期；恢复演练通过 | 不留第二套 registry/task/store 权威，不遗留仅供 demo 使用的插件骨架 |

## 顺序

B00 → B01 → 决策审查 → B02 → W1 → W2/W3/W4/W5 的依赖明确的小卡 → W6。

默认串行。W2/W3 共享资产 DTO/plan；W1/W4 共享安装治理；W5/Memory/Team 共享运行时和事件。只有 B02 写明文件所有权且维护者安排并行时才拆多个执行者。不要以“不同领域”推断没有代码冲突。

## 每包统一交付五联表

`现有机制 → 成熟库/已有机制 → 生产能力入口 → 删除项 → 行为证据`。

若保留自有接口，注明其领域规则、输入/输出和为什么成熟库不提供该业务语义；不是再次制作通用容器、配置管理、日志系统或查询调度器。
