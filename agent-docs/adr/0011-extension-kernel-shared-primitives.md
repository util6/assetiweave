# 0011: Extension Kernel 共享底座

> 状态：已接受
> 决策日期：2026-08-19
> 决策证据：`c9b9964`
> 记录日期：2026-08-19

## 决策

Conversation Adapter 与 Agent Market 共享 `backend/extension_kernel/` 的身份、兼容性、信任门禁、进程探测、原子注册表和生命周期键类型；领域 manifest、领域数据库状态和领域业务流程继续由各自模块拥有。

`PackageKind` 保持封闭枚举，新增市场型扩展必须新增领域实现与 ADR，不通过万能 manifest 或热重载扩大内核职责。运行时版本目录保持不可变，安装/升级/移除通过生命周期操作键去重并按资源键冲突。

## 影响

- `TrustGate` 只提供能力判定，不抹平 Conversation 的 `Changed` 等安全状态。
- `RegistrySnapshot<T>` 在锁外构建完整快照后原子替换，读方不持有注册表写锁。
- `ProcessInvocation` 和 `ProbeSpec` 是无损表达层，不替代现有领域 manifest。
- `DomainPackageSystem` 只负责声明 `PackageKind` 并把领域安装目录解释为
  `InspectedPackage`；不再提供空的安装/移除 hook。
- Agent Market 与 Conversation 的安装、升级、卸载和运行时重载由各自的领域
  workflow/registry 负责，Kernel 只提供共享的身份、探测、进程和快照原语。
- 迁移阶段保留旧领域 API 作为行为兼容层，完成接入后按边界检查删除重复注册表和生命周期实现。

## 回滚

若行为等价测试失败，可暂时让领域系统继续使用现有注册表/任务实现，同时保留 kernel 纯类型；不得回滚数据库迁移或删除已写入的 package identity 数据。
