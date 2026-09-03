# A-R08：建立 reqwest blocking 客户端与确定的生命周期

**Depends:** A-R02、A-R04、A-R07
**Contracts:** C-BASE、C-HTTP、C-TASK、C-ERROR
**Outcome:** 复用标准连接池并明确同步/async桥；本卡提供HTTP接缝，不迁移全部消费者。

## 执行规则

状态：`PLANNED`。先读总入口、本卡 Contract IDs、`../02-dependencies.md`、`../05-playbook.md`。一轮只做本卡。保真测试先green、采用库/删除旧路径guard再red；测试未命中不算通过。网络测试只用loopback fixture，数据测试只用临时目录/内存SQLite。

## 文件

- Create：`src-tauri/src/backend/http_client.rs`，仅client装配、既有redirect兼容规则和内联fixture测试。
- Modify：`src-tauri/src/backend/mod.rs`、`src-tauri/Cargo.toml`、`Cargo.lock`。
- Test：`http_client.rs::tests`、`runtime/tests.rs`；不新增HTTP服务框架或async runtime。

## 接口（全部创建）

```rust
pub(crate) fn shared_http_client() -> AppResult<reqwest::blocking::Client>;
pub(crate) fn get_with_redirects(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: reqwest::header::HeaderMap,
    timeout: std::time::Duration,
) -> AppResult<reqwest::blocking::Response>;
```

`shared_http_client` 返回 Clone，共享库的连接池。使用一个进程级 `static Mutex<Option<Client>>`；普通线程/已有 spawn_blocking worker 内首次构建，锁只覆盖构建/clone，不覆盖网络。静态 owner 保留到进程结束，Rust static 不运行 Drop，不另建关闭/重新打开状态机；进程退出时系统释放空闲连接池。活动请求的接受/取消/排空继续归 TaskRuntime，不归 HTTP 模块。

调用 clone 在 blocking 作用域内使用/释放；测试直接使用私有 builder，在同一 blocking 作用域释放最后一个 Client，验证库运行时约束。禁止在 async runtime 线程首次懒加载 blocking Client。全局资源测试可使用独立子进程，但普通单测不要清空进程共享静态状态影响其他测试。

## 已核验兼容点

当前 ureq2.12.1 默认5跳、`RedirectAuthHeaders::Never`、未启用proxy-from-env（本仓库仅json，ureq默认tls/gzip）。本轮保持：**不自动代理**，每次重定向删除Authorization，最多5跳；保留gzip响应解码。reqwest用 `no_proxy()`、`redirect(Policy::none())`，不用默认10跳/自动代理。`system-proxy` 不作为新增产品行为开启；锁表若不同，先纠正冲突再实施。

`get_with_redirects` 只适配上述已存在语义，HTTP解析、TLS、连接池与body均由reqwest负责。301/302/303/307/308才跟随；304原样返回；相对Location用Url::join；每跳剩余timeout从单一deadline计算；第6跳报错；重定向后移除Authorization，保留其余调用者既有headers。来源允许列表仍由对应领域检查，不能删Agent catalog的最终URL校验。

## 步骤与具体代码

- [ ] 保留ureq直依赖，查锁表加入reqwest blocking/json/rustls/gzip；先用旧fixture确认5跳/Authorization策略。
- [ ] 添加新接缝测试，未创建方法时编译red；再用source guard确认不再每次构建client。

```rust
#[test]
fn client_can_be_built_used_and_dropped_in_blocking_worker() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        tokio::task::spawn_blocking(|| {
            let client = build_http_client().unwrap();
            let second = client.clone();
            drop(second);
            drop(client); // 最后一个owner在blocking作用域销毁
        }).await.unwrap();
    });
}
```

- [ ] 私有builder使用真实库API，不包自研Request/Response：

```rust
fn build_http_client() -> AppResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(None)
        .user_agent(concat!("AssetIWeave/", env!("CARGO_PKG_VERSION")))
        .build().map_err(AppError::external)
}
```

- [ ] 每次get helper设置请求timeout（catalog 15s，artifact沿context最多10min）；builder无默认总timeout避免偷偷改变旧下载上限。

```rust
let remaining = deadline.saturating_duration_since(std::time::Instant::now());
if remaining.is_zero() { return Err(AppError::Timeout("HTTP request timed out".into())); }
let response = client.get(current.clone()).headers(headers.clone())
    .timeout(remaining).send().map_err(AppError::external)?;
if response.status() == reqwest::StatusCode::NOT_MODIFIED { return Ok(response); }
// 跟随标准redirect前：
headers.remove(reqwest::header::AUTHORIZATION);
```

- [ ] fixture内用std TcpListener返回200/304/相对302/6跳和gzip；记录headers，证明任何redirect下一跳都没有Authorization。请求错误不调用mock；用两次请求验证共享Client可复用连接。
- [ ] 确认所有 HTTP 调用属于现有后台任务/同步 Engine 作用域；关闭由 TaskRuntime 停止接纳和排空。HTTP 模块没有第二个 accepting/active/shutdown registry；静态连接池仅为进程级库资源。

## 验证

```bash
cargo test -p assetiweave --lib backend::http_client::tests -- --nocapture
cargo test -p assetiweave --lib backend::runtime::tests
cargo tree -p assetiweave -e features -i reqwest
cargo fmt --all -- --check
```

成功：fixture全绿；独立Client的创建/使用/最后drop都在blocking作用域；共享静态owner按进程生命周期保留；无新Tokio运行时；ureq仍留给后续卡。停止：执行者计划在Tokio async线程直接初始化LazyLock client、跨请求保存含凭据的默认header、把loopback测试改成外网请求。

[reqwest blocking API](https://docs.rs/reqwest/latest/reqwest/blocking/)；[ureq2既有默认规则源码](https://github.com/algesten/ureq/blob/2.12.1/src/agent.rs)
