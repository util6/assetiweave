# 任务二：独立验收矩阵

| Gate | 通过证据 |
|---|---|
| P-INPUT | #22 A-G01 已接受；revision、锁版本、现有测试与工作区成果明确 |
| P-DECISION | 运行时/协议/版本/生命周期七项有唯一已采纳结论；可复现候选实验；不是仅列流行库名称 |
| P-IMPLEMENT | B02 代码卡通过完整性审查；输入输出、生产路径、删除项、测试与依赖无空缺 |
| P-EXTENSION | 同一个稳定宿主构建先运行内置能力，再安装独立插件获得同类新能力；记录宿主二进制 hash 前后一致 |
| P-LIFECYCLE | 未知协议/能力拒绝清晰；重复 ID 明确；禁用后新调用被拒绝；活动调用持旧版本完成/取消；升级失败恢复旧版本；卸载不破坏使用中包 |
| P-BUSINESS | 源只读、挂载意图和直接 symlink、Conversation 规范记录/卡片身份、Team/Memory/ACP 取消/回放/租户回归 |
| P-DATA | 新旧库/文件副本演练，完整性/外键/领域关联验证，停写切换与成对回退；不以 SQL 执行成功代替数据保真 |
| P-FINAL | W1–W6 覆盖，旧硬编码/重复治理删除，现有前端/Rust/Go/适配器测试、CLI、桌面及支持平台证据齐全 |

## 必须包含的场景

1. 插件提供已有 capability version 的新实现，无需主程序重新编译；宿主不认识的新 capability 返回可诊断结果，不静默当作其他资产。
2. 两个插件冲突的 ID/优先级有确定规则和断言；禁用/重启后选择可重复。
3. 正在扫描/安装/ACP 调用时尝试升级/卸载，按租约规则等待或明确 conflict；不把正在执行的文件删掉。
4. 错误响应、超大响应、超时、取消、进程退出均进入 AppError/TaskRuntime 正确终态；日志不污染 Engine stdout。
5. 内置实现也从同一能力契约进入；禁用默认能力时给明确缺能力结果，不悄悄绕过注册表跑硬编码路径。
6. 非可信插件输出作为数据解析；不能绕过来源只读、租户、路径与持久变更宿主检查。

## 命令基线

B02 按实际施工新增定向测试；最终至少保留并运行：

```sh
pnpm lint
pnpm format:check
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
pnpm check:boundaries
pnpm test:boundaries
pnpm cli:contract
pnpm gen:surface-matrix
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
pnpm conversation-adapters:test
```

CLI e2e 必须指定当前工作树构建的 CLI/Engine，桌面运行使用隔离 DB/源/目标。记录测试过滤匹配数、平台和退出码；未执行平台不标通过。所有命令通过仍需要独立安装插件的生产路径测试，不以纯内存 registry 单测冒充插件交付。
