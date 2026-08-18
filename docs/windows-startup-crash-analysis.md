# Windows 平台启动闪退 / 无法打开问题分析

本文记录 Windows 启动闪退的根因、旧修复分支的审计意见，以及在
`runtime-extension-refactor` 最新代码上的落实结果。

## 1. 故障链路

复现场景是 Windows 检出源码时使用 `core.autocrlf=true`，使内置 Skill 的
`SKILL.md` 变成 CRLF：

1. `include_bytes!` 将 `---\r\n` 原样嵌入二进制。
2. 旧版 `validate_embedded_skills()` 只接受 `starts_with("---\n")`。
3. 校验失败返回 `Err`。
4. 启动入口中的 `install_builtin_skills().expect(...)` 触发 panic。
5. Release 程序使用 Windows GUI 子系统，双击启动时看不到 stderr，表现为进程瞬间退出。

涉及模块：

- [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs)：桌面入口、启动自检和 panic hook。
- [`src-tauri/src/backend/builtin_skills.rs`](../src-tauri/src/backend/builtin_skills.rs)：内置 Skill 校验与安装。
- [`src-tauri/src/backend/logs.rs`](../src-tauri/src/backend/logs.rs)：应用日志与 panic 日志。
- [`.gitattributes`](../.gitattributes)：仓库文本资源的 LF 约束。

## 2. 复现方式

在 Windows 工作区执行：

```powershell
git config core.autocrlf true
git checkout-index --force --all
cargo build --manifest-path src-tauri/Cargo.toml
& ".\target\debug\assetiweave.exe"
```

修复前，CRLF 内置 Skill 会在窗口创建前触发 panic。`core.autocrlf=input`
主要用于提交时规范化，不会在 checkout 时把 LF 转换成 CRLF，因此不是本问题的主要复现场景。

## 3. 旧修复分支审计结论

`fix/windows-startup-crlf-crash` 的原始修复提交为 `183bfd4`。其根因判断和
CRLF 归一化方向正确，但审计发现以下问题：

- Rust 测试断言未通过 rustfmt。
- panic 日志只依赖系统数据目录，目录不可写时仍没有诊断信息。
- `panic.log` 不在应用日志列表中。
- 只处理 CRLF，没有处理 UTF-8 BOM 和孤立 CR。
- 启动阶段仍有不可见的 `eprintln!`。
- 内置 Skill 目录替换缺少 Windows 文件锁重试，回滚错误被忽略。
- 缺少无 UI 的 Windows 启动级自检。
- `pnpm cli:test` 使用 POSIX 环境变量语法，Windows `cmd.exe` 无法执行。
- 原文中的提交号、构建路径、内置 Skill 路径、绝对 `file:///` 链接和 panic 调用栈描述不准确。

## 4. 在最新运行时重构上的落实

本次以 `runtime-extension-refactor` 为代码基线，没有把旧分支整体合并回旧架构，
而是按修复意图重新适配最新的 `AppRuntime` 启动链路。

### 4.1 内置 Skill 内容归一化与严格 frontmatter 校验

校验前依次处理：

```rust
skill
    .trim_start_matches('\u{feff}')
    .replace("\r\n", "\n")
    .replace('\r', "\n")
```

归一化后解析 frontmatter 的开始标记、结束标记、`name` 和非空
`description`，不再用全文件 `contains()` 判断。回归测试覆盖 LF、CRLF、BOM、
孤立 CR、缺失 frontmatter 和错误 Skill 名称。

### 4.2 Windows 文件锁重试和可靠回滚

内置 Skill 的 staging、激活、回滚和清理操作统一经过有限重试。Windows 下每次
失败采用短退避，激活失败时显式检查回滚和 staging 清理结果；激活成功后的旧目录
清理失败只记录持久化 warning，不再掩盖错误细节。

### 4.3 启动诊断

- 启动入口安装 panic hook，持久化 payload、源码位置和 `Backtrace`。
- 首选日志目录不可用时写入 `%TEMP%\AssetIWeave\panic.log`。
- `panic.log` 纳入日志查看器的受管文件列表。
- 启动设置读取、数据库恢复、资产刷新、挂载状态刷新和 Tauri 构建错误统一写入结构化日志。
- 新增 `--startup-self-check`，依次验证 Tauri context、内置资源安装、数据库初始化和运行时关闭。

### 4.4 跨平台 CLI 测试命令

`pnpm cli:test` 改为调用 `scripts/run-go-tests.mjs`，由 Node 设置跨平台的
`GOCACHE`，不再依赖 `$PWD` 或 shell 环境变量语法。

## 5. Windows CI 自检

Windows CI 在 Rust 测试后构建桌面二进制，并运行：

```powershell
& ".\target\debug\assetiweave.exe" --startup-self-check
```

自检使用 runner 临时目录中的数据库和日志目录，避免污染 CI 用户目录。Windows
CI 的 CLI 测试也通过 `pnpm cli:test` 执行。

## 6. 故障排查

1. 首先查看 `%APPDATA%\AssetIWeave\logs\panic.log`。
2. 如果系统数据目录不可写，再查看 `%TEMP%\AssetIWeave\panic.log`。
3. 从 PowerShell 直接运行 `target\debug\assetiweave.exe`，观察退出码和输出。
4. 检查 `SKILL.md` 的行尾，并确认仓库 `.gitattributes` 生效：

   ```powershell
   git check-attr text eol -- builtin-assets/skills/assetiweave-memory/SKILL.md
   git checkout-index --force --all
   ```

## 7. 审计后建议落实清单

| 建议 | 落实状态 |
| --- | --- |
| 运行 rustfmt 并修正文档错误 | 已落实 |
| `panic.log` 纳入日志列表并增加 `%TEMP%` 后备路径 | 已落实 |
| 启动期错误改为持久化日志 | 已落实 |
| 增强 BOM、CRLF、孤立 CR 兼容性和测试 | 已落实 |
| 增加 Windows 文件锁重试及可靠回滚 | 已落实 |
| 增加 Windows 启动自检并修复 `pnpm cli:test` | 已落实 |

以下事项属于后续产品级工作，不影响本次 CRLF 启动崩溃修复：启动窗口与大型恢复
任务的进一步解耦、Windows Symlink 权限预检，以及安装后的 NSIS 交互式启动验证。
