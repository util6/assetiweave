# P0 · Provider 凭据隔离 Spike 清单

> 目的：在动任何业务代码前，用本机真实 CLI 验证「同一 Provider 多账号能否靠目录/环境变量隔离」。
> 结论写入本文的矩阵表，作为 P1/P2 的设计地基。Spike 期间**禁止修改 `~/.codex`、`~/.claude` 等真实登录态**。

## 1. 前置

- 在**用户自己的终端**执行（本机沙箱内没有 `codex` / `claude` 等 CLI 的 PATH，且 Spike 需要真实登录态）。
- 所有实验目录放 `/tmp/aiw-spike/`，跑完即删。
- 每个用例都要回答三个问题：
  1. 子进程是否读取了指定目录的凭据（而非 home 下真实凭据）？
  2. 两个不同目录的进程能否**并行**且互不干扰？
  3. home 下的真实凭据文件是否被修改？（`stat` 前后对比 mtime）

## 2. 用例模板（按 Provider 逐个跑）

### Codex（预期：支持 `CODEX_HOME`）

```bash
mkdir -p /tmp/aiw-spike/codex-a /tmp/aiw-spike/codex-b
cp ~/.codex/auth.json /tmp/aiw-spike/codex-a/   # 用现有登录态做样本，不改动源文件
cp ~/.codex/auth.json /tmp/aiw-spike/codex-b/
stat -f "%m" ~/.codex/auth.json                  # 记录基线 mtime

# 并发起两个进程，指向不同 CODEX_HOME，各发一条最小提示词
CODEX_HOME=/tmp/aiw-spike/codex-a <codex-bin> exec "reply with the single word: alpha" &
CODEX_HOME=/tmp/aiw-spike/codex-b <codex-bin> exec "reply with the single word: beta" &
wait

ls -la /tmp/aiw-spike/codex-a /tmp/aiw-spike/codex-b   # 是否生成/读取了各自目录
stat -f "%m" ~/.codex/auth.json                        # 与基线对比，必须未变
```

### Claude Code（预期：`CLAUDE_CONFIG_DIR`，存在不确定性）

```bash
mkdir -p /tmp/aiw-spike/claude-a
CLAUDE_CONFIG_DIR=/tmp/aiw-spike/claude-a <claude-bin> -p "say: alpha"
ls -la /tmp/aiw-spike/claude-a
# 判据：凭据是否落在该目录；若仍读 ~/.claude/.credentials.json，则改试覆写 HOME
HOME=/tmp/aiw-spike/claude-home CLAUDE_CONFIG_DIR=/tmp/aiw-spike/claude-a <claude-bin> -p "say: alpha"
```

### Antigravity（未知，需先摸清目录结构）

```bash
ls -la ~/.gemini/antigravity-ide/          # 找凭据/配置的实际存放位置
# 再试：ANTIGRAVITY_APP_DATA_DIR / XDG_DATA_HOME / HOME 覆写，观察哪个生效
# 若全都无法隔离 → 结论登记为「不支持并发切号」，UI 上该 Provider 只暴露单账号并加串行锁
```

### OpenCode / 其它

```bash
ls -la ~/.local/share/opencode/
# 试 XDG_DATA_HOME=<profile> 与 HOME=<profile>，观察 opencode.db 是否随目录走
```

## 3. 结论矩阵（Spike 完成后填写）

| Provider | 首选手段 | 并行安全 | home 是否被污染 | 结论 | 备注 |
|---|---|---|---|---|---|
| Codex | `CODEX_HOME` | | | 待填 | 源码层面已确认支持 |
| Claude Code | `CLAUDE_CONFIG_DIR` / 覆写 `HOME` | | | 待填 | 需确认凭据是否随目录 |
| Antigravity | 待定 | | | 待填 | 先摸清 `~/.gemini/antigravity-ide` 结构 |
| OpenCode | `XDG_DATA_HOME` / 覆写 `HOME` | | | 待填 | |
| 通用兜底 | 覆写 `HOME` + `XDG_*` | | | 待填 | workspace 路径必须传绝对路径 |

## 4. 出口条件

- 至少 Codex 三项判据全绿；
- 每个 Provider 都有明确结论（可用 / 降级 / 不支持），无「待定」残留；
- 矩阵填完后，回到 `00-execution-router.md` 的 P1/P2 继续推进。
