# 任务二：范围与决策门

## P-INPUT — 只接收已验收基线

输入：任务一 accepted revision、依赖锁、公共契约版本、平台与行为测试证据、数据备份/升级结果、保留的 ExtensionKernel/Conversation/ACP 接缝。不得按旧 HEAD 重搭框架，也不从任务一未完成的工作树开始大规模改造。

## P-BUSINESS — 插件能力，而非托管文件

应用插件为宿主提供可执行/可声明的业务能力；Skill/Prompt/Rule 等托管资产不因被扫描就获得应用执行权限。

- AssetType/Scanner：未来资产形态可以变化；识别、元数据与扫描协议可扩展，不在核心写死所有将来类型。
- DeploymentStrategy：隔离计划/检查/执行/撤销的策略责任；内置 symlink 仍为当前唯一批准的默认行为。不为了演示插件先实现未定义 copy/render/config_merge 产品。
- Conversation：现有独立安装、normalized Session/Turn/Part/Question、声明式卡片能力继续使用。外部只提供受协议约束的数据，可信 renderer 归宿主，不加载任意远端 React。
- ACP/Agent：协议接入与协作编排分开。复用现有 AgentExecutionRuntime 与 provider 接缝，Team/MCP/Memory 的业务语义不降格为无约束脚本回调。
- 稳定的通用基础设施直接使用成熟库；领域组合/规则是 AssetIWeave 的价值，不追求把一切代码都交给外部包。

## P-RUNTIME — B01 必须决定的内容

每项必须给出唯一结论、依据和未选方案的代价；“几个方案都可行”不解锁代码实施。

1. 宿主边界：Rust 内置能力与独立插件如何进入同一业务契约；是否复用外部进程、引入成熟 Wasm 宿主或采用其他有证据的方案。
2. 运行时所有权：安装/版本/平台支持/启动/退出/诊断；若需要 Node/Python 等，区分已有 Conversation runtime 与新应用插件 runtime。
3. Manifest 与能力版本：package ID、version、capability ID/version、entrypoint、宿主兼容、未知字段/未知能力行为。复用已有 PackageIdentity/semver，避免第三套包身份。
4. 执行协议：request/response/event/cancel 的实际 schema、大小/超时/并发限制、WireError 映射；现有协议能用的部分复用，不造通用 RPC 产品。
5. 生命周期：安装、校验、启用、禁用、升级、卸载、回退；活动调用持有旧版本租约，禁用拒绝新调用，卸载等待或报 conflict，不能删除正在使用的目录。
6. 权限与数据边界：插件请求能力，宿主检查来源/路径/租户与持久变更。普通子进程限制不描述为 OS 沙箱保证。
7. 代价：真实平台构建、包体、冷启动、内存、运行时分发、维护面的净收益；旧代码具体删除范围。

## P-DECISION — 决策审查

B01 输出写 Issue #23 的 Decision Record 评论：候选矩阵、原型 revision/命令/结果、唯一建议和以上七项结论。需要真实不可逆取舍时按仓库规则新增 ADR，使用仓库当时可用编号，不修改历史 ADR 描述来伪造完成状态。

维护者明确采纳方案后才通过此门；研究型 Agent 的“建议采用”不等于批准。若无明确净收益，保留已有成熟进程/声明式接缝并缩小自研内核，而不是为了实现“插件化”强上新运行时。

## P-IMPLEMENT — Flash/Luna 入场条件

B02 每张施工卡均包含：精确现有路径与 Create 路径、依赖版本、输入/输出签名、真实生产接入点、独立 regression、旧实现删除清单、验证命令、前置关系、数据/协议影响。状态经审查变为 READY 才给执行模型；仅有架构图和工作包标题的卡不进入队列。

## P-DATA — 数据与兼容

不预设整库重写。正常 schema 改动走新增 SQLx migration，历史 migration 不改。需要不兼容转换时单独制作可重复脚本、记录映射损失、用一致旧库快照转新库、运行 integrity_check/foreign_key_check/业务计数关联校验，停写后切换。旧版程序与旧数据库成对回退；新库留存，不自动反向合并。文件/链接/安装包另有备份与回退步骤；AI 可辅助编写映射，正式升级无需临场 LLM。
