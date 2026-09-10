# T04：复刻 AionUi 单聊壳层、Composer 与滚动

## Outcome

Team 单成员使用一套可复用的 AionUi 等价 interactive chat surface：紧凑 Header、开放式消息区、底部 Composer、真实发送/停止状态、自动跟随、上滚冻结和回到底部。

## Blocked by

T03。

## Scope

- Requirements：R-CHAT-001/002/007、R-NFR-005–007、A-006/A-007
- Contracts：C-020–C-033
- Seams：S-FE-03、S-FE-05、S-FE-07；AionUi Shell/SendBox/AutoScroll references
- Gates：G-04、G-06、G-10、G-11

## Preflight

1. 确认 T03 已 verified，共享 timeline 已能完整渲染 canonical fixture。
2. 在固定提交 `18022a49684d5a2b54b0a47f904e76b17f758b3b` 中读取 `03-codebase-seams.md` 指定的 AionUi ChatLayout、MessageList、SendBox 与 auto-scroll 文件。
3. 定位 AssetIWeave Foundation tokens、现有输入控件与 Team 发送/停止 callback。
4. 记录现有 Team 单聊 desktop screenshot、keyboard behavior 与 component-test baseline。

## Red tests

1. Header/timeline/composer 填满 bounded parent，timeline 是唯一纵向滚动区。
2. interactive 显示 Send；running capability 显示 Stop/Queue/Interrupt 的正确一种。
3. Enter 发送、Shift+Enter 换行、IME Enter 不发送。
4. 发送后 optimistic user item 与 server item 去重。
5. near-bottom 时流式跟随；上滚后不移动并显示新活动；点击回到底部恢复。
6. observer props 不显示 composer。
7. loading/empty/restoring/error/unavailable 都有非空 UI。

## Implementation steps

1. 对照固定 AionUi ChatLayout、MessageList、SendBox、useAutoScroll 记录布局与状态。
2. 重构 shared Workspace 为 Header/Timeline/Composer composition。
3. 使用当前 Foundation/UI/Input/Button 与 semantic tokens 重建，不 import AionUi CSS。
4. 实现 container-driven chat width/gutter。
5. 实现 composer capability matrix、IME 与 optimistic send binding。
6. 实现 lane-local auto-follow/new activity state。
7. 加入 loading/error/empty/unavailable boundaries。
8. 接入 i18n、visible focus、reduced motion。
9. 在 Team 单成员路径真实替换旧外观。

## Acceptance criteria

- [ ] 单成员页面结构与 AionUi chat hierarchy 等价。
- [ ] Composer 使用真实 Team service callback，无 direct invoke。
- [ ] keyboard/IME/optimistic send 正确。
- [ ] scroll 行为不抢用户位置。
- [ ] observer mode 没有输入区。
- [ ] dark/light 与四个宽度可用。
- [ ] 无 raw color/第二 UI library。

## Verification

- component tests + fake scroll geometry；
- Team single-member integration tests；
- 320/768/1024/1440 browser/Tauri screenshots；
- keyboard walkthrough；
- console zero new errors；
- `pnpm typecheck && pnpm test && pnpm build` 的相关/最终阶段命令。

## Non-goals

- Team parallel/member tabs；
- 完整 terminal/diff/image；
- 未有 backend 的 voice/mention/slash 等假能力；
- Memory observer navigation。

## Handoff

完成后 T07 可开始。记录已接通与隐藏的 composer capabilities。
