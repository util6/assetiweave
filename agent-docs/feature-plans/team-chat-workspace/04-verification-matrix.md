# Team 聊天工作台：验证矩阵

每张执行卡只运行列出的 Gates；T15 运行全部。命令必须记录退出码和关键结果，不能只写“测试通过”。

## G0 — 工作区与基线

```bash
git status --short
git diff --check
```

完成标准：

- 开工前记录已有修改并标注本卡不会触碰的文件。
- 完工后没有未解释的新文件、冲突标记、尾随空格或覆盖用户修改。
- 当前卡 diff 能逐文件映射到工作卡中的职责。

## G1 — Frontend

目标测试先运行：

```bash
pnpm test -- frontend/src/pages/team/TeamPage.test.tsx
```

若当前卡新增更窄测试文件，先运行该文件，再运行：

```bash
pnpm typecheck
pnpm test
pnpm build
```

完成标准：类型、Vitest 和生产构建全部通过；组件测试断言用户可见行为，不以 mock 调用次数代替 timeline/state 结果。

## G2 — Rust 与边界

目标测试先使用本卡新增的唯一测试名过滤运行，再运行：

```bash
cargo fmt --all -- --check
cargo test --workspace
pnpm check:boundaries
```

完成标准：Rust tests 全绿；Session Adapter、Team、Conversation 和 transport 依赖方向满足边界脚本；没有真实网络或用户数据库依赖。

## G3 — Engine contract

公开 Engine 方法、DTO、风险或 exposure 变化时运行：

```bash
pnpm cli:contract
pnpm gen:surface-matrix
pnpm check:surface-matrix
git diff --check -- cli/internal/schema/contract.json agent-docs/generated/surface-matrix.md
```

随后再次运行生成命令；第二次运行必须保持上述两个生成文件的 diff 不变。生成文件只通过命令更新。

完成标准：Rust registry、surface mapping、生成 contract 和 surface matrix 一致，无手工编辑痕迹。

## G4 — Go CLI

```bash
git diff --name-only --diff-filter=ACMR -- 'cli/**/*.go' | while IFS= read -r file; do gofmt -w "$file"; done
go vet -C cli ./...
go test -C cli -race ./...
```

完成标准：Go 格式、vet 和 race tests 全绿；CLI 只通过 Engine client 调用新操作。

## G5 — Session、Authority 与隐私回归

自动测试必须证明：

1. Team member turn/replay 前后 Conversation 核心表行数不变。
2. Team/Conversation 表、operation log 和 durable task snapshot 不包含测试 prompt、assistant marker、thought marker、tool payload、credential 或 Resume Anchor marker。
3. replay event 不执行工具、不写 mailbox、不改变 TeamTask revision/state。
4. 重复 event、reconnect snapshot 和 replay/live overlap 只形成一个 logical item。
5. OneShot 成功、失败、超时和 cleanup 行为保持既有结果。

证据来自 TS01、TS02、TS08；字符串 grep 只能辅助，不能替代数据库与公开结果断言。

## G6 — Antigravity deterministic fixtures

使用 fake `agy` 和临时 Provider store 覆盖：

- 新 Session 的非空 init Conversation ID；
- 下一 turn 的 resume 参数；
- result 才提供 ID 的情况；
- 失败返回空 ID 时保留旧 anchor；
- text、tool step/result、unknown event、malformed line、cancel、timeout；
- 完整 transcript 优先、简化 transcript fallback、missing/malformed partial。

完成标准：无真实 Antigravity 登录、网络和用户目录依赖；fixture 标明对应 Provider 版本/格式来源。

## G7 — Chat UX 行为证据

组件自动测试至少覆盖当前卡相关项：

- Leader 默认、群主标识与活动头像；
- 头像切换改变 timeline/composer recipient，不改变 task owner；
- inactive member 流继续并更新状态；
- delta 原位追加、thinking/tool 分组、error 就地显示；
- Leader task mode、inline review、confirm gate；
- task projection、Leader aggregate、jump；
- structured shell 先出现、active replay 优先、partial/unavailable；
- near-bottom autoscroll、阅读旧内容不跳动、键盘与焦点。

桌面手工验证使用 `pnpm tauri:dev`，记录截图或短视频。浏览器 mock 只能验证布局，不替代 Tauri event/Provider smoke。

## G8 — 最终矩阵

```bash
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
pnpm check:boundaries
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
git diff --check
```

若 Engine 契约变化，额外运行 `pnpm cli:contract` 后再次执行相关检查。若当前环境允许，运行 `pnpm cli:test:e2e`；环境缺失必须记录具体阻塞，不伪报通过。

## Ticket 证据格式

每个 Acceptance 对应一行：

| Acceptance | Test/command | 结果 | 证据位置 |
|---|---|---|---|
| A1 | `exact command or test name` | PASS/FAIL | 测试名、截图或输出摘要 |

“代码看起来正确”“手工点过”或只给总测试命令不能证明单条 Acceptance。

## Checkpoint Review

Review Agent 检查：

1. 每条 Acceptance 是否有最高层证据。
2. Authority 是否仍为 Provider正文、Team facts、Agent Execution binding、TaskRuntime transient projection 四条明确边界。
3. 是否出现 Vendor/Agent ID 分支泄漏到 Team、transport 或 frontend。
4. replay 是否产生 live 副作用。
5. 是否存在 form-first 平行旧 UI、frontend 直连、CLI 直写或手工 generated contract。
6. 是否泄漏正文、tool payload、credential 或 Resume Anchor。

只输出 P0/P1/P2 findings；每条包含文件与行、破坏的 Contract ID、可执行修复和回归测试。
