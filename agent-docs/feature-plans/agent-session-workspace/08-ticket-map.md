# 工作卡地图：Agent Session Workspace（Issue #31）

## 1. 依赖图

```text
T01 → T02 → T03
             ├─ T04 → T07 → T08 ─┐
             ├─ T05               ├─ T11 → T12
             ├─ T06               │
             └─ T09 → T10 ────────┘
```

> Issue #33 的外部接缝前置条件已由提交 `dcb0fcbf` 满足；T09 开工时重新验证当前代码即可。

## 2. 工作卡索引

| ID | Outcome | Blocked by | Requirements | Contracts | Seams | Gates |
|---|---|---|---|---|---|---|
| T01 | Team 当前单聊经共享兼容 surface 渲染 | 无 | A-001,A-009,R-NFR-001 | C-030,C-033 | S-FE-01–05 | G-00,G-04,G-11 |
| T02 | 一条 ACP Tool Step 完整穿透到 UI | T01 | R-SES-001/002,R-CHAT-005 | C-040–043,C-080–087 | S-BE-01–07,S-FE-01/02/05 | G-01–05 |
| T03 | 完整 Turn/Thinking/View Steps | T02 | R-SES-003–006,R-CHAT-001–007 | C-050–073,C-090–103 | S-BE-01–06,S-FE-02/03 | G-01–05 |
| T04 | AionUi 单聊 Shell/Composer/Scroll | T03 | R-CHAT-001/002/007,R-NFR-005–007 | C-020–033 | S-FE-03/05/07 | G-04–06,G-10/11 |
| T05 | Terminal/Command 结果完整展示 | T03 | R-CHAT-005 | C-080–100 | S-BE-03/04,S-FE-03 | G-05–07 |
| T06 | File/Diff/Image/Artifact 展示 | T03 | R-CHAT-005,R-NFR-008 | C-083–103 | S-BE-03/04,S-FE-03/07 | G-05,G-07,G-11 |
| T07 | Team parallel/single lanes | T04 | R-TEAM-001–005 | C-020–033 | S-BE-07,S-FE-02–04 | G-06,G-08,G-10/11 |
| T08 | Team Plan/Task/operations 迁移 | T07 | R-TEAM-006 | C-030–033,C-092 | S-BE-07,S-FE-03–05 | G-08,G-11 |
| T09 | Session Memory observer tracer | T03 | R-MEM-001–005 | C-010–033,C-060–063,C-110–114 | S-BE-05/06/08–10,S-FE-05/06 | G-02–05,G-09/11 |
| T10 | Project/Global/Recall 与 unavailable | T09 | R-MEM-001–006 | C-010–114 | S-BE-05/08/09,S-FE-05/06 | G-02,G-09,G-11 |
| T11 | 收缩旧 Team Session 呈现 | T08,T10 | A-009,R-NFR-001/002 | 全部 | Legacy 清单 | G-01–11 |
| T12 | 视觉、响应式、无障碍和全仓验收 | T05,T06,T08,T10,T11 | 全部 | 全部 | 全部 | G-12 |

## 3. Frontier

初始 frontier：T01。

T03 完成后的可并行 frontier：T04、T05、T06、T09。

T08 与 T10 完成后：T11。

T05、T06、T08、T10、T11 完成后：T12。

## 4. Checkpoints

### CP-1：共享合同（T01–T03）

演示：一个 Team member Session 显示 request、assistant、thinking、两个 grouped tools 和 terminal；input/output 可展开；replay/live 无重复。

通过条件：G-01–G-05。

### CP-2：用户界面（T04–T08）

演示：AionUi 风格单聊 surface；Team 两/三成员 parallel；single 切换；Plan/Task/stop/interrupt 正常。

通过条件：G-06–G-08、G-10、G-11。

### CP-3：Memory（T09–T10）

演示：四种 Memory scope 从 Task Agent Stage 打开相同 observer；无 composer；validate/publish 仍在 Task View；旧 ref unavailable。

通过条件：G-02–G-05、G-09、G-11。

### CP-4：Contract/Final（T11–T12）

演示：无旧 Team renderer/sanitizer，20 个视觉场景与全仓命令通过。

通过条件：G-12。

## 5. Ticket 发布规则

若发布为 GitHub 子 Issue：

- Parent 为 #31；
- T09/T10 同时关联 #33；
- 使用 GitHub native sub-issue；
- blocker 使用 native dependencies；
- 标签 `ready-for-agent`；
- 按依赖顺序创建，frontier ticket 可领取；
- Parent Issue 不因拆票而修改或关闭。

## 6. 卡尺寸规则

每卡目标为一个 fresh context 可完成。执行中出现以下情况需拆卡：

- 预计同时修改超过两个独立业务子系统；
- Acceptance 无法在 3–6 个外部行为内描述；
- Red test 无法独立落地；
- 当前 slice 不能保持编译/测试 Green；
- 需要引入与 Outcome 无关的 wide refactor。

机械 wide refactor 使用 expand–migrate–contract：T01/T02 expand，T07–T10 migrate，T11 contract。
