# SPEC-BA-03：HostProcess 与 Extension Kernel 执行权威

- 状态：Proposed v1
- 优先级：P1
- 前置：SPEC-BA-01；进程取消与 SPEC-BA-02 集成
- 关联：0011

## 1. 当前问题

Extension Kernel 文档声明拥有 process invocation 和 probing，但
`extension_kernel/launcher.rs` 只有 `ProcessInvocation`、`ProbeSpec`、`ProbeResult` 类型，
没有 launcher/probe 实现。

生产旁路包括：

- `conversations/external.rs::run_external_adapter` 自建子进程和轮询超时。
- `conversations/io_utils.rs::run_runtime_probe` 自建 probe runner。
- `ai_execution/backends/native.rs` 使用 `tokio::process::Command::output().await`，无统一
  timeout、cancellation、output cap。

## 2. 目标分层

```text
Domain manifest
   │ normalize
   ▼
ProcessInvocation / ProbeSpec
   │
   ▼
ExtensionLauncher (kernel policy)
   │
   ▼
HostProcess (OS mechanics)
   │
   ▼
std/tokio process + process tree
```

- Domain 负责把强类型 manifest 转换成无损 invocation/probe。
- Kernel 负责 extension 级 policy、错误分类和结果映射。
- HostProcess 负责程序解析、stdio、deadline、output cap、取消和进程树清理。

## 3. HostProcess API

```rust
pub(crate) struct HostCommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub working_dir: Option<PathBuf>,
    pub stdin: HostInput,
    pub timeout: Duration,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
}

pub(crate) enum HostInput {
    Null,
    Bytes(Vec<u8>),
}

pub(crate) struct HostCommandOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub elapsed: Duration,
}

pub(crate) async fn run_host_command(
    spec: HostCommandSpec,
    cancellation: CancellationToken,
) -> Result<HostCommandOutput, HostProcessError>;
```

同步调用方使用一个薄的 `run_host_command_blocking` 桥；同步和异步 API 必须共享实际执行
内核，不能形成两套 kill/output 逻辑。

## 4. 强制执行语义

### 4.1 程序与参数

- 禁止通过 shell 拼接参数。
- 相对 executable name 必须经 `resolve_host_executable`。
- package manifest 的 entry 必须先经过 HostFilesystem containment 校验。
- 日志/Debug 不得输出完整 env、stdin、prompt、token。

### 4.2 Timeout 与 cancellation

- deadline 和 cancellation 竞争时，先观察到者决定公开错误分类。
- timeout → `HostProcessError::Timeout`。
- cancellation → `HostProcessError::Cancelled`。
- 两者都必须终止整个进程树、关闭 stdin、drain 有界输出并 reap 直接子进程。
- `child.kill()` 不是合格的独立实现。

### 4.3 Output cap

- stdout/stderr 必须边运行边 drain，不能等待退出后才读。
- 达到 cap 后继续 drain/丢弃直到退出，防止 pipe deadlock。
- probe 发生截断必须失败闭合，不得把截断版本文本当成功。
- 对外错误只暴露稳定摘要；原始 stderr 仅进入受控、本地、脱敏日志。

## 5. ExtensionLauncher

```rust
pub(crate) struct ExtensionLauncher {
    host: HostProcess,
}

impl ExtensionLauncher {
    pub async fn invoke(
        &self,
        invocation: &ProcessInvocation,
        input: HostInput,
        limits: InvocationLimits,
        cancellation: CancellationToken,
    ) -> Result<InvocationResult, ExtensionError>;

    pub async fn probe(
        &self,
        invocation: &ProcessInvocation,
        probe: &ProbeSpec,
        cancellation: CancellationToken,
    ) -> Result<ProbeResult, ExtensionError>;
}
```

`ProbeResult` 必须实际由生产代码构造；若最终决定 kernel 不拥有 probe，则必须修订
0011 和模块注释并删除虚假类型。不得继续保留“文档声称拥有、代码不执行”的状态。

## 6. DomainPackageSystem 契约

当前 `on_installed`/`on_removed` 两个实现均为空操作。必须二选一：

### 选择 A（默认）

删除空 hook。生命周期的领域激活显式由 Application 调用领域 service，避免空接口制造完成假象。

### 选择 B

保留 hook，但必须定义事务顺序、失败补偿和至少一个真实生产行为；两个领域都需要行为等价测试。

禁止为“以后可能需要”保留无语义 hook。

## 7. 迁移映射

| 旧实现 | 新实现 |
|---|---|
| `run_external_adapter` 手写 Command | `ExtensionLauncher::invoke` |
| `run_runtime_probe` | `ExtensionLauncher::probe` |
| native Agent connection probe | `ExtensionLauncher::probe` 或 Agent Registry 统一 probe |
| native model discovery `output().await` | 有 timeout/cap 的 HostProcess command |
| Agent installers | 保持 Installer 领域逻辑，进程执行委托 HostProcess |
| `agents/process.rs` 长连接 ACP | 保留 ManagedAgentProcess，但复用 HostProcess 的进程树配置/清理原语 |

长连接 ACP session 与一次性 command runner 不是同一个抽象；不得为了“统一”把 ACP stdio
session 强行改成一次性 output API。

## 8. 错误分类

`ExtensionError` 至少区分：

```text
ManifestInvalid
Incompatible
TrustRejected
ProgramNotFound
LaunchFailed
ProbeFailed
Timeout
Cancelled
OutputLimitExceeded
CleanupFailed
```

禁止所有错误最终都变成 `LaunchFailed(String)`。

## 9. 边界守卫

生产代码允许直接构造 Command 的位置仅限精确 allowlist：

```text
backend/host_process.rs
backend/agents/process.rs       # 长连接进程实现
adapters/platform/*             # 系统 open/reveal
测试 fixture
```

以下目录命中 `Command::new` 必须失败：

```text
backend/application
backend/conversations
backend/ai_execution/backends
backend/extension_kernel（launcher 内部也必须通过 HostProcess）
```

Installer 若临时保留 Command builder，必须只构造 spec，不得 `.spawn()`/`.output()`。

## 10. 测试要求

1. `host_command_timeout_kills_child_and_grandchild`
2. `host_command_cancel_kills_child_and_grandchild`
3. `host_command_drains_output_after_cap_without_deadlock`
4. `probe_rejects_truncated_version_output`
5. `external_adapter_timeout_uses_shared_host_error`
6. `native_model_discovery_has_bounded_deadline`
7. `extension_probe_result_preserves_required_and_detected_versions`
8. `invocation_debug_redacts_env_and_stdin`
9. `kernel_and_domain_probe_classification_are_equivalent`

## 11. 验收标准

- 指定 domain 目录中不再出现独立 process runner。
- native connection/model discovery 有 timeout、取消、输出上限和进程树清理。
- `ProbeResult` 是生产路径的一部分，不再是 dead code。
- Extension Kernel 的实际职责与 0011、模块文档完全一致。
- 所有迁移后的 probe 在超时、缺失程序、非零退出、截断输出上返回一致错误码。
