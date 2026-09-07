# 验证矩阵

状态只能使用：`NOT_RUN`、`PASS`、`FAIL`、`BLOCKED`。Luna 不得以“代码看起来正确”填写 PASS。

| ID | 行为 | 接缝 | 必需证据 | Gate |
|---|---|---|---|---|
| AG-ACP-01 | Catalog protocol 为 ACP | catalog release gate | 解析后的 item 精确断言 | Required |
| AG-ACP-02 | logical ID 仍为 antigravity，上游 ID 为 antigravity-acp | catalog parser | identity assertion | Required |
| AG-ACP-03 | 五平台 Binary metadata 正确 | distribution selector + catalog test | target/URL/entry/args | Required |
| AG-ACP-04 | 每个 archive 使用真实 SHA-256 | 下载证据 + release gate | 命令、size、hash | Required |
| AG-ACP-05 | 动态 installation 生成 ACP definition | RuntimeManager integration | protocol/command/args | Required |
| AG-ACP-06 | empty models 不使 connection 失败 | fake ACP backend | connected=true | Required |
| AG-ACP-07 | model failed/unsupported 不覆盖 protocol ready | RuntimeManager + SQLite | 独立持久状态 | Required |
| AG-ACP-08 | connected 不依赖 model status | installation behavior test | 状态组合表 | Required |
| AG-ACP-09 | execution_ready 不依赖 model status | AppService/installation test | 默认模型可执行 | Required |
| AG-ACP-10 | Registry candidate 不依赖 model status | repository/runtime test | definition 被发布 | Required |
| AG-ACP-11 | initialize/session-new 失败仍为 protocol failure | fake ACP | stable code/state | Required |
| AG-ACP-12 | auth failure 为 auth_required，非 broken | fake ACP + health persistence | status/error/no crash | Required |
| AG-ACP-13 | Translation 经 ACP backend | AgentExecutor high seam | ACP frames + result | Required |
| AG-ACP-14 | 不调用 Native/agy fallback | instrumented fake backend | zero Native observations | Required |
| AG-ACP-15 | 旧 system-antigravity installation incompatible | startup recovery + SQLite | row/status/registry absence | Required |
| AG-ACP-16 | reinstall 前不静默改写旧 executable | repository state | agy path preserved | Required |
| AG-ACP-17 | 前端 legacy label 为 ACP Agent | frontend test | rendered protocol | Required |
| AG-ACP-18 | 未验证 Team 能力关闭 | catalog + Team eligibility | capability false/controlled error | Required |
| AG-ACP-19 | Conversation Adapter 无 diff | git diff path check | no changes | Required |
| AG-ACP-20 | Target Profile 无 diff | git diff path check | no changes | Required |
| AG-ACP-21 | Native generic seam 仍存在 | Rust tests | other Native fixture passes | Required |
| AG-ACP-22 | release evidence 不伪造能力 | release gate | not_run/unsupported accepted, false passed rejected | Required |
| AG-ACP-23 | real initialize/session-new | official binary smoke | transcript-free metadata | Release evidence |
| AG-ACP-24 | real prompt exact response | official binary smoke | expected marker | Tested promotion |
| AG-ACP-25 | real cleanup 无 orphan process/workspace | official binary smoke | reap/removal evidence | Tested promotion |

## 1. 状态组合真值表

| installation | runtime | protocol | model | connected | execution_ready | registry |
|---|---|---|---|---:|---:|---:|
| ready/enabled | ready | ready | ready | true | true | yes |
| ready/enabled | ready | ready | unsupported | true | true | yes |
| ready/enabled | ready | ready | failed | true | true | yes |
| ready/enabled | ready | ready | unchecked | true | true | yes |
| ready/enabled | ready | auth_required | any | false | false | no |
| ready/enabled | ready | failed | any | false | false | no |
| incompatible | any | any | any | false | false | no |
| disabled | ready | ready | any | false | false | no |

## 2. 自动验证命令

按工作包运行目标测试，最终运行：

```bash
cargo fmt --all -- --check
cargo test --workspace
pnpm typecheck
pnpm test
pnpm build
node --test scripts/check-agent-catalog-release.test.mjs
node scripts/check-agent-catalog-release.mjs --release
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

若 `pnpm cli:contract` 因无公开契约变化产生 diff，恢复应由生成器自然产生的无变化状态；不得手工编辑 contract。

## 3. Source guards

```bash
! rg -n 'id\.as_str\(\) == "antigravity"|agent_id == "antigravity"|parse_agy_models' \
  src-tauri/src/backend/ai_execution src-tauri/src/backend/agents

! rg -n '"commandCandidates"\s*:\s*\[\s*"agy"|"protocol"\s*:\s*"native"' \
  builtin-assets/agent-market/catalog-v1.json

git diff --exit-code -- builtin-assets/adapters/antigravity builtin-assets/targets/antigravity.json
```

Source guard 是补充证据，不替代路由行为测试。

## 4. Real smoke 记录字段

- platform / arch
- upstream version 与 Registry commit
- archive URL、size、SHA-256
- executable entry 与 launch args
- initialize protocolVersion、agentInfo、capability names
- session/new 成败与 config option 分类，不记录 session ID
- model discovery ready/empty/unsupported/failed
- prompt result marker，不记录其他生成内容
- session close/delete outcome
- process exit/reap、workspace removal、orphan check
- auth state

禁止把 credential、完整环境变量、原始 prompt/result、用户目录或 session ID 写入 evidence。

