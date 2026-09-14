# Memory Q1–Q36：Markdown 投影、Recent UI 与统一设置

## 1. Markdown 根目录

每个 tenant 的唯一 Memory 文档目录：

```text
~/.assetiweave/memories/<tenant>/
├── memory_summary.md
└── MEMORY.md
```

- `<tenant>` 使用现有安全目录编码，不直接拼接未经验证的 tenant 文本。
- 路径位于应用自有目录，不写入项目仓库、第三方 Source 或宿主 Agent 私有目录。
- SQLite 保存逻辑内容和 last-success；文件路径不是身份。

## 2. 发布算法

1. 在同一目录生成唯一临时文件；
2. 从 SQLite last-success 读取投影 DTO；
3. 使用确定性 renderer 生成 UTF-8、LF、末尾单换行内容；
4. 校验标题、窗口、item/revision 和 source reference 数量；
5. flush + fsync 临时文件；
6. 在支持的平台设置只读权限；
7. 原子 rename 替换目标；
8. fsync 父目录；
9. 记录 projection hash、source revision hash 和 published_at。

任何步骤失败保留旧目标文件并产生 `MEMORY_PROJECTION_FAILED`。应用读取不依赖该文件，因此 SQLite last-success 不回滚。

用户通过外部工具修改文件不改变 Authority。下一次投影可以覆盖修改；应用 UI 不提供编辑入口，也不解析修改内容回写数据库。

## 3. `memory_summary.md`

文件只投影最新成功 L1 Snapshot，格式固定：

```markdown
# Recent Memory

- Window: 48h
- From: 2026-09-13 14:00 +08:00
- To: 2026-09-15 14:00 +08:00
- Published: 2026-09-15 14:00 +08:00
- Content generated: 2026-09-15 14:00 +08:00
- Mode: generated

## 2026-09-15

### AssetIWeave

#### What changed
...

#### Suggested next steps
1. ...

#### Memory items
- **Decision · active** — ...
  - Why: ...
  - Sessions: `Session title`, `Session title`
```

确定性规则：

- 日期按本地日期降序；同日项目按 project title、project key 稳定排序。
- 项目窗口摘要和建议只出现一次，放在该项目 `latestActivityAt` 所属日期。
- 普通 Item 放在 `occurredAt` 所属日期。
- 同项目无建议时写固定文案“本窗口没有形成明确下一步”。
- unavailable Session 仍以标题显示并附“来源不可用”，不输出失效链接。
- `publication_kind=reused` 时 Mode 为 reused，并保留原 Content generated 时间。
- 不输出数据库 ID、原始 locator、Prompt、tool call 或 raw JSON。

## 4. `MEMORY.md`

文件只投影当前有效 L2/L3：

```markdown
# Memory

- Published: 2026-09-15 14:00 +08:00
- Revision: <projection revision hash>

## Project Memory

### AssetIWeave

- **Decision** — ...
  - Why: ...
  - Updated: ...
  - Sources: 2 available, 1 unavailable

## Permanent Memory

- **Preference** — ...
  - Why: ...
  - Updated: ...
```

规则：

- L2 项目按标题/key 稳定排序；Item 按 category priority、updated_at 降序、稳定 ID 排序。
- L3 不复制项目进展、项目待办或 Session 细节。
- 当前 revision 有部分不可用引用时只显示计数，不让正文消失。
- superseded/retired revision 不进入普通文件。
- 文件不包含逐项目 include、外部 Markdown 依赖或可执行指令。

## 5. Recent 页面信息架构

### 5.1 顶部

- 标题「近期」；
- 只读窗口说明，例如“48 小时 · 截至 9 月 15 日 14:00”；
- `PillTabs`：按时间 / 按项目；默认按时间；
- 右侧只显示安全状态：已更新、内容复用、更新未完成。

顶部不放窗口设置、刷新、立即生成或深度回忆输入。

### 5.2 时间视图

```text
Date rail（降序）
  Date
    Project group
      Window summary
      Suggested next steps (0–3)
      Memory items
```

项目摘要和建议只在项目最新活动日期出现一次；Item 按日期进入对应段。日期轨道为当前视觉草稿的稳定信息架构，最终颜色和动效另行收口。

### 5.3 项目视图

```text
Project group（按 latestActivityAt 降序）
  Window summary
  Suggested next steps (0–3)
  Items（按 occurredAt 降序）
```

