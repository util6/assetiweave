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

审计时结论（基于 `183bfd4`）：`fix/windows-startup-crlf-crash` 对原始 CRLF 闪退的根因判断正确，核心修复有效；当时仍有 CI 阻塞、文档错误和 Windows 启动可观测性缺口。本次已在 `runtime-extension-refactor` 基线上逐项落实，以下保留原始审计依据与后续产品建议。

## 8. 原始审计记录与当前落实

故障链路成立：

1. Windows 上 `core.autocrlf=true` 将 `SKILL.md` 检出为 CRLF。
2. `include_bytes!` 将 CRLF 原样编译进程序。
3. 旧代码用 `starts_with("---\n")` 校验，遇到 `"---\r\n"` 返回错误。
4. `install_builtin_skills().expect(...)` 触发 panic。
5. Release 程序启用了 Windows GUI 子系统，双击启动时看不到 stderr，表现为闪退。

审计时旧修复分支的三层修复方向合理；本次实现已按最新运行时结构重新适配：

- `src-tauri/src/backend/builtin_skills.rs:269-284`：先把 CRLF 转成 LF，直接解决根因。
- `.gitattributes:1-29`：强制文本资源使用 LF，避免再次产生平台差异。
- `src-tauri/src/lib.rs:51-105`、`src-tauri/src/backend/logs.rs:104-116`：增加启动错误和 panic 落盘，提高诊断能力。

验证结果：

- 新增的 CRLF/LF 单元测试通过。
- 审计时文件的 Git 属性确实为 `text eol=lf`。
- 审计时工作区保持干净。

### 审计发现（审计时状态）

### 高：分支自身未通过 Rust 格式检查

`cargo fmt --all -- --check` 失败，其中本分支新增的测试断言不符合 rustfmt：

- `src-tauri/src/backend/builtin_skills.rs:604-608`

单独检查 `main` 版本的该文件通过，分支版本失败。PR 的 Linux Rust CI 会在 `.github/workflows/ci.yml:90` 阻塞。

### 中：panic 日志仍覆盖不了关键失败场景

`record_fatal_panic()` 与数据库路径都依赖系统数据目录。如果 `%APPDATA%` 无效、无写权限或被安全策略拦截：

- `app_db_path()` 失败；
- `log_error()` 也写不进去；
- panic hook 再次使用相同日志目录，仍然没有日志。

建议增加 `%TEMP%/AssetIWeave/panic.log` 后备路径，并在 Windows 使用原生错误弹窗或 `OutputDebugStringW`；本次已落实 `%TEMP%` 后备日志，原生弹窗属于后续产品增强。

另外，审计时文档称 panic hook 会记录“调用栈”，但代码只记录 payload 和源码位置，没有 `Backtrace`；本次已补充 `Backtrace`。

### 中：`panic.log` 不会出现在应用日志列表

`src-tauri/src/backend/logs.rs:11-13` 的受管日志只有：

- `app.log`
- `codex-api.log`

审计时新建的 `panic.log` 不在 `MANAGED_LOG_FILE_PREFIXES` 中；本次已将其纳入受管日志文件列表。

### 中：解决了 CRLF，但没有覆盖 BOM 和孤立 CR

审计时旧修复只做：

```rust
skill.replace("\r\n", "\n")
```

Windows 编辑器仍可能写入 UTF-8 BOM，旧式文本还可能使用单独的 `\r`。这两种输入仍会在 `starts_with("---\n")` 处失败。

审计建议至少归一化为；本次已落实并增加严格 frontmatter 解析：

```rust
let normalized = skill
    .trim_start_matches('\u{feff}')
    .replace("\r\n", "\n")
    .replace('\r', "\n");
```

长期应解析真正的 frontmatter，而不是全文件 `contains("name: ...")`。

### 文档问题（已在本次文档中修正）

`docs/user-guides/windows-startup-crash-analysis.md` 需要修正：

