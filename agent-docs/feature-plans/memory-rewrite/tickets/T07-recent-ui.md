# T07：交付「近期」项目/时间视图与精确跳转

## Outcome

Memory 打开到「近期」后立即展示 72 小时工作概览；用户可在项目/时间视图间切换，并从自然语言事件跳到 Session 或精确 Content Node。页面不展示内部实现信息。

## Blocked by

T03、T04、T06。需要稳定读模型、任务状态与 locator freshness。

## Read

- Contracts：C-D04–C-D06、C-A01、C-M04、C-R06。
- Seams：S09、S13–S15；Tests TS06。
- Gates：G0、G3、G7。

## Authority changed

无新持久 Authority；只新增 frontend service schema、UI projection 与导航动作。

## Red test first

service fixture 同时含 Markdown、内部 IDs 和 locator。渲染「近期」后用户可见自然语言标题/摘要/时间/source agent，Markdown 正确渲染，DOM 不出现 ID/locator/raw JSON；点击 content reference 后导航 target 能定位并高亮节点。

## Execution steps

1. 收口 Recent frontend types/schema/service。完成标准：页面无直接 invoke，browser mock 与真实 DTO 使用同一 schema。
2. 实现默认项目视图、时间视图、筛选和 72h 空状态。完成标准：两种视图复用同一事件集合，长列表分页/虚拟化且工具栏可达。
3. 实现项目摘要与六类事件卡。完成标准：只显示自然语言内容、时间、source agent、状态和跳转。
4. 复用 Markdown renderer 与 Conversation navigation chain。完成标准：Session 跳转和 Content Node 高亮/滚动均有测试。
5. 接入 Memory task progress/error。完成标准：只禁用冲突操作，导航、筛选和 Conversation 浏览保持可用。

## Acceptance

- [ ] 默认打开即可看到按项目分组的近期工作。
- [ ] 项目/时间视图身份集合一致。
- [ ] 六类事件有清晰但克制的视觉区分。
- [ ] Markdown 正确渲染。
- [ ] DOM 不出现 Conversation ID、locator、raw JSON、Evidence 管理信息。
- [ ] Session 与具体内容跳转可用。
- [ ] 背景任务不产生页面级 busy 锁。

## Non-goals

Recall 聊天页、设置切换、旧页面最终删除、Engine/CLI。

## Ticket-specific stop

如果跳转需要新建 Card 数据库实体或绕过现有 ConversationsPage 导航链，停止并报告。
