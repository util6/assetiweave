# 0001: 参照 Cockpit-Tools 定稿工作区形态与单层直接软链接挂载策略

> **重要等级**：基石级（P0）  
> **状态**：已接受  
> **参考项目**：[jlcodes99/cockpit-tools](https://github.com/jlcodes99/cockpit-tools)  
> **决策日期**：2026-05-25  
> **决策证据**：`03b2f16`, `57ab9a5`  
> **记录日期**：2026-08-25  

## 背景

AssetIWeave 立项初期的核心诉求是解决开发者在多个 AI 工具（如 Codex、Claude Code、Cursor 等）中分散管理 Skill、Rule、Prompt 资产导致的重复拷贝与版本混乱问题。

在设计资产部署物理层与桌面交互形态时，面临两个核心分歧：
1. **交互形态**：采用传统网页式多级弹窗跳转，还是类似 Finder/Cockpit-Tools 的高密度操作流工作区。
2. **部署拓扑**：是将多源仓库的资产先集中拷贝/软链接到 AssetIWeave 本地中转池（两次软链接），还是直接从目标 App 软链接至源仓库（单次软链接）。

## 决策

1. **工作区架构**：参考开源项目 [jlcodes99/cockpit-tools](https://github.com/jlcodes99/cockpit-tools)，确立侧边导航栏（Side Nav）、顶部标签栏（Header Tabs）、工具栏（Toolbar）与高密度分栏列表（Splitter Column View）的工作区形态，操作与预览同屏呈现，拒绝大弹窗阻断交互。
2. **单层直接软链接（Direct Symlink）**：AssetIWeave 仅作为“元数据管理者”在 SQLite 中记录挂载意图（`asset_mounts`）。部署时直接由目标 App 目录创建单向符号链接指向真实源资产。
3. **物理集中导出独立化**：资产物理集中管理不作为部署前置条件，仅作为按需的导出与备份功能。
4. **外部 Source 绝对只读**：外部代码库与用户目录默认只读，禁止在第三方 Source 目录下写入 `.assetiweave/` 等隐式元数据。

## 备选方案

### 两级软链接中转池（Double Symlink Pool）

- 方案：Source 目录 → AssetIWeave 本地集中池 → 目标 App 目录。
- 优点：集中池在本地有完整的物理镜像映射。
- 缺点：引入了双重符号链接嵌套，极易产生循环链接、死链与跨平台文件系统权限故障；排查路径断裂成本极高。
- 结论：否决。

### 物理文件全量拷贝同步（File Copy Sync）

- 方案：将资产真实物理文件拷贝到每个 App 目标目录。
- 缺点：多副本导致修改无法实时同步回源仓库，违背“集中源头维护”初衷。
- 结论：否决。

## 后果

- 文件系统拓扑极度清爽，无中间虚拟层负担，跨平台排障直接可读。
- 部署计划与状态完全由 SQLite 数据库状态机收拢，保证了本地优先（Local-First）与操作确定性。
