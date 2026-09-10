# T05：补齐终端与命令结果展示

## Outcome

工具步骤能够完整、可读地显示命令、cwd、stdout、stderr、exit code/signal、运行中更新和截断状态；长输出不撑宽聊天 lane。

## Blocked by

T03。

## Scope

- Requirements：R-CHAT-005、R-NFR-003/004/008
- Contracts：C-080–C-103
- Seams：S-BE-03、S-BE-04、S-FE-03；AionUi Terminal/Tool references
- Gates：G-01、G-05、G-07、G-11

## Preflight

1. 确认 T03 已 verified，并读取 tool block 与 truncation 的最终合同。
2. 定位 ACP/native command 映射、现有 ANSI 工具和可复用 code/terminal primitive。
3. 在固定 AionUi 提交中读取 `03-codebase-seams.md` 指定的 Terminal/Tool renderer。
4. 固定 running、success、non-zero exit、signal、oversized UTF-8 五组 fixture 并记录 baseline。

## Red tests

1. ACP/native command fixture 映射 command/cwd/stdout/stderr/exit。
2. ANSI 清理后无控制序列。
3. running output 更新同一 Step。
4. failed exit 与 tool failed 一致；错误局部显示。
5. 长 UTF-8 输出按 head/tail 截断并显示字节元数据。
6. 折叠时不执行昂贵 terminal/markdown 渲染。

## Implementation steps

1. 扩展 Provider mapping 到 command/terminal/exit blocks。
2. 复用或建立受测 ANSI normalization；不改 Conversation Adapter 持久化策略。
3. 更新 Session bounds/truncation metadata。
4. 实现 Terminal detail renderer、stdout/stderr 分区与 copy。
5. 保证 preformatted overflow 留在 Step detail 内。
6. 对照 AionUi `MessageAcpTerminalOutput` 与 ToolGroup 的状态层次。
7. 添加 malformed/unknown terminal 降级。

## Acceptance criteria

- [ ] 命令和结果事实端到端可见。
- [ ] ANSI 不执行/不污染 UI。
- [ ] update/result 不重复 Step。
- [ ] exit/error 状态一致且局部。
- [ ] 长内容可折叠、复制、截断可见。
- [ ] Task/logs 不获得正文副本。

## Verification

- ACP/native mapping tests；
- session projection tests；
- Terminal component tests；
- 真实成功/失败 command 手工验收；
- G-07/G-11 审计。

## Non-goals

- Diff/image/artifact；
- Team layout；
- Memory wiring。

## Handoff

T12 前保持本卡视觉场景证据；无需阻塞 T04/T06/T09。
