# A-R10：迁移工件下载并移除 ureq

**Depends:** A-R09
**Contracts:** C-BASE、C-HTTP、C-TASK、C-ERROR
**Outcome:** Agent/Adapter工件使用标准HTTP流式落临时文件；原校验/解包/激活规则保留，最后删ureq。

## 执行规则

状态：`PLANNED`。先读总入口、本卡 Contract IDs、`../02-dependencies.md`、`../05-playbook.md`。一轮只做本卡。保真测试先green、采用库/删除旧路径guard再red；测试未命中不算通过。网络测试只用loopback fixture，数据测试只用临时目录/内存SQLite。

## 文件

- Modify：`src-tauri/src/backend/http_client.rs`；`agent_market/lifecycle/install.rs`、`agent_market/installers/binary.rs`；`application/conversation_adapter_installer.rs`；`src-tauri/Cargo.toml`、`Cargo.lock`。
- Test：各同文件内联测试；`agent_market/lifecycle/mod.rs` 现有fixtures；`application/conversation_script_catalog.rs` 现有恶意zip/校验测试仅运行。Create：无。

## 接口

Consumes：A-R08共享client/get_with_redirects；已有 `InstallContext: Clone`（timeout、cancellation、staging_dir）、`Distribution: Clone`、`BinaryInstaller`、sha2、zip/tar/flate2/bzip2。

Produces（创建）：

```rust
pub(crate) struct DownloadSpec<'a> {
    pub(crate) url: &'a str,
    pub(crate) path: &'a std::path::Path,
    pub(crate) max_bytes: u64,
    pub(crate) expected_size: Option<u64>,
    pub(crate) timeout: std::time::Duration,
}
pub(crate) fn download_to_file(
    client: &reqwest::blocking::Client,
    spec: DownloadSpec<'_>,
    cancelled: &dyn Fn() -> bool,
) -> AppResult<u64>;
// BinaryInstaller新方法：
pub(crate) fn materialize_file(
    &self, distribution: &Distribution, context: &InstallContext,
    artifact: &std::path::Path,
) -> Result<MaterializedRuntime, InstallError>;
```

Adapter解包新增私有 `extract_install_artifact_reader<R: std::io::Read + std::io::Seek>(spec: &ConversationAdapterPackageInstallSpec, reader: R, staging_dir: &Path) -> AppResult<PathBuf>`。既有bytes方法只可保留为 `#[cfg(test)]` 的Cursor薄入口，共享同一解包逻辑。

## 步骤

- [ ] 跑当前BinaryInstaller/Adapter zip安全测试、Agent安装rollback测试，保存green；保留512MiB工件上限、展开大小/文件数/路径限制。
- [ ] 写 `download_to_file` loopback测试（与A-R09相同标准库server方式）：返回Content-Length=4/body=1234，max_bytes=3时应报错且不存在partial文件；随后加入 `ureq::` source guard得到red。

```rust
#[test]
fn artifact_production_paths_no_longer_use_ureq_or_buffered_extract() {
    for source in [include_str!("agent_market/lifecycle/install.rs"),
                   include_str!("application/conversation_adapter_installer.rs")] {
        assert!(!source.contains(concat!("ur", "eq::")));
    }
    let source = include_str!("agent_market/installers/binary.rs");
    assert!(source.contains("materialize_file"));
}
```

此测试位于 `backend/http_client.rs::tests`，路径相对backend。

- [ ] helper在创建输出前验证expected_size≤max；每次Read前后检查cancel，累计max+1检测超限，写到调用者的 `.part` 临时文件。任意读取、大小不符、取消、写入错误都删除partial；成功返回字节数。核心循环：

```rust
let mut response = get_with_redirects(client, spec.url,
    reqwest::header::HeaderMap::new(), spec.timeout)?
    .error_for_status().map_err(AppError::external)?;
let mut file = std::fs::File::create(spec.path)?;
let mut count = 0u64;
let mut buffer = [0u8; 8192];
loop {
    if cancelled() { return Err(AppError::Canceled("download cancelled".into())); }
    let read = std::io::Read::read(&mut response, &mut buffer)?;
    if cancelled() { return Err(AppError::Canceled("download cancelled".into())); }
    if read == 0 { break; }
    count += read as u64;
    if count > spec.max_bytes { return Err(AppError::Validation("artifact_size_invalid".into())); }
    std::io::Write::write_all(&mut file, &buffer[..read])?;
}
```

将该循环包在返回Result的闭包中，闭包结束后 `if result.is_err() { remove_file(spec.path) }`；成功前检查expected_size并flush。不是提前return绕过清理。

- [ ] Agent `materialize_and_activate` 的Binary分支clone distribution/context，把**下载+hash+解包**整体放 `tokio::task::spawn_blocking`；Client在closure内取得/释放，JoinError转既有download_failed/install_failed分类。保留调用者per-Agent mutation lease，不移动DB事务或全局AppService锁进网络worker。
- [ ] 新 `materialize_file` 打开临时File，检查metadata大小，流式Sha256验证后seek回0；none通过io::copy，zip将既有extract_zip改为 `R: Read+Seek`，gzip/bzip2用File→Decoder→既有extract_tar。保留每个entry安全判断及取消。生产删除整个Vec下载与Cursor<Vec>入口；bytes仅测试适配。
- [ ] Adapter下载到既有staging目录；先stream hash匹配 expected_artifact_hash，再传 File 给同一泛型zip extractor。保留两阶段路径预验证、Portable filesystem碰撞检测、失败记录和原子激活。
- [ ] fixture验证慢body取消：取消最迟在该请求剩余timeout后观察，测试timeout=200ms，CI上界2s；不承诺瞬时中断。测试失败时旧版本仍active、partial不存在、超限hash错包不进入激活。
- [ ] `rg 'ureq::' src-tauri/src` 生产结果归零后删直接ureq依赖；Cargo lock由cargo更新，禁止手删transitive项。保持reqwest唯一业务HTTP入口。

## 验证

```bash
cargo test -p assetiweave --lib backend::http_client::tests
cargo test -p assetiweave --lib backend::agent_market::installers::binary
cargo test -p assetiweave --lib backend::agent_market::lifecycle -- --test-threads=1
cargo test -p assetiweave --lib conversation_script_catalog
cargo test -p assetiweave --lib conversation_adapter_installer
rg 'ureq::' src-tauri/src
cargo tree -p assetiweave -e normal | grep -E 'reqwest|ureq'
cargo fmt --all -- --check
```

成功：rg无生产ureq；直接依赖删掉；hash/解包/rollback测试不退化；网络流不按整个工件扩张内存；async安装桥及Client最后drop无nested runtime panic。停止：改写包格式/信任策略、移除校验、把hash错误当网络重试、失败后覆盖active包。

[官方 API：blocking Response 实现 Read](https://docs.rs/reqwest/latest/reqwest/blocking/struct.Response.html)
