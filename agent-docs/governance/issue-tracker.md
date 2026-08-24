# 问题追踪器：GitHub

本仓库的问题与规格均保存在 GitHub Issues。所有操作使用 `gh` CLI。

## 操作约定

- **创建问题**：`gh issue create --title "..." --body "..."`；多行正文使用 heredoc。
- **读取问题**：`gh issue view <number> --comments`；按需通过 `jq` 过滤评论并一并获取标签。
- **列出问题**：`gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`；根据需要增加 `--label` 与 `--state` 过滤。
- **评论**：`gh issue comment <number> --body "..."`。
- **添加或移除标签**：`gh issue edit <number> --add-label "..."` / `--remove-label "..."`。
- **关闭问题**：`gh issue close <number> --comment "..."`。

在本仓库中运行时，`gh` 会根据 `git remote -v` 自动推断目标仓库。

## 将 Pull Request 作为分诊入口

**PR 作为请求入口：否。** 将此项改为“是”后，`/triage` 会将外部 PR 与问题使用同一套标签和状态处理。

启用后，使用以下对应命令：

- **读取 PR**：`gh pr view <number> --comments`；需要差异时使用 `gh pr diff <number>`。
- **列出待分诊外部 PR**：`gh pr list --state open --json number,title,body,labels,author,authorAssociation,comments`，仅保留 `authorAssociation` 为 `CONTRIBUTOR`、`FIRST_TIME_CONTRIBUTOR` 或 `NONE` 的 PR。
- **评论、标签与关闭**：`gh pr comment`、`gh pr edit --add-label` / `--remove-label`、`gh pr close`。

GitHub 的问题和 PR 共享编号空间；裸编号如 `#42` 可能属于任一类型。先运行 `gh pr view 42`，失败后再运行 `gh issue view 42`。

## 技能操作约定

- 技能要求“发布到问题追踪器”时，创建一个 GitHub Issue。
- 技能要求“获取相关工单”时，运行 `gh issue view <number> --comments`。

## Wayfinder 操作

`/wayfinder` 使用一个 map 问题及其子问题表达工作地图：

- **Map**：创建带 `wayfinder:map` 标签的单个 Issue，正文包含“笔记 / 当前决策 / 未知项”。
- **子工单**：将 Issue 作为 map 的 GitHub 子问题；若子问题功能不可用，则在 map 正文任务列表中加入链接，并在子工单正文顶部写入 `Part of #<map>`。标签使用 `wayfinder:<type>`，其中类型为 `research`、`prototype`、`grilling` 或 `task`；认领后分配给执行者。
- **依赖阻塞**：优先使用 GitHub 原生问题依赖：`gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`。`<blocker-db-id>` 是阻塞问题的数据库 ID，可通过 `gh api repos/<owner>/<repo>/issues/<n> --jq .id` 获取。若依赖功能不可用，则在子工单顶部写入 `Blocked by: #<n>, #<n>`。
- **可执行队列**：列出 map 中未关闭的子工单，排除有未关闭阻塞项或已有受让人的条目，并按 map 中的顺序选择第一个。
- **认领**：`gh issue edit <n> --add-assignee @me`，作为会话的首次写操作。
- **完成**：使用 `gh issue comment <n> --body "<answer>"` 写入结果，再运行 `gh issue close <n>`；最后把上下文指针追加到 map 的“当前决策”。
