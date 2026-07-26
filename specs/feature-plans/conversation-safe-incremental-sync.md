# Spec: Conversation 安全增量同步

## Objective

把 Conversation 同步从“来源快照镜像”调整为“本地历史归档的追加与更新”。源 Session 被归档、删除、超出来源列表上限或暂时不可访问时，已导入的本地内容必须继续可浏览、搜索和导出；旧 Session 重新产生内容时，系统必须识别其版本变化并补充最新记录。

默认同步采用两阶段模型：完整分页发现轻量 Session 元数据，再只读取新建或版本变化的 active Session。完整内容校验是显式兜底模式，也不得因为来源缺失而删除本地历史。

## Assumptions

1. 同一来源内的 `external_id` 是 Session 的稳定身份；Adapter 若无法保证，必须提供稳定的 `source_locator` 或父子关系，不能静默合并。
2. “active”表示新建、版本变化、读取失败待重试或读取期间继续变化，不等同于最近若干天。
3. 同步只能读取第三方来源；来源归档和删除不会触发本地内容删除。
4. 用户显式删除本地 Conversation 是独立能力，不属于本同步改造。
5. 第三方来源只能保证已持久化内容可被发现，尚未落盘或尚未出现在远端接口中的内容由后续同步补齐。

## Contract

Adapter 协议继续使用同一版本并以可选字段增量扩展：

- `list_sessions` 分页返回 Session 描述符：`external_id`、`updated_at`、`source_locator`、`version_token`。
- 列表响应必须声明 `snapshot_complete`；分页、权限或网络失败时必须为 `false`。
- Core 将描述符与 SQLite 中保存的最后成功版本比较，得到 active Session 集合。
- `read_session(session_id)` 每次返回指定 Session 的完整标准化内容；Core 仅对 active Session 调用。
- 读取前后的版本不一致时，Session 保持 dirty，不能推进已同步版本。
- 旧 Adapter 只支持 `read_session(null)` 时，Core 保留兼容路径，但按“保留式全量导入”处理，不因未返回记录删除本地历史。

SQLite 将内容生命周期与来源观测分离：

- Conversation 内容默认永久保留。
- 来源未返回只更新观测状态，不删除、不隐藏内容。
- `last_seen_at` 表示最近一次在完整元数据发现中被看到。
- `source_version` 表示最近一次成功写入内容对应的来源版本。
- `source_presence` 使用 `present`、`absent`、`unknown`；扫描不完整时不能从 `present` 推导为 `absent`。

## Commands

- Rust 定向测试：`cargo test --workspace conversation_incremental`
- Rust 全量验证：`cargo fmt --all -- --check && cargo test --workspace`
- 前端验证：`pnpm typecheck && pnpm test && pnpm build`
- Adapter 校验：`pnpm conversation-adapters:check`
- Engine/CLI 契约：`pnpm cli:contract && go vet -C cli ./... && go test -C cli -race ./...`

## Project Structure

- `src-tauri/src/backend/conversations/`：Adapter 两阶段协议、输出校验和 active Session 编排。
- `src-tauri/src/backend/store/`：来源观测、保留式导入和幂等内容更新。
- `src-tauri/migrations/`：来源观测字段或表的 schema 演进。
- `parser-catalog/adapters/`、`src-tauri/bundled/conversation-adapters/`：官方 Adapter 的元数据发现与按 ID 读取。
- `frontend/src/services/`、`frontend/src/pages/conversations/`：同步结果类型和用户可见状态。
- `cli/internal/schema/contract.json`：由 Rust Engine contract 生成，不手工编辑。

## Code Style

Rust 使用现有 `AppResult`、SQLx repository 和 `serde` snake_case 契约；TypeScript 使用严格类型、双引号、分号和两空格缩进。新增协议使用可辨识枚举和可选字段扩展现有结构，不创建并行 v2 模块。

```rust
match discovery.snapshot_complete {
    true => record_complete_observation(&mut tx, descriptors).await?,
    false => record_partial_observation(&mut tx, descriptors).await?,
}
```

## Testing Strategy

- Store 集成测试使用临时 SQLite，先证明当前代码会隐藏或删除缺失 Session，再实现保留语义。
- 协议单元测试覆盖分页完成标记、版本比较、旧 Session 重新活跃和读取期间版本变化。
- Adapter fixture 测试覆盖无变化零详情读取、新 Session、旧 Session 追加内容以及不完整列表。
- 应用层测试覆盖兼容 Adapter 与增量 Adapter 的同一同步入口。
- 前端测试覆盖同步统计和来源不可见记录仍可浏览。

## Boundaries

- Always：来源只读；同步后台化；事务成功后才推进版本；不完整快照禁止推导删除。
- Ask first：新增自动永久删除策略、跨来源自动合并 Session、引入后台实时 watcher。
- Never：因来源未返回、分页上限、归档或删除而清除本地 Conversation；手工编辑生成的 CLI contract。

## Success Criteria

1. Session 和 Web Record 在来源不再返回后仍保留、可列出、可读取、可搜索、可导出。
2. 默认同步不会从 Conversation 内容表删除来源缺失记录。
3. 完整元数据发现能够识别新 Session，以及任意年龄但版本发生变化的旧 Session。
4. 未变化 Session 不执行完整内容读取或数据库内容重写。
5. 不完整列表、Adapter 失败或同步事务失败不会推进版本或改变来源存在状态。
6. 读取期间继续变化的 Session 会重试或保持 dirty，下一次同步可补齐。
7. 旧 Adapter 保持可用，并自动获得“不删除历史”的安全语义。
8. Rust、前端、Adapter、Engine contract 和 Go CLI 验证全部通过。

## Open Questions

- 某些第三方来源若不能提供可靠 `updated_at`，第一阶段使用文件 `mtime + size`、消息尾部标记或轻量指纹；来源专有 change feed 留待后续优化。
- 来源明确区分 archived/deleted 时可以保存更精确状态；无法区分时统一使用 `absent/unknown`，不能猜测删除原因。
