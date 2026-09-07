# Codebase Seams

本页是定位索引。执行卡拥有修改范围；本页不授予额外 scope。

## AppRuntime / Database

- Runtime ownership：`src-tauri/src/backend/runtime/app_runtime.rs`
- Database/pool/migrations：`src-tauri/src/backend/store/database.rs`
- Task lifecycle：`src-tauri/src/backend/runtime/tasks.rs`
- Runtime behavior tests：`src-tauri/src/backend/runtime/tests.rs`
- Tauri/Engine process entries：`src-tauri/src/lib.rs`、`src-tauri/src/bin/`
- Adapter surface：`src-tauri/src/adapters/tauri/`、`src-tauri/src/adapters/engine/`

定位命令：

```bash
rg -n 'Database|AppRuntime|build_runtime|\.block_on\(|\.run_sync\(' src-tauri/src --glob '*.rs'
```

## Domain Event Dispatcher

- Event contract/consumer context：`src-tauri/src/backend/events/mod.rs`
- Scheduling implementation：`src-tauri/src/backend/events/dispatcher.rs`
- Built-in consumers：`src-tauri/src/backend/events/consumers.rs`
- Durable behavior tests：`src-tauri/src/backend/events/tests.rs`

定位命令：

```bash
rg -n 'EventDispatcher|WakeSignal|Completion|Condvar|retry|domain_event_outbox|consumer_offsets' src-tauri/src/backend/events src-tauri/src/backend/runtime
```

## HostProcess / Agent Process

- Bounded host commands：`src-tauri/src/backend/host_process.rs`
- Long-lived Agent process：`src-tauri/src/backend/agents/process.rs`
- Extension launch：`src-tauri/src/backend/extension_kernel/launcher.rs`
- Agent execution consumers：`src-tauri/src/backend/ai_execution/`、`src-tauri/src/backend/agent_market/`

定位命令：

```bash
rg -n 'run_host_command|run_program|ManagedAgentProcess|taskkill|process_group|setpgid|stdout|stderr' src-tauri/src/backend --glob '*.rs'
```

## Settings

- Document canonicalization：`src-tauri/src/backend/app_settings.rs`
- SQLite persistence：`src-tauri/src/backend/store/settings_repo.rs`
- AppService operations：`src-tauri/src/backend/application/system.rs`
- Tauri/Engine commands：adapter settings/system registry consumers

定位命令：

```bash
rg -n 'app_settings|load_app_settings|save_app_settings|serde_json::Value|\.get\(' src-tauri/src/backend src-tauri/src/adapters --glob '*.rs'
```

## Logging

- Subscriber/guard：`src-tauri/src/backend/logging.rs`
- LogSnapshot/read/open/panic：`src-tauri/src/backend/logs.rs`
- Legacy field façade：`src-tauri/src/backend/operation_log.rs`
- Process entry initialization：`src-tauri/src/lib.rs`

定位命令：

```bash
rg -n 'log_info|log_warn|log_error|record_info|record_warn|record_error|tracing::|#\[tracing::instrument' src-tauri/src --glob '*.rs'
```

## Paths / Filesystem

- Portable path contract：`src-tauri/src/backend/host_paths.rs`
- Host filesystem policy：`src-tauri/src/backend/host_filesystem.rs`
- App path consumers：`src-tauri/src/backend/path_utils.rs`
- Target detection：`src-tauri/src/backend/targeting.rs`、`src-tauri/src/backend/target_catalog.rs`

定位命令：

```bash
rg -n 'to_string_lossy|PathBuf::from|replace\(.+\\\\|canonicalize|dirs::|HostPathResolver|PortableRelativePath' src-tauri/src/backend --glob '*.rs'
```

## SQLx Rows

- Repository modules：`src-tauri/src/backend/store/`
- Shared codec：`src-tauri/src/backend/store/codec.rs`
- SQL constants：`src-tauri/src/backend/store/sql.rs`

定位命令：

```bash
rg -n 'try_get|query_as|FromRow|serde_json::from_str' src-tauri/src/backend/store --glob '*.rs'
```

## Existing High Seams

1. AppRuntime lifecycle tests：启动、角色差异、任务与关闭。
2. Event tests：outbox 原子性、initial position、failure isolation、bounded shutdown。
3. HostProcess fixture：超时、取消、大输出、后代管道与回收。
4. AppSettings tests：canonicalization、未知字段、legacy migration、locale 并发初始化。
5. Log tests：脱敏、panic 可见、写入路径与 LogSnapshot。
6. Repository/AppService tests：临时 SQLite、tenant 隔离、排序和 JSON 行为。
7. Engine contract/surface matrix/CLI e2e：跨 surface 公开行为。