切换只对 `RecentMemorySnapshotView.projects/items` 做 memoized 客户端分组，不发起新的 Memory 请求或 Agent 调用。

### 5.4 Item

折叠状态显示：category、status、时间、title、summary、来源可用性。
展开状态增加：rationale、生成水位、Session 卡片。

Session 卡片显示来源 Agent、Session 标题、相对时间和可用状态。点击 available 卡片进入对话记录详细浏览并选中对应 Session；Recent 页面不提供 Content Node 级跳转。内部 locator 仍可用于审计和深度回忆。

### 5.5 项目空建议

项目有工作摘要但没有证据支持的下一步时，显示固定次要文案，不渲染空卡片，不要求 Agent 生成占位建议。

## 6. 页面交互边界

允许：

- 时间/项目投影切换；
- Item 展开/收起；
- available Session 导航；
- 正常应用导航、滚动和选择文本。

Recent 页面不承载：窗口修改、刷新、生成、重建、retry、cancel、完成、忽略、采纳、置顶、编辑、删除和 Memory 对话。retry/cancel 在任务中心，设置与维护重建在统一设置页。

## 7. 状态表现

| 状态 | Recent 行为 |
|---|---|
| 无 Snapshot、无合格 Session | 空状态：尚无近期记忆 |
| 无 Snapshot、首轮 queued/running | 骨架 + 非阻断生成状态 |
| last-success ready | 展示 Snapshot |
| current Snapshot reused | 展示 Snapshot + “内容复用” |
| 最新目标失败且有 last-success | 展示 last-success + “更新未完成” |
| 最新目标失败且无 last-success | 配置/生成失败空状态，详情指向任务中心 |
| projection 文件失败 | 页面正常读取 SQLite；不把文件错误升级为页面空白 |
| source reference unavailable | 展示不可用状态；Session 卡片不响应导航 |

页面不弹阻断式错误，不向用户提出 Memory 问题。

## 8. 统一设置页

Memory 设置区包含：

| 控件 | 值 | 默认 |
|---|---|---|
| 生成 Memory | boolean | true |
| 使用 Memory | boolean | true |
| 近期窗口 | 24h / 48h / 72h | 48h |
| 水位 1 | 本地 HH:mm | 02:00 |
| 水位 2 | 本地 HH:mm | 14:00 |
| Memory Generation Skill | 内置默认或 tenant 可见普通 Skill | 内置默认 |

Skill 操作：选择、创建可编辑副本、打开当前 Skill、恢复默认。
维护操作：重建近期、重建项目、重建永久、修复 Markdown 投影。所有操作快速返回 Task，不在 Dialog 中等待 Agent。

两个水位相同或格式非法时阻止保存并内联显示 validation。选择新 Skill 时先验证；已经保存的 Skill 后续因用户编辑而失效时保留选择以便修复，但 generation 状态明确失败，不静默回退。

## 9. 视觉与组件约束

- 遵守 Auroraqua-UI 与 Foundation → Common → Domain 分层。
- 视图切换使用 `PillTabs`；业务组件不手写裸 Tab 按钮。
- 卡片与面板使用温润圆角和主题 token，不硬编码颜色。
- 对话记录和 Skill 目录总览是密度与交互基准；视觉方向结合对象化手账卡片与线性时间轨。
- 外部 HTML 草稿只作为信息架构/视觉输入，不直接复制生产 CSS 或绕过设计系统。
- 长列表使用有界渲染；常见 2–5 个项目不是硬上限。

## 10. 可访问性与响应式

- PillTabs、展开按钮和 Session 卡片支持键盘、焦点环和可读 aria label。
- 状态不只依赖颜色，必须有文字或图标语义。
- 窄窗口下日期轨道缩为紧凑日期标签，项目和 Item 保持单列；不得横向裁掉 Session 导航。
- 展开状态由稳定 Item ID 管理，切换投影后仍保持同一 Item 的展开状态。
- 加载、错误和空状态不造成页面级操作锁；后台任务运行时导航保持可用。

## 11. 深度回忆隔离

Memory 一级入口仍包含「近期」和「深度回忆」。深度回忆继续由现有 `assetiweave-memory` 及 Recall workflow 实现；Recent 页面不嵌入其输入框、结果、历史或运行控制。
