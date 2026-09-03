# A-R09：迁移 catalog 与 GitHub JSON 请求

**Depends:** A-R08
**Contracts:** C-BASE、C-HTTP、C-ERROR
**Outcome:** 四组只读catalog/JSON请求使用共享reqwest；ETag、缓存与本地文件fallback保真。

## 执行规则

状态：`PLANNED`。先读总入口、本卡 Contract IDs、`../02-dependencies.md`、`../05-playbook.md`。一轮只做本卡。保真测试先green、采用库/删除旧路径guard再red；测试未命中不算通过。网络测试只用loopback fixture，数据测试只用临时目录/内存SQLite。

## 文件与接口

- Modify/Test：`src-tauri/src/backend/agent_market/cache.rs`；`backend/application/conversation_adapter_catalog_v2.rs`、`conversation_script_catalog.rs`、`skill_remote.rs`。
- Read：`backend/http_client.rs`，A-R08输出；已有后台任务入口与Engine registry证明这些调用在同步worker，不改业务流程。
- Create：无。
- Consumes：`shared_http_client() -> AppResult<Client>`、`get_with_redirects(&Client,&str,HeaderMap,Duration) -> AppResult<Response>`。
- Produces：原 `CatalogCache::refresh_default() -> Result<CatalogRefreshOutcome,String>`、`fetch_catalog_document(&str,Option<&str>) -> AppResult<CatalogFetchResult>`、`fetch_catalog_text(&str) -> AppResult<String>`、`github_get_json(&str,&str) -> AppResult<Value>` 签名不变。可在各同文件增加接收 `&Client/url` 的私有测试接缝，不导出新的Repository。

## 步骤

- [ ] 跑Agent cache现有测试，确认缓存etag/版本选择/fingerprint行为green；Catalog v2本地路径分支继续走原fs读取，不把本地文件伪装HTTP URL。
- [ ] 在Catalog v2文件内加入loopback测试（fixture返回 `HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n`）；调用 `fetch_catalog_document` 应返回NotModified。再加source deletion guard得到red：

```rust
#[test]
fn catalog_fetch_keeps_not_modified() {
    use std::{io::{Read, Write}, net::TcpListener, thread};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let n = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..n]).to_lowercase().contains("if-none-match:"));
        stream.write_all(b"HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n").unwrap();
    });
    assert!(matches!(fetch_catalog_document(&format!("http://{address}/index.json"), Some("etag-1")).unwrap(), CatalogFetchResult::NotModified));
    server.join().unwrap();
}
#[test]
fn catalog_document_uses_reqwest_not_ureq() {
    let source = include_str!("conversation_adapter_catalog_v2.rs");
    assert!(!source.contains(concat!("ur", "eq::")));
}
```

- [ ] 迁移请求并**先处理304再error_for_status**，代码核心：

```rust
let client = crate::backend::http_client::shared_http_client()?;
let mut headers = reqwest::header::HeaderMap::new();
if let Some(etag) = etag {
    headers.insert(reqwest::header::IF_NONE_MATCH,
        reqwest::header::HeaderValue::from_str(etag).map_err(AppError::external)?);
}
let response = crate::backend::http_client::get_with_redirects(
    &client, url, headers, std::time::Duration::from_secs(15))?;
if response.status() == reqwest::StatusCode::NOT_MODIFIED {
    return Ok(CatalogFetchResult::NotModified);
}
let response = response.error_for_status().map_err(AppError::external)?;
let etag = response.headers().get(reqwest::header::ETAG)
    .and_then(|v| v.to_str().ok()).map(str::to_owned);
let text = response.text().map_err(AppError::external)?;
Ok(CatalogFetchResult::Text { text, etag })
```

- [ ] Agent cache保留5MiB body cap，读取max+1以检测超限；保留最终response.url的GitHub允许列表、无缓存304为错误、原原子写缓存；两个Conversation catalog继续原验证和本地默认回退，不把格式错误标为成功。
- [ ] GitHub JSON的Accept和Bearer仍每个request构造，不存共享Client；库response.json处理反序列化；重定向后的凭据剥离继承A-R08。
- [ ] 查所有四入口调用链：若某Tauri async command直接同步执行网络，则仅把该现有service调用包在已有spawn_blocking中，并报告新增Modify文件；不在AppService持锁期间await或创建另一个runtime。该文件超出本卡清单先按停止协议更新卡，禁止悄悄扩张。
- [ ] 添加HTTP500、无效JSON、etag缺失、超限和gzip fixture；断言旧缓存未被无效响应覆盖。删除四文件内ureq imports/calls；Artifact调用留A-R10。

## 验证

```bash
cargo test -p assetiweave --lib backend::agent_market::cache
cargo test -p assetiweave --lib catalog_fetch_keeps_not_modified
cargo test -p assetiweave --lib catalog_document_uses_reqwest_not_ureq
cargo test -p assetiweave --lib conversation_adapter_catalog_v2
cargo test -p assetiweave --lib conversation_script_catalog
cargo test -p assetiweave --lib skill_remote
cargo fmt --all -- --check
```

成功：四文件生产ureq调用为零，所有ETag/缓存/凭据/错误fixture绿，catalog不再同步阻塞async runtime线程。停止：为了换Client重写整个catalog模型或批准新远程来源。

[官方 API：reqwest Response](https://docs.rs/reqwest/latest/reqwest/blocking/struct.Response.html)
