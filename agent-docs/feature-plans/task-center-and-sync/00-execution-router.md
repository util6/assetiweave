# 执行路由：统一任务中心、结构化后台进度与 Conversation Adapter 并发同步 (Issue #33)

## 1. 目标与事实源

本专项落地 Issue #33，交付：
1. 进程内公开任务中心（Task Center）模型与 AppService 收口；
2. Conversation Adapter 分组并发（最多 4 组）、单组 Source 串行、确定性聚合与 partial_success 语义；
3. Conversation Adapter NDJSON 可选 progress 消息扩展与内置 Adapter 升级；
4. Memory 任务多阶段（领取、加载、Agent、校验、发布）脱敏投影；
5. 侧边栏“任务、日志、设置”次级导航收口、活动/失败徽标与侧边栏锚定气泡通知；
6. 设置“通用 → 通知”面板与 `showTaskNotifications` 控制；
7. 清理旧右下角悬浮任务卡。

事实源优先级：
- 代码与测试事实 > GitHub Issue #33 > `CONTEXT.md` > 本目录文档。

## 2. 文档导航

- `01-contract.md`：公开 Task View 契约、NDJSON progress 协议与设置契约
- `02-codebase-seams.md`：代码库接缝与分层边界
- `03-ticket-map.md`：工单与实施步骤分解（T01–T16）
- `04-verification-matrix.md`：验证矩阵与门禁检查清单
- `07-progress.md`：增量进度记录表
