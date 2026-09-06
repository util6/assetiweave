# ERR-C01：Conversation Projection 与 LogSnapshot Typed Error

**Authority:** Conversation projection validation 与日志浏览分别拥有 typed error；跨模块不返回 String error。

**Files:**

- Modify/Test: `src-tauri/src/backend/projection/conversation_cards.rs`
- Modify/Test: `src-tauri/src/backend/projection/conversation_content_nodes.rs`
- Modify/Test: `src-tauri/src/backend/logs.rs`
- Modify callers only where compiler requires conversion at Application/adapter boundary.

## Steps

- [ ] 分类三个模块的 `Result<T,String>`：schema/kind/renderer 是 ProjectionError；路径/读取/打开/紧急写入是 LogAccessError。
- [ ] 增加未知 card schema、非法 renderer、日志路径逃逸、日志读取 I/O 的行为测试；断言原有成功结果不变、失败保留 typed source 并安全映射。
- [ ] 运行测试记录当前 String error 缺少 source/code 的 RED。
- [ ] 使用 `thiserror` 定义两个局部 error enum；私有纯 parser 也返回所属 typed error，避免 caller 重新解析字符串。
- [ ] AppService/adapter 只在领域边界转换为 AppError/WireError。
- [ ] 三个模块公开/跨模块 `Result<T,String>` 为零；测试锁 helper 可保留并在交接注明 test-only。

## Verify

```bash
cargo test -p assetiweave backend::projection -- --nocapture
cargo test -p assetiweave backend::logs -- --nocapture
cargo test -p assetiweave projection_and_log_error_wire_parity -- --nocapture
cargo fmt --all -- --check
pnpm check:boundaries
```

提交：`refactor(errors): 类型化投影与日志访问错误`
