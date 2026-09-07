# Luna 执行手册

## 1. 每轮固定协议

Luna 每轮只执行 `03-work-packages.md` 中一个 `AGACP-*`。不得一次实现整个专项。

开始时：

1. 读取执行路由、契约、当前基线和目标工作包。
2. 运行 `git status --short`，列出已有未提交修改并声明不会触碰。
3. 输出 3–8 条当前代码事实、完成条件、文件白名单和第一个失败测试。
4. 确认依赖工作包在 `07-progress.md` 为 PASS。
5. 先使测试按预期失败，再做最小实现。

结束时：

1. 运行目标验证。
2. 检查白名单外 diff。
3. 填写 handoff 与 progress。
4. 使用中文 Conventional Commit；未通过门禁不提交。
5. 停止，不执行下一工作包。

## 2. 通用 Prompt

```text
执行 Google Antigravity 官方 ACP 接入的 {{TASK_ID}}：{{TASK_TITLE}}。

必须按顺序读取：
1. AGENTS.md 与 CONTEXT.md。
2. agent-docs/feature-plans/antigravity-official-acp/00-execution-router.md。
3. 01-contract.md 中与本任务相关的 C-*。
4. 02-current-baseline.md。
5. 03-work-packages.md 中 {{TASK_ID}} 全节。
6. 04-verification-matrix.md 对应 AG-ACP-*。
7. 07-progress.md 与 git status --short。

文件白名单：
{{FILE_ALLOWLIST}}

工作协议：
- 先输出当前代码事实、完成条件、预期失败测试。
- 依赖任务未 PASS 就停止。
- 先写测试并记录旧实现的精确 FAIL，再实现最小改动。
- 不修改白名单外文件；需要时停止报告，不自行扩大范围。
- 不修改用户已有未提交改动，不 reset，不 clean。
- 不新增 AntiGravity-specific ACP backend、protocol 分支或 agy fallback。
- 不修改 Conversation Adapter、Target Profile、fs/terminal/MCP/OAuth。
- 不伪造 SHA-256、release evidence 或 real E2E。
- 不因 model empty/failed 把 protocol ready 改为 failed。
- 不放松业务 OneShot session cleanup。

必须运行：
{{VERIFY_COMMANDS}}

交付格式：
1. CHANGES MADE
2. TESTS ADDED/UPDATED
3. EXPECTED FAIL EVIDENCE
4. VERIFICATION（逐命令 PASS/FAIL）
5. CONTRACT COMPLIANCE
6. THINGS NOT TOUCHED
7. OPEN ISSUES / DEVIATIONS
8. NEXT TASK（只写，不执行）
```

## 3. 全局 Stop Conditions

- 官方 Registry 版本、URL、cmd 或 args 与基线不同。
- 五个平台任一 archive 无法完整下载或 hash 不可复核。
- 需要 fake/placeholder hash 才能通过 catalog。
- 需要 `if antigravity` 才能修复 ACP compatibility。
- 需要保留 `agy` fallback 才能让 Translation/Team 通过。
- 需要修改 Conversation Adapter 或 Target Profile。
- 需要新增 migration，而现有状态枚举足以表达目标。
- connection probe 与 Issue #18 cleanup 契约无法同时满足，且真实 Server 会留下不可删除 session。
- 需要开启 terminal/fs/MCP 或实现 OAuth。
- 目标任务超过 5 个主要文件且无法按测试接缝拆分。
- 已有未提交修改与白名单文件冲突，无法判断所有权。

停止报告只包含：精确冲突、受影响契约、2–3 个方案与 trade-off、推荐方案、等待决策。

## 4. AGACP-01 专用 Prompt

```text
执行 AGACP-01。先用 fake ACP 增加“initialize + session/new 成功但 configOptions=[]”失败测试。

证明以下真值：
- check_connection 成功；
- protocol_status=ready；
- model_status=unsupported 或 failed；
- connected/execution_ready=true；
- 动态 Registry 包含 definition；
- model discovery 没有调用 agy models。

只修改工作包白名单。connection probe cleanup 与业务 OneShot cleanup 必须分开；不得用 preserve_session 假装 probe 成功。
```

## 5. AGACP-02 专用 Prompt

```text
执行 AGACP-02。临时 SQLite 预置：agent_id=antigravity、protocol=native、distribution_id=system-antigravity、program=agy、状态均 ready。

活动 catalog fixture 使用相同 logical ID、protocol=acp、Binary distribution。
先证明旧代码仍发布 Native definition；再实现 startup reconciliation，使 record=incompatible 且 Registry 无该 definition。

保留旧 row 供 reinstall/uninstall，不就地改 protocol/program，不删除外部 agy，不改 assignment。
```

## 6. AGACP-03 专用 Prompt

```text
执行 AGACP-03。首先从 07-progress.md 读取 AGACP-00 的五个 hash；任何一个缺失立即停止。

Catalog 只使用 Google 官方 1.1.1 archive。verification=experimental。
默认关闭 modelDiscovery/resume/historyReplay/liveEvents/richHistoryReplay/teamTools；只有既有文本用途和 textPrompt 保持。

release evidence 每个状态必须来自真实命令；未运行写 not_run，不得把 Registry 的官方身份当作 AssetIWeave E2E passed。
```

## 7. AGACP-04 专用 Prompt

```text
执行 AGACP-04。先增加高层路由测试：antigravity ACP definition 执行 Translation 时只触发 ACP fake。

删除 Native backend 中按 antigravity ID 的 selector 与专属模型解析。若 antigravity.rs/history/tests 已无合法 production consumer，删除模块接线和文件；保留 Conversation Adapter 与 Target Profile。

Team 行为按 capability fail-closed，不保留 Direct-CLI fallback。
```

## 8. Review Prompt

```text
只读审查 {{TASK_ID}}，不修改代码。

依据：01-contract、目标工作包、04-verification-matrix、git diff、测试输出。
检查：
1. logical/upstream/runtime identity 是否混淆；
2. model 状态是否反向污染 protocol/readiness；
3. 是否存在 Vendor branch 或 agy fallback；
4. 旧 installation 是否仍可进入 Registry；
5. SHA/evidence 是否来自真实制品与真实执行；
6. OneShot/probe cleanup 是否被偷换；
7. Conversation Adapter、Target、Team、OAuth、fs/terminal/MCP 是否越界；
8. 测试是否穿过最高接缝。

输出 P0/P1/P2 findings、文件与行、residual risks；无 finding 时写 no findings。
```

## 9. 建议提交消息

```text
test: 固化 AntiGravity ACP 制品证据
fix: 解耦 ACP 连接与模型发现状态
fix: 标记旧 AntiGravity Native 安装不兼容
feat: 接入 Google AntiGravity 官方 ACP 分发
refactor: 移除 AntiGravity Native 执行路径
test: 补齐 AntiGravity ACP 端到端覆盖
docs: 记录 AntiGravity ACP 真实验证证据
```

