# 验证矩阵：Agent Session、Team 与 Memory Observer

## 1. 验证原则

- 测试外部行为、公开 DTO、领域事实和用户交互；
- 一个主 tracer fixture 贯穿 Fake Agent → AppService → frontend → UI；
- Provider 映射、runtime bounds 等在低层补充 unit tests；
- 每卡先 Red，再 Minimal，再全量 Gate；
- 不用固定 sleep 证明并发/流式；使用 barrier/channel/fake timers；
- 不断言脆弱 class 字符串，视觉通过真实渲染/截图验收。

## 2. Canonical Fixture

统一 fixture `complete_agent_session_v1` 的逻辑顺序：

1. user request：`Inspect the workspace and update the target.`
2. assistant text delta：`I will inspect`；
3. thinking delta/snapshot；
4. tool A start：`run_command` + command input；
5. tool A update：running + partial stdout；
6. tool A result：success + stdout + exit 0；
7. assistant text：`I found the file.`；
8. tool B start：file edit + input；
9. tool B result：Diff + location；
10. tool C failure：error + stderr + exit 1；
11. assistant final text；
12. terminal result。

派生 fixture：replay + live duplicate、out-of-order、oversized、processing-only、cancel、provider missing detail、unknown content、registry unavailable。

## 3. Gate 列表

### G-00：工作树与事实基线

- [ ] 记录 HEAD、branch、`git status --short`。
- [ ] 标识其他任务未提交文件并保持不动。
- [ ] 当前 targeted tests 已运行并记录结果。
- [ ] AionUi reference commit 可读取。

### G-01：Session Event Contract

- [ ] user request、assistant、thinking、processing、tool、terminal/error 可序列化。
- [ ] Debug/tracing 不包含正文和 tool detail。
- [ ] event dedupe 不增加 revision。
- [ ] tool start/update/result materialize 为一个 item。
- [ ] terminal 状态单调。

### G-02：Bounds 与 Registry

- [ ] 默认 256/2048/4MiB 生效。
- [ ] oversized content 产生 visible truncation metadata。
- [ ] UTF-8 截断不破坏字符。
- [ ] terminal entry 优先淘汰；旧 ref 返回 unavailable。
- [ ] tenant scope 不泄漏其他 Session。
- [ ] shutdown 清空 registry。

### G-03：AppService 与 Transport

- [ ] get 只通过 AppService materialize view。
- [ ] unknown/cross-tenant ref 返回统一 unavailable。
- [ ] update event 只含 ref + revision。
- [ ] receiver lag 可通过 get 恢复。
- [ ] Schema/Rust/TS/Zod 一致。
- [ ] Engine 暴露变化由生成命令产生。

### G-04：Frontend Store

- [ ] 只接受更高 revision。
- [ ] live/replay/out-of-order 确定性合并。
- [ ] terminal 不回退。
- [ ] disconnect 保留最后 view。
- [ ] reconnect/polling 恢复最新 snapshot。
- [ ] UI expand/scroll 不被 server snapshot 重置。

### G-05：Timeline 与 StepGroup

- [ ] 完整 fixture 顺序与真实 sequence 一致。
- [ ] text → tools → text 形成两个正确分段。
- [ ] `查看步骤 · N` 计数逻辑 tool 数。
- [ ] running/failed/default replay 展开规则正确。
- [ ] Input/Output/Diff/Location/Error/Exit/Unknown/Truncated 可见。
- [ ] 更新不 remount row 或丢焦点。

### G-06：Chat Shell 与 Composer

- [ ] Header/timeline/composer 占满父 viewport 且无 page-level overflow。
- [ ] user/assistant/Markdown/代码可读。
- [ ] Enter/Shift+Enter/IME 正确。
- [ ] send/stop/queue/interrupt 按 capability 显示。
- [ ] 乐观 user item 与 server item 去重。
- [ ] auto-follow、上滚冻结、新活动和回到底部正确。
- [ ] loading/empty/restoring/error/unavailable 完整。

### G-07：Terminal、Diff 与 Artifact

- [ ] stdout/stderr/exit 显示且 ANSI 已清理。
- [ ] 长结果折叠与复制正确。
- [ ] Diff 使用现有安全 renderer 或受控文本降级。
- [ ] image/artifact 仅通过合法 viewer 打开。
- [ ] malformed content 不使 Session 崩溃。

### G-08：Team

- [ ] Leader first、roster order、member status/unread 正确。
- [ ] 两成员/三成员 parallel，宽度与 overflow 正确。
- [ ] single/parallel 切换保持 active/scroll。
- [ ] 每 lane 独立 composer recipient。
- [ ] 非活动 lane streaming 不抢焦点/滚动。
- [ ] Plan/TeamTask/anchor/review/confirm/stop/interrupt/recovery 保持。

### G-09：Memory Observer

- [ ] Session/Project/Global/Recall 都产生 typed ref。
- [ ] Task View 主详情不含 transcript。
- [ ] observer 显示真实 prompt/thinking/tools/terminal。
- [ ] observer 无 composer/permission/write action。
- [ ] validate/publish 仍在 Task View。
- [ ] cancel/retry 仍走 Task API/Memory Job。
- [ ] restart/eviction/invalid ref 显示 unavailable。

