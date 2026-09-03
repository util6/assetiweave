# 任务一：验证矩阵

命令默认从仓库根运行；每个 gate 记录 revision、命令、退出码、测试数量/跳过项。已有失败是基线事实，不可冒充本卡通过；新增或受影响失败必须修正后验收。

| Gate | 何时执行 | 可观察通过条件 |
|---|---|---|
| G-BASE | A00/每卡开始 | 工具链、工作区归属、当前相关测试结果已记录；无覆盖用户修改 |
| G-FE | 每张前端卡 | 当前卡测试、typecheck、lint 通过；移交时 format:check/build 通过 |
| G-RUST | 每张 Rust/跨层卡 | 当前模块测试和 cargo check、fmt；不增加 blocking 全局锁或 wire 退化 |
| G-CONTRACT | 新命令/DTO/错误/任务行为变化 | Rust registry、Tauri exposure、生成 CLI 合同和真实 CLI 同时匹配 |
| G-BEHAVIOR | 相关替换卡 | 下表中对应场景自动化通过；桌面专项明确人工证据 |
| G-FINAL | A-G01 | 所有卡已验收，全套检查/平台证据齐全，旧机制无生产调用，交接包完整 |

## 常用命令

```sh
pnpm typecheck
pnpm test
pnpm build
# A-F01/F02/F16 加入脚本后执行
pnpm lint
pnpm format:check
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
pnpm check:boundaries
pnpm test:boundaries
pnpm cli:contract
pnpm gen:surface-matrix
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
pnpm conversation-adapters:test
```

`pnpm cli:contract` 和 surface generator 会写生成文件；它们的变化必须审查，不是只跑命令。执行完保存生成物，再运行第二遍确认内容稳定；检查 contract.json 的语义 diff，不手改。格式化只动当前卡范围，A-F02 全量机械格式化单独提交。

## 必须保留/新增的行为证据

| 领域 | 证据（不能仅 source grep） | Owner |
|---|---|---|
| Router | 全部可达菜单、会话/Memory 精确定位、后退/恢复、URL 不变、加载失败 | A-F03 |
| Query | 本地离线继续读取；同 key 合并；租户 A→B→A 旧响应不污染；批量成功只一次业务刷新 | A-F04/A-F05/A-F15 |
| Settings/i18n | SQLite 优先；旧 locale 只导入一次；两窗口争用；save 空 locale 不覆盖；reset 保留语言；旧 mutation 不覆盖新草稿 | A-C01/A-F06/A-F10 |
| Tasks | 事件丢失 polling 收敛；多个组件一个 poll owner；重复终态只通知/刷新一次；切租户不串；操作中仍可导航 | A-F07/A-F08 |
| UI | 源导入 normalize/校验；分组业务约束；最小宽度/键盘/窄屏横向滚动；Markdown 表格/公式/链接/Diff/Mermaid | A-F11–A-F14 |
| Config | 显式参数 > env > 旧默认；HOME 不意外搬库；非 UTF-8 路径；不读取用户偏好或调用级凭据 | A-R01/A-R02 |
| Logging/error | 日志读取旧格式；stdout 只有 JSON 协议；guard 落盘；code/retryable/details 稳定；source 可追踪且公开字段脱敏 | A-R03/A-R04 |
| Validation | 字段长度/范围边界与 normalize 后校验；租户/Leader/去重规则保留 | A-R05 |
| Shutdown | close 同时来新任务被接纳闸门拒绝；任务 panic/取消都释放追踪；终态正确；进程树清理；不同运行模式可退出 | A-R06/A-R07 |
| HTTP | 本地 HTTP fixture 的 200/304/重定向/超时/过大体积/错误 checksum/中断下载/清理/代理；安装期间普通 CRUD 可用 | A-R08–A-R10 |
| Existing crates | banner/requirement 语法不扩大；SQLx 所有历史行/nullable/租户映射相同 | A-R11/A-R12 |
| Contract | services 不吞 Tauri 真错误变 mock；Tauri 与 Engine 同一业务结果、WireError；生成契约/Go handshake 匹配 | A-C02 |

## CLI 与桌面验证

现有 `cli/tests/cli_e2e/cli_e2e_test.go` 从 `ASSETIWEAVE_CLI`/`ASSETIWEAVE_ENGINE` 找二进制，默认可能落到已安装版本。先 `pnpm cli:build`、`pnpm engine:build`，按脚本实际输出指定**本工作树构建**的路径后运行 `pnpm cli:test:e2e`；记录 `version` 返回的提交/版本，防止测到旧安装。fixture 自带临时目录；核查任何新增用例也设置临时数据库，不让真实用户数据成为测试样本。

桌面在隔离数据库/源目录执行：启动、菜单定位、后台扫描期间导航/设置、取消/退出提醒、语言切换重启、源导入和批量挂载、Conversation/Memory/Team 的现有关键流程。截图/录屏或操作日志只记录去隐私样本，写清平台、revision、动作和结果。

最终通过当前 macOS 环境验证并取得 Linux/Windows 现有 CI 结果；没有运行的平台记为未验证，G-FINAL 保持未完成。无需在本任务另搭一套 E2E 框架。
