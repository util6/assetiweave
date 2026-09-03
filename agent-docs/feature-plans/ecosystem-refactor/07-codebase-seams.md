# 源码定位地图

相对仓库根。行号会漂移，以文件和符号定位；这是 2026-09-03 编制时的事实索引，不替代执行时 `rg`。

| 切片 | 生产接缝 | 保留的业务 |
|---|---|---|
| Router | `frontend/src/router/{AppRouter.tsx,routes.ts,routeLoaders.ts,RouteTransition.tsx}` | NavigationModel、会话/Memory 定位 |
| Query | `frontend/src/hooks/catalog/useCatalogData.ts`、`frontend/src/app/backgroundTasks/BackgroundTaskRuntime.tsx`、`frontend/src/lib/asyncCache.ts` | 事件归并、批次刷新、Team 流式投影 |
| Settings | `frontend/src/store/settings/{AppSettingsProvider.tsx,settingsSchema.ts,settingsPersistence.ts}`、`frontend/src/services/appSettings.ts` | theme/font 应用与 SQLite authority |
| i18n | `frontend/src/i18n/{I18nProvider.tsx,messages.ts}` | 旧键、文案、中文 fallback、语言偏好 |
| UI | `frontend/src/components/sources/SourceImportDialog.tsx`、`components/groups/SkillGroupCreateDialog.tsx`、`components/layout/ResizableColumns.tsx`、`components/conversations/ConversationMarkdown.tsx`（后三项同属 frontend/src） | SourceInput、分组约束、overflow、可信 renderer |
| Config/logs | `src-tauri/src/backend/{path_utils.rs,logs.rs,operation_log.rs}`、`runtime/app_runtime.rs`、`application/system.rs`、`src-tauri/src/lib.rs` | 路径发现、Engine stdout、退出落盘 |
| Error/validation | `src-tauri/src/backend/runtime/error.rs`、`ai_execution/error.rs`、`agents/process.rs`、`agent_market/types.rs`、`target_catalog.rs`、`store/team_repo.rs`（省略项同属 backend） | WireError、业务约束和 source |
| Tasks/process | `src-tauri/src/backend/runtime/tasks.rs`、`src-tauri/src/backend/host_process.rs` | 任务状态、冲突、租户、进程清理 |
| HTTP | `src-tauri/src/backend/agent_market/cache.rs`、`agent_market/lifecycle/install.rs`、`application/skill_remote.rs`、`application/conversation_adapter_catalog_v2.rs`、`application/conversation_adapter_installer.rs`、`application/conversation_script_catalog.rs`（省略项同属 backend） | 缓存、来源校验、安装业务和 checksum |
| Existing crates | `src-tauri/src/backend/conversations/io_utils.rs`、`src-tauri/src/backend/store/{team_repo.rs,source_repo.rs}` | banner 提取、历史数据转换 |
| Cross-layer | `src-tauri/src/backend/application/{params.rs,system.rs}`、`src-tauri/src/adapters/tauri/commands.rs`、`src-tauri/src/adapters/engine/{registry.rs,surface_mapping.rs,protocol.rs}`、`cli/internal/schema/contract.json` | AppService 唯一 workflow、生成契约 |

本任务触及 Memory/Recall/Recent 的代码时，先读 `agent-docs/feature-plans/memory-rewrite/00-execution-router.md`；这用于保留当前业务目标，不是冻结其基础设施实现。