- `:26`：`core.autocrlf=input` 通常不会在 checkout 时把 LF 转成 CRLF，主要复现场景是 `true`。
- `:43`：workspace 构建产物通常位于 `target/debug/assetiweave.exe`，不是 `src-tauri/target/debug/...`。
- `:71`：示例内置 Skill 路径与当前仓库不一致。
- `:117`：写的是提交 `94ae8f3`，实际分支提交是 `183bfd4`。
- `:146`：声称捕获调用栈，代码没有捕获 backtrace。
- `:256`：代码没有使用 `~/.assetiweave/logs/panic.log` 作为日志后备路径。
- 文档中的 `file:///e:/...` 是单机绝对链接，不适合仓库文档。

### 其他 Windows 潜在问题与后续建议

以下问题描述均是审计时观察；已落实项目在各条后注明，未注明的保留为后续产品建议。

#### 1. 启动阶段仍有大量不可见的 `eprintln!`

`src-tauri/src/lib.rs:109-141` 中，设置读取、数据库恢复、资产刷新、挂载状态刷新等错误仍然只输出 stderr。Windows Release 双击启动时这些信息不可见，应统一改为持久化 `log_warn/log_error`。

#### 2. 窗口创建前执行了较多同步工作

数据库迁移、Agent Runtime 恢复、资产与挂载状态刷新都发生在 Tauri `build()` 之前，启动日志也到 `src-tauri/src/lib.rs:140` 才写入。

在 Windows Defender、OneDrive、漫游配置目录或网络盘环境中，这可能表现为长时间“打不开”。建议窗口先建立，再把扫描/恢复工作放入后台任务。

#### 3. 内置 Skill 安装对 Windows 文件锁较脆弱

`src-tauri/src/backend/builtin_skills.rs:212-236` 通过两次目录 `rename` 完成替换：

- 没有针对 Defender/索引器造成的临时共享冲突重试；
- 第二次 rename 失败后的回滚错误被忽略；
- 安装失败会直接导致应用启动 panic。

建议增加短退避重试，并显式检查回滚结果；本次已落实重试与回滚错误记录，非关键内置资源降级启动仍属于后续产品决策。

#### 4. 主功能依赖 Windows Symlink 权限

`src-tauri/src/backend/host_filesystem.rs` 已能识别错误 1314，但普通 Windows 用户未启用 Developer Mode、未提权时仍不能挂载。这是已识别但尚未消除的产品约束。至少应在首次挂载前做能力预检并提供明确引导。

#### 5. `pnpm cli:test` 在 Windows 下不可执行

`package.json:29` 使用 POSIX 环境变量语法：

```json
"GOCACHE=$PWD/target/go-build go test -C cli ./..."
```

Windows `cmd.exe` 不支持这种写法。建议改成跨平台 Node 包装脚本，或让脚本内部设置 `GOCACHE`；本次已落实 Node 包装脚本。

#### 6. Windows CI 没有启动级烟雾测试

Windows CI 会编译、测试和打包 CLI，但不会启动桌面程序；Release 任务构建 NSIS，也没有安装后启动验证。因此窗口创建前的 panic、WebView2 初始化和安装路径资源问题仍可能漏过。

建议增加一个无 UI 交互的 `--startup-self-check`，在 Windows CI 中至少验证；本次已落实：

- 内置资源校验与安装；
- 数据目录和数据库初始化；
- Tauri context/resource 解析；
- panic 日志后备路径。

### 审计建议处理顺序

1. 运行 rustfmt，修正文档错误。
2. 将 `panic.log` 纳入日志列表，并增加 `%TEMP%` 后备日志。
3. 把所有启动期 `eprintln!` 改为持久化日志。
4. 增强 BOM/孤立 CR 兼容性和测试。
5. 为内置资源替换增加 Windows 文件锁重试及可靠回滚。
6. 增加 Windows 启动自检，并修复 `pnpm cli:test`。
