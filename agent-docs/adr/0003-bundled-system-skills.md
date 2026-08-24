# 0003: 将产品自有的 Skill 打包为只读系统资产

> 状态：已接受
> 决策日期：2026-07-22
> 决策证据：`c666f517feb31fa9085eb800ee86ae06b28c8fa2`
> 记录日期：2026-07-22

## 背景

AssetIWeave 中的 Skill 具有双重属性：既是可以挂载到外部 AI 应用中的目录资产（Catalog Asset），也是产品工作流中面向 Agent 的稳定指令集。产品自有的内置 Skill（例如会话整理与 Memory）必须与 Rust Engine 契约保持同步，并随应用程序一同分发。

现有的 `assetiweave-library-skills` 源是一个租户作用域下的用户备份库。其根目录可以被修改或迁移，其内容可以被导入、备份或删除。这些生命周期规则与受应用程序严格控制的系统资产是不兼容的。

## 决策

- 产品自有的内置 Skill 源码存放在 `builtin-assets/skills/` 目录下。
- 将其文件同时编译打包进桌面端应用二进制和独立 Engine 中。
- 在桌面端或 Engine 启动时，将其安装释放到 `~/.assetiweave/skills/.system` 目录。
- 在每个租户中，将该共享目录注册为受保护的 `assetiweave-system-skills` 源，其来源标记为 `SourceOrigin::AssetiweaveSystem`。
- 使用内容指纹机制（Content Fingerprint）跳过未变更的安装，并在文件过期或被篡改时进行原子替换。
- 禁止对系统 Skill 进行编辑、删除和备份。将系统 Skill 暴露/挂载给目标应用程序依然是一个显式的 `asset_mounts` 决策。
- 业务逻辑严格保留在 `AppService` 与 Engine 契约中。内置的 `SKILL.md` 是面向 Agent 适配这些契约的指令接口，而不是替代性的持久化或工作流引擎。

## 备选方案

### 将产品 Skill 存放在外部 `util6-agents` 仓库中

否决原因：应用程序发布无法保证 Skill 版本的一致性、离线可用性以及确定性的安装路径。

### 将系统 Skill 放入租户备份库中

否决原因：自定义备份迁移、删除、备份状态判定以及用户编辑都会与应用程序的升级发生冲突。

### 仅作为 Tauri 静态资源打包

否决原因：独立的 Engine 也必须能够安装并暴露相同的资产，而不能强依赖于 Tauri 的应用资源目录。

## 后果

- 新增或修改内置 Skill 会改变应用二进制指纹，并通过正常的应用发布流程交付。
- `SourceOrigin` 及生成的 Engine 契约中包含 `assetiweave_system` 选项。
- 内置的可执行资源必须经过严格校验，并与其标准产品实现保持同步。
- 用户自建与 AI 生成的 Skill 依然是可变的租户库资产；它们绝不会被写入 `.system` 目录。
- Memory 等功能可以依赖稳定的内置 Skill，同时保留 SQLite 中的 Card、Question、Session 和 Memory 结构化记录。
