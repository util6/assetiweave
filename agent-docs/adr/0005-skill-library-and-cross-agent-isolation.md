# 0005: Skill 本地库（Skill Library）与跨 Agent 共享防自环隔离机制

> 状态：已接受
> 决策日期：2026-06-04
> 决策证据：`058e12d`, `019e8e9f`
> 记录日期：2026-08-25

## 背景

在实际使用中存在强烈的用户故事需求：“将某个 Agent（如 Codex 或 Claude）私有目录下好用的原生 Skill 提取出来，共享给其他 Agent 使用；同时容纳从外部网络下载的独立 Skill”。

早期曾将该功能设想为“收养/下载”，但在实现中遇到了致命矛盾：如果将某个 App 的私有目录直接作为全局 Source 挂载给其他 App，不仅会导致跨 App 目录强耦合，还容易在批量挂载时将该 Skill 再次软链接回原 App 自身，产生死循环、覆盖与重复识别。

## 决策

1. **统一命名为 Skill 本地库（Skill Library / Backup）**：摒弃模糊的“收养”概念，确立由 AssetIWeave 托管的独立本地中立持久化目录（`Skill Library`）。
2. **中立沉淀区（Decoupled Commons）**：从特定 Agent 提取的 Skill、网络导入的 Skill 或用户手工备份的资产统一存放于本地库中，使其生命周期与单一 Agent 完全解耦。
3. **原生从属性与防自环挂载（Origin Tracking & Self-Mount Prevention）**：在资产模型中引入 `SourceOrigin`（`AppTarget`, `AppLocal`, `AssetiweaveLibrary`, `AssetiweaveSystem` 等），严格识别资产的原生出处，在生成挂载计划时自动过滤掉向原宿主的自环挂载。

## 备选方案

### 直接跨 App 目录互相挂载（Direct Cross-Mounting）

- 方案：直接让 Cursor 软链接指向 `~/.codex/skills/xxx`。
- 缺点：一旦卸载或更新 Codex，Cursor 的能力将瞬间碎裂；且无法防止 Codex 自身再次被反向挂载。
- 结论：否决。

## 后果

- 资产在各 Agent 之间形成了“提取入库 → 中立沉淀 → 自由分发”的健康流转闭环。
- 彻底消除了符号链接自环与文件覆盖风险。
