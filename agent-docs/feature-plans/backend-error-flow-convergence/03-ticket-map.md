# 错误链单卡队列

| 顺序 | Card | 唯一结果 | 前置 |
|---:|---|---|---|
| 1 | [ERR-00](tickets/ERR-00-baseline-guard.md) | 当前命中完整分类并建立防增长守卫 | Issue #24 B2-G03 |
| 2 | [ERR-D01](tickets/ERR-D01-database-codec-sources.md) | SQLx/Serde source 无损进入 AppError | ERR-00 |
| 3 | [ERR-A01](tickets/ERR-A01-agent-market-errors.md) | Agent Market 跨模块 typed error | ERR-D01 |
| 4 | [ERR-C01](tickets/ERR-C01-projection-log-errors.md) | Conversation projection 与 LogSnapshot typed error | ERR-A01 |
| 5 | [ERR-E01](tickets/ERR-E01-app-error-wire.md) | AppError taxonomy 收紧且 WireError parity 保持 | ERR-C01 |
| 6 | [ERR-G01](tickets/ERR-G01-acceptance.md) | 全量行为、删除与契约验收 | ERR-E01 |

默认串行。ERR-D01 与 Issue #24 的 SQLx 卡、ERR-C01 与 Issue #24 的日志卡修改相同区域，因此整个 Issue #2 必须等待 #24 完成。
