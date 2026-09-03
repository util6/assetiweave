# A-R07：移除 HostProcess 取消镜像线程

**Depends:** A-R06
**Contracts:** C-BASE、C-TASK、C-ERROR
**Outcome:** CancellationToken直接进入现有进程控制循环，删除token→AtomicBool watcher；不重写进程管理协议。

## 执行规则

状态：`PLANNED`。先读总入口、本卡 Contract IDs、`../02-dependencies.md`、`../05-playbook.md`。一轮只做本卡。原有正确行为先 characterization green；随后新增 adoption/deletion guard 得到 red，再迁移。筛选测试先 `-- --list`，零测试不算 green。只用临时目录/内存库/loopback fixture；本卡不授权插件架构或真实用户数据操作。

## 文件与接口

- Modify/Test：`src-tauri/src/backend/host_process.rs`。若内部HostProcessControl被外部模块构造，先 `rg HostProcessControl src-tauri/src` 并报告；当前控制结构与构造点集中本文件。
- Create：无。
- Consumes（已有）：`run_host_command_with_cancellation(HostCommandSpec, Option<&CancellationToken>) -> Result<HostCommandOutput, HostProcessError>`、`run_host_command(HostCommandSpec, CancellationToken) -> Future<Output=Result<HostCommandOutput,HostProcessError>>`。
- Produces：上面签名不变；内部新增借用适配枚举，只连接两种既有取消源：

```rust
#[derive(Clone, Copy)]
enum HostCancellation<'a> {
    Atomic(&'a std::sync::atomic::AtomicBool),
    Token(&'a tokio_util::sync::CancellationToken),
}
impl HostCancellation<'_> {
    fn is_cancelled(self) -> bool {
        match self {
            Self::Atomic(flag) => flag.load(std::sync::atomic::Ordering::Acquire),
            Self::Token(token) => token.is_cancelled(),
        }
    }
}
```

`HostProcessControl.cancellation` 改为 `Option<HostCancellation<'a>>`；保留 `run_host_command_blocking_with_cancellation` 对AtomicBool的原入口，仅在构建control时包装Atomic，保护现有安装器调用者。

## 步骤

- [ ] 跑现有HostProcess超时、取消、进程树回收、stdout/stderr cap测试，记录green。
- [ ] 新增deletion guard，旧 `watcher_done` 必须red：

```rust
#[test]
fn token_cancellation_has_no_mirror_watcher_thread() {
    let source = include_str!("host_process.rs");
    assert!(!source.contains(concat!("let watcher_", "done =")));
    assert!(!source.contains(concat!("let watcher_", "token =")));
}
#[test]
fn token_view_observes_cancellation_without_copying_state() {
    let token = tokio_util::sync::CancellationToken::new();
    let view = HostCancellation::Token(&token);
    assert!(!view.is_cancelled());
    token.cancel();
    assert!(view.is_cancelled());
}
```

- [ ] 将同步token入口直接构建control，调用原 `run_command_with_control_and_input`；保留stdin bytes/EOF、timeout、输出上限。删除watcher thread、done Atomic、10ms mirror loop及join。
- [ ] `run_host_command` 的spawn_blocking closure持有clone token，在control内借用；外层select收到取消后仍await worker完成进程组terminate→grace→kill→reap，不提前返回泄漏子进程。
- [ ] 原is_cancelled helper改成 `cancellation.is_some_and(HostCancellation::is_cancelled)`；原Atomic调用点显式包装。保持轮询进程退出的原周期，本卡不创造新的CancellationRuntime。
- [ ] 用现有 `fixture_command("timeout")` 或现有fixture spec跑取消，断言返回Cancelled且子进程已回收；再测预取消不spawn、超时和输出超限依然走原分类。用测试计数/guard证明没有每次额外watcher线程，而非依赖操作系统线程总数。

## 删除与验证

```bash
cargo test -p assetiweave --lib backend::host_process::tests -- --nocapture
cargo test -p assetiweave --lib backend::extension_kernel
cargo test -p assetiweave --lib backend::agent_market::installers
cargo fmt --all -- --check
```

成功：镜像线程删净；token传播测试、超时/进程树回收/Atomic旧入口均绿。停止：需要更改进程协议或全安装器签名、取消时未回收进程、把token触发等同于操作已经终止。

[官方 API：CancellationToken](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html)