### G-10：Accessibility 与 Responsive

- [ ] 320/768/1024/1440 无不可达内容。
- [ ] tablist/tab、button、log/status/alert 语义正确。
- [ ] Arrow/Home/End、Tab、Enter、Space、Escape 可用。
- [ ] 流式更新不逐 token 播报。
- [ ] focus 不因 delta 丢失。
- [ ] reduced-motion 关闭非必要动画。
- [ ] dark/light 对比度达标。

### G-11：Architecture Guard

- [ ] 页面/组件无 direct `invoke(...)`。
- [ ] 无新增 Session/Transcript migration。
- [ ] 无 AionUi runtime dependency。
- [ ] 无第二 registry/executor/task runtime。
- [ ] Task Center Provider 不保存 Session items。
- [ ] Provider content 不进入 logs/tracing/notifications。
- [ ] raw color/border/shadow 已通过 lint/rg 审计。

### G-12：Final Gate

- [ ] 所有 G-00–G-11 有证据。
- [ ] AionUi 20 个视觉场景已对照。
- [ ] #19/#20/#21/#31/#33 回归通过。
- [ ] Legacy 收缩完成且无调用点。
- [ ] 全仓命令通过。
- [ ] Code review 无 P0/P1 未解决项。

## 4. 测试层矩阵

| 层 | 主要行为 | Fixture/方法 |
|---|---|---|
| Rust session_events | merge/order/state/bounds/debug | event constructors + property/table tests |
| Rust registry | scope/lifecycle/eviction | small capacity registry |
| Rust ACP/native | raw input/output/content mapping | protocol fixtures |
| AppService | ref/get/unavailable/Memory link | temp DB + Fake Agent |
| Tauri transport | wire casing/schema/event | command serialization tests |
| TS schema/store | parse/revision/dedupe/reconnect | complete fixture + fake events |
| Shared components | Turn/Thinking/Steps/Composer | Testing Library |
| Team page | lanes/workflow/scroll | mocked services/provider |
| Task Center | stage link/observer/no transcript | Task + Session fixtures |
| Desktop/E2E | real layout/theme/console | Tauri/browser screenshots |

## 5. Memory Scope Matrix

| Scope | Purpose | Mode | Request item | Tool events | Task link | Observer write |
|---|---|---|---|---|---|---|
| Session | sessionMemory | oneShot | required | Provider-dependent | required during Agent stage | false |
| Project | projectMemory | oneShot | required | Provider-dependent | required during Agent stage | false |
| Global | globalMemory | oneShot | required | Provider-dependent | required during Agent stage | false |
| Recall | recall | persistent | per actual turn/replay | reader tools | required when run as background task | false |

## 6. Merge Matrix

| Current | Incoming | Result |
|---|---|---|
| lower sequence | higher sequence | incoming materialized |
| higher sequence | lower sequence | current retained, incoming stored only if needed for deterministic replay |
| replay same sequence | live | live wins |
| live same sequence | replay | live retained |
| running | succeeded/failed/cancelled | terminal |
| terminal | running | terminal retained |
| terminal A | different terminal B | retain deterministic first + conflict notice |
| delta | later delta | append |
| text | later snapshot | replace |
| terminal text == assistant | terminal state only |

## 7. UI State Matrix

| Data state | Timeline | Header | Composer |
|---|---|---|---|
| initial loading | skeleton | identity if known | disabled/skeleton only interactive |
| empty idle | empty state | idle | interactive enabled |
| restoring | old items + status | restoring | according to capability |
| running | stream | running | send/stop/queue rules |
| succeeded | retained items | succeeded | interactive enabled if Session continues |
| failed | retained items + local error | failed | retry only if callback |
| cancelled | retained items + cancel | cancelled | per Session capability |
| unavailable | unavailable state | known identity + unavailable | absent/disabled |
| observer any | same timeline | read-only | absent |

## 8. 性能测试

- 2048 events 合并后 snapshot 不超过配置 bounds；
- 100 token deltas 不导致 100 次所有 Team lanes 完整 render；
- 一个 lane 高频更新时其他 lane memoized renderer 不重复工作；
- 4MiB fixture 的折叠 tool detail 不提前执行完整 Markdown/Diff parse；
- event lag 后一次 get 收敛到最新 revision；
- 测试使用 render counters/profiling assertions，而非不稳定 wall-clock 作为唯一证据。

## 9. 视觉证据

保存或在验收报告引用：

- AionUi reference commit；
- before/after screenshot；
- 宽度、主题和状态名称；
- console warning/error 结果；
- 与规范差异及原因。

截图不作为代码事实源；行为测试仍是回归 Authority。

## 10. 必跑命令

按当前仓库事实执行：

```bash
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
```

Engine/CLI 公开契约变化时：

```bash
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

每卡先运行与改动对应的 targeted tests；Final Gate 才运行完整矩阵。任何命令失败都记录命令、退出码、首个根因和复现方式。
