#!/bin/sh
set -eu

ROOT=${BOUNDARY_ROOT:-.}
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CHECK_OUTPUT=${TMPDIR:-/tmp}/assetiweave-boundary-check.$$.out
trap 'rm -f "$CHECK_OUTPUT"' EXIT HUP INT TERM

fail=0
check_absent() {
  pattern=$1
  scope=$2
  if grep -R -n -E "$pattern" "$scope" >"$CHECK_OUTPUT" 2>/dev/null; then
    cat "$CHECK_OUTPUT"
    fail=1
  fi
}

check_max() {
  baseline=$1
  pattern=$2
  scope=$3
  grep -R -n -E --include='*.rs' "$pattern" "$scope" >"$CHECK_OUTPUT" 2>/dev/null || true
  count=$(wc -l <"$CHECK_OUTPUT" | tr -d ' ')
  if [ "$count" -gt "$baseline" ]; then
    cat "$CHECK_OUTPUT"
    printf '%s\n' "BOUNDARY VIOLATION: $pattern count $count exceeds baseline $baseline"
    fail=1
  fi
}

check_absent_production() {
  pattern=$1
  scope=$2
  if ! node "$SCRIPT_DIR/check-rust-production-pattern.mjs" "$scope" "$pattern" >"$CHECK_OUTPUT" 2>&1; then
    cat "$CHECK_OUTPUT"
    fail=1
  fi
}

check_absent_flattened_file() {
  pattern=$1
  file=$2
  if [ ! -f "$file" ]; then
    return 0
  fi
  if tr '\n' ' ' <"$file" | grep -o -E "$pattern" >"$CHECK_OUTPUT" 2>/dev/null; then
    printf '%s: ' "$file"
    cat "$CHECK_OUTPUT"
    fail=1
  fi
}

require_path() {
  path=$1
  if [ "${BOUNDARY_ALLOW_MISSING_ALLOWLIST:-0}" = "1" ]; then
    return 0
  fi
  if [ ! -e "$path" ]; then
    printf '%s\n' "BOUNDARY CONFIGURATION ERROR: canonical path does not exist: $path"
    fail=1
  fi
}

# Canonical paths for the in-progress Issue #44 migration. Keeping these
# checks explicit prevents a moved/deleted legacy path from silently disabling
# a boundary rule.
if [ -d "$ROOT/backend/asset_core/src" ]; then
  BACKEND_ROOT="$ROOT/backend/asset_core/src"
else
  BACKEND_ROOT="$ROOT/src-tauri/src/backend"
fi

if [ -d "$ROOT/desktop/src" ]; then
  DESKTOP_ROOT="$ROOT/desktop/src"
else
  DESKTOP_ROOT="$ROOT/src-tauri/src"
fi

require_path "$BACKEND_ROOT/application"
require_path "$BACKEND_ROOT/domain"
require_path "$BACKEND_ROOT/infrastructure"
require_path "$BACKEND_ROOT/store"
require_path "$BACKEND_ROOT/infrastructure/host_process.rs"
if [ -d "$ROOT/backend/agent_engine" ]; then
  require_path "$ROOT/backend/agent_engine/src/infrastructure/process/managed_process.rs"
  require_path "$ROOT/backend/agent_engine/src/infrastructure/protocol/acp.rs"
  require_path "$ROOT/backend/agent_engine/src/infrastructure/registry.rs"
else
  require_path "$BACKEND_ROOT/infrastructure/agent_execution/managed_process.rs"
  require_path "$BACKEND_ROOT/infrastructure/agent_execution/protocol/acp.rs"
  require_path "$BACKEND_ROOT/infrastructure/agent_execution/registry.rs"
fi
require_path "$BACKEND_ROOT/infrastructure/runtime"
require_path "$BACKEND_ROOT/infrastructure/extensions"
require_path "$BACKEND_ROOT/domain/memory/evidence"
require_path "$BACKEND_ROOT/domain/mounting/planning.rs"

# Enable only for the final structural acceptance; the migration phase keeps
# this disabled until the legacy roots have actually been retired.
if [ "${BOUNDARY_REQUIRE_FOUR_BACKEND_ROOTS:-1}" = "1" ]; then
  if [ -f "$BACKEND_ROOT/mod.rs" ] || [ "${BOUNDARY_ALLOW_MISSING_ALLOWLIST:-0}" != "1" ]; then
    if ! node "$SCRIPT_DIR/check-backend-root-layout.mjs" "$ROOT" >"$CHECK_OUTPUT" 2>&1; then
      cat "$CHECK_OUTPUT"
      fail=1
    fi
  fi
fi

# The backend root declares layer/context namespaces only; shared types must
# be imported from their owning module rather than re-exported across layers.
check_absent '^[[:space:]]*pub([[:space:]]*\([^)]*\))?[[:space:]]+use[[:space:]]' \
  "$BACKEND_ROOT/mod.rs"

# Domain must remain pure business logic and free of application, store, infra, dto, runtime IO, or env reads
check_absent_production '--domain-purity' \
  "$BACKEND_ROOT/domain"

# Cross-layer glob imports are forbidden in production code; explicit symbols are required
check_absent_production '--cross-layer-glob' \
  "$BACKEND_ROOT"

# Infrastructure must not re-export or declare bridge modules for Domain/Store
check_absent_production '--infra-bridge' \
  "$BACKEND_ROOT/infrastructure"

# WireError belongs to the Adapter/DTO boundary; Infrastructure must not construct it
check_absent 'WireError' \
  "$BACKEND_ROOT/infrastructure"

# C-PROCESS-01 & B2-P03C: host_process must not retain sync runner primitives
check_absent 'libc::kill|process_group\(0\)|std::thread::spawn|thread::sleep|std::process::Command' \
  "$BACKEND_ROOT/infrastructure/host_process.rs"

# Tauri wrappers must reuse the process runtime and keyed locks, not reopen a
# database or serialize all commands behind the removed global mutex.
check_absent_production 'state\.lock|AppService::open_with_db_path' "$DESKTOP_ROOT/adapters"

# Tauri commands must route application-facing log operations through AppService.
check_absent_production 'AgentExecutionRuntime|AiExecutionRequest|state\.agent_runtime|runtime\.execute\(' \
  "$DESKTOP_ROOT/adapters/tauri"
check_absent 'agent_runtime[[:space:]]*:' "$DESKTOP_ROOT/adapters/app_state.rs"
check_absent_flattened_file 'backend::(infrastructure::)?logs::logs_(get_snapshot|open_log_directory|write_operation)' \
  "$DESKTOP_ROOT/adapters/tauri/commands.rs"
check_absent_flattened_file 'backend::(infrastructure::)?logs([[:space:]]+as[[:space:]]+[A-Za-z_][A-Za-z0-9_]*|[[:space:]]*[,;}])' \
  "$DESKTOP_ROOT/adapters/tauri/commands.rs"
check_absent_flattened_file 'backend::(infrastructure::)?[[:space:]]*\{[^;]*(^|[^[:alnum:]_])logs[[:space:]]*::[[:space:]]*\{[^}]*logs_(get_snapshot|open_log_directory|write_operation)' \
  "$DESKTOP_ROOT/adapters/tauri/commands.rs"
check_absent_flattened_file 'backend::(infrastructure::)?[[:space:]]*\{[^;]*(^|[^[:alnum:]_])logs[[:space:]]*[,}]' \
  "$DESKTOP_ROOT/adapters/tauri/commands.rs"

# The store boundary must not materialize official adapter files or invoke
# filesystem-backed adapter discovery during normal SQL persistence.
check_absent 'ensure_official_conversation_adapters' "$BACKEND_ROOT/store/conversations"

# Store must not reach application/bootstrap or non-type conversation modules.
check_absent_production 'backend::application|crate::backend::application' "$BACKEND_ROOT/store"
check_absent_production 'backend::conversations::(official|external|harvester|io_utils|package)' \
  "$BACKEND_ROOT/store"

# Scan every production Rust file in both lower layers and keep reverse
# dependencies at the final zero-reference target.
if ! BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS=1 \
  node "$SCRIPT_DIR/check-backend-layer-dependencies.mjs" "$ROOT"; then
  fail=1
fi

# Capabilities facade is deleted; zero tolerance across the entire codebase.
check_absent 'backend::capabilities|crate::backend::capabilities' "$DESKTOP_ROOT"

# Extension kernel compatibility facade is deleted; zero tolerance across the entire codebase.
check_absent 'backend::extension_kernel|crate::backend::extension_kernel' "$DESKTOP_ROOT"
check_absent 'backend::planner|crate::backend::planner' "$DESKTOP_ROOT"
check_absent 'backend::executor|crate::backend::executor' "$DESKTOP_ROOT"

# Legacy top-level models, bootstrap, evidence are deleted; zero tolerance across the entire codebase.
check_absent 'backend::models|crate::backend::models' "$DESKTOP_ROOT"
check_absent 'backend::bootstrap|crate::backend::bootstrap' "$DESKTOP_ROOT"
check_absent 'backend::evidence|crate::backend::evidence' "$DESKTOP_ROOT"

# Projection is a neutral read-model layer: no business modules or filesystem/
# process APIs may leak back into it.
check_absent 'backend::(store|application|capabilities|conversations|scanner|planner|executor|search|agent_market|agents|ai_execution)' \
  "$BACKEND_ROOT/projection"
check_absent 'std::fs|tokio::fs|std::process|crate::adapters' \
  "$BACKEND_ROOT/projection"

# Generated contract metadata remains the only risk/confirmation source.
check_absent 'SurfaceMapping.*risk|SurfaceMapping.*confirmation' \
  "$DESKTOP_ROOT/adapters/engine/surface_mapping.rs"

# Application code must use host/runtime seams for platform-sensitive work and
# must not retain a provider-specific Gemini process path.
check_absent 'std::process::Command|process::Command|Command::new' \
  "$BACKEND_ROOT/application"
check_absent 'legacy_gemini' "$BACKEND_ROOT/application"

# Catalog v2 consumes the version-neutral installer spec directly. A legacy
# catalog item may remain behind the compatibility mapper, but not in v2.
check_absent 'ConversationScriptCatalogItem|ConversationScriptCatalogSource' \
  "$BACKEND_ROOT/application/conversations/conversation_adapter_catalog_v2.rs"

# The migrated translation application module uses the runtime error contract
# explicitly instead of inheriting the DTO String alias through the prelude.
check_absent '(^|[^A-Za-z0-9_])AppResult<' \
  "$BACKEND_ROOT/application/conversations/card_translation.rs"
check_absent 'dto::AppResult' \
  "$BACKEND_ROOT/application/conversations/card_translation.rs"

# Card translation is the first backend/domain typed-error vertical slice;
# transport-facing String conversion must stay outside the backend module.
check_absent 'dto::AppResult' "$BACKEND_ROOT/ai_execution/card_translation.rs"
check_absent 'Result<[^>]*, ?String>' "$BACKEND_ROOT/ai_execution/card_translation.rs"

# Installer Core is a module-level boundary: it consumes InstallSpec directly
# and must not import legacy Script Catalog item/source types anywhere.
check_absent 'ConversationScriptCatalog(Item|Source)' \
  "$BACKEND_ROOT/application/conversations/conversation_adapter_installer.rs"
check_absent 'pub\(super\) fn install_conversation_adapter_package_from_spec' \
  "$BACKEND_ROOT/application/conversations/conversation_script_catalog.rs"

# TaskRuntime is the only lifecycle authority. The extension coordinator may
# translate keys, but it must not grow a second reservation map or projection
# cleanup path.
check_absent 'collections::(HashMap|BTreeMap)|sync::.*(Mutex|RwLock)' \
  "$BACKEND_ROOT/infrastructure/extensions/lifecycle.rs"
check_absent 'lifecycle\.(release|finish_projection)' \
  "$DESKTOP_ROOT/adapters/tauri/background_tasks.rs"

# The service boundary is runtime-backed in both production and test builders.
check_absent 'runtime: Option<Arc<AppRuntime>>' \
  "$BACKEND_ROOT/application/service.rs"

# Application owns startup sequencing; Runtime accepts prepared resources and
# owns runtime state without depending back on Application.
check_absent_production 'backend::application|crate::backend::application' \
  "$BACKEND_ROOT/infrastructure/runtime"
# Runtime owns process resources, not startup use-case sequencing. The
# Application bootstrap opens the database, loads the catalog and seeds
# tenant defaults before constructing this runtime.
check_absent_production 'store::open_migrated_pool|ensure_app_library_dirs|TargetCatalog::load_with_overrides|seed_defaults_with_catalog|seed_tenant_defaults_with_catalog|reconcile_tenants_with_target_catalog|ensure_official_conversation_adapters|materialize_and_seed_builtin_adapters|recover_startup|migrate_legacy_assignments|prepare_startup_health_refresh' \
  "$BACKEND_ROOT/infrastructure/runtime/app_runtime.rs"
check_absent_production 'backend::application|crate::backend::application' \
  "$BACKEND_ROOT/infrastructure/tasks"
check_absent_production 'backend::application|crate::backend::application' \
  "$BACKEND_ROOT/infrastructure/events"
check_absent_production 'backend::application|crate::backend::application' \
  "$BACKEND_ROOT/infrastructure/extensions"
check_absent 'backend::runtime|crate::backend::runtime' \
  "$DESKTOP_ROOT"
check_absent 'impl From<String> for AppError|impl From<&str> for AppError' \
  "$BACKEND_ROOT/error/app_error.rs"

# Application workflows and their prelude must not consume the DTO transport
# alias or its explicitly named legacy infrastructure alias.
check_absent 'dto::.*(AppResult|LegacyResult)|dto::\{[^}]*([^A-Za-z0-9_]|^)(AppResult|LegacyResult)([^A-Za-z0-9_]|$)' \
  "$BACKEND_ROOT/application"
check_absent 'type (AppResult|LegacyResult)<T> = Result<T, String>' \
  "$BACKEND_ROOT/dto"

# Agent Market must preserve typed errors across the Application boundary.
check_absent 'AppError::Legacy|map_err\([^)]*to_string' \
  "$BACKEND_ROOT/application/agents"

# BA-020 removes the provider-specific CLI execution compatibility seam.
check_max 0 'legacy_gemini' "$BACKEND_ROOT/ai_execution"
check_max 0 'configured_agent_capability' "$BACKEND_ROOT/ai_execution"
check_max 0 'AiCliRuntime' "$BACKEND_ROOT/ai_execution"
check_max 0 'AiStructuredTextRequest' "$BACKEND_ROOT/ai_execution"
check_max 0 'execute_structured_text' "$BACKEND_ROOT/ai_execution"
check_max 0 'run_cli_command' "$BACKEND_ROOT/ai_execution"

# Managed child-process ownership lives under Infrastructure, not the legacy
# Agent definitions/protocol namespace. Check direct and grouped imports in
# the two execution backends that consume this process seam.
check_absent_flattened_file 'backend::agents::process|agents[[:space:]]*::[[:space:]]*\{[^}]*process[[:space:]]*::' \
  "$BACKEND_ROOT/ai_execution/backends/acp.rs"
check_absent_flattened_file 'backend::agents::process|agents[[:space:]]*::[[:space:]]*\{[^}]*process[[:space:]]*::' \
  "$BACKEND_ROOT/ai_execution/backends/native.rs"
check_absent_production 'backend::agents::types|crate::backend::agents::types' \
  "$BACKEND_ROOT/infrastructure/agent_execution"
check_absent_flattened_file 'backend::agents::protocol|agents[[:space:]]*::[[:space:]]*\{[^}]*protocol[[:space:]]*::' \
  "$BACKEND_ROOT/ai_execution/backends/acp.rs"
check_absent_flattened_file 'backend::agents::protocol|agents[[:space:]]*::[[:space:]]*\{[^}]*protocol[[:space:]]*::' \
  "$BACKEND_ROOT/ai_execution/backends/acp_aggregator.rs"
check_absent_flattened_file 'backend::agents::registry|agents[[:space:]]*::[[:space:]]*\{[^}]*registry[[:space:]]*::' \
  "$BACKEND_ROOT/ai_execution/executor.rs"
check_absent_flattened_file 'backend::agents::registry|agents[[:space:]]*::[[:space:]]*\{[^}]*registry[[:space:]]*::' \
  "$BACKEND_ROOT/ai_execution/executor_tests.rs"
check_absent_flattened_file 'backend::agents::registry|agents[[:space:]]*::[[:space:]]*\{[^}]*registry[[:space:]]*::' \
  "$BACKEND_ROOT/ai_execution/card_translation_tests.rs"
check_absent_flattened_file 'backend::agents::registry|agents[[:space:]]*::[[:space:]]*\{[^}]*registry[[:space:]]*::' \
  "$BACKEND_ROOT/agent_market/runtime.rs"
check_absent_flattened_file 'backend::agents::registry|agents[[:space:]]*::[[:space:]]*\{[^}]*registry[[:space:]]*::' \
  "$BACKEND_ROOT/agent_market/lifecycle/install.rs"

# The builtin catalog is used only to recognize the previous defaults during
# the one-time Application migration; runtime path/default code consumes the
# active catalog assembled by Application bootstrap.
check_max 0 'TargetCatalog::builtin\(' "$BACKEND_ROOT/infrastructure/host_paths.rs"
check_absent_production 'TargetCatalog::builtin\(' "$BACKEND_ROOT/application"
check_absent_production 'TargetCatalog::builtin\(' "$BACKEND_ROOT/infrastructure"

# Monotonic migration baselines from SPEC-01/SPEC-02.
# In B2-R14, all backend modules have been fully migrated to async with 0 block_on.
check_max 23 'block_on' "$DESKTOP_ROOT"
check_max 0 'Legacy\(' "$DESKTOP_ROOT"
check_absent '(^|[^A-Za-z0-9_])LegacyResult([^A-Za-z0-9_]|$)' \
  "$DESKTOP_ROOT"
check_absent_production 'open_with_db_path' "$DESKTOP_ROOT/adapters"
# Application has no explicit Result<T, String> declarations or legacy result
# aliases; compatibility string errors stay below the application boundary.
check_max 0 'Result<[^>]*, ?String>' "$BACKEND_ROOT/application"
check_absent '(^|[^A-Za-z0-9_])LegacyResult([^A-Za-z0-9_]|$)' \
  "$BACKEND_ROOT/application"
check_absent 'type (AppResult|LegacyResult)<T> = Result<T, String>' \
  "$BACKEND_ROOT/dto"

# Keep synchronous bridges monotonic per module, not only in the aggregate.
# All backend modules are strictly zero.
while IFS='|' read -r scope baseline; do
  [ -z "$scope" ] && continue
  case "$scope" in '#'*) continue ;; esac
  check_max "$baseline" 'block_on' "$ROOT/$scope"
done <<'EOF'
desktop/src/adapters|10
backend/asset_core/src/agent_market|0
backend/asset_core/src/ai_execution|0
backend/asset_core/src/application|0
backend/asset_core/src/infrastructure/backup|0
backend/asset_core/src/infrastructure/events|0
backend/asset_core/src/infrastructure/runtime|0
backend/asset_core/src/search|0
backend/asset_core/src/store|0
backend/asset_core/src/application/mounting/target_catalog.rs|0
EOF

# Monotonic error flow guards (Issue #2 / ERR-00)
# 1. Global bounds prevent untyped and string-mapped error expansion
check_max 80 'Result<[^>]*, ?String>' "$DESKTOP_ROOT"
check_max 970 'map_err\(AppError::external\)' "$BACKEND_ROOT"
check_max 77 'AppError::External\([^)]*\.to_string\(\)' "$BACKEND_ROOT"

# 2. Runtime layer must not introduce cross-module String results
check_absent 'Result<[^>]*, ?String>' "$BACKEND_ROOT/infrastructure/runtime"

# 3. Runtime layer must not expand map_err(AppError::external), and runtime core forbids it entirely
check_max 13 'map_err\(AppError::external\)' "$BACKEND_ROOT/infrastructure/runtime"
check_absent 'map_err\(AppError::external\)' "$BACKEND_ROOT/infrastructure/runtime/app_runtime.rs"
check_absent 'map_err\(AppError::external\)' "$BACKEND_ROOT/error/app_error.rs"

# 4. Known typed errors must not be mapped to string losing source
check_absent '(sqlx::Error|io::Error|HostProcessError).*to_string\(\)' "$BACKEND_ROOT/infrastructure/runtime"

# Frontend architecture boundaries: enforce services-only Tauri IPC via ESLint
if [ -f "$ROOT/package.json" ]; then
  if ! (cd "$ROOT" && node scripts/lint-architecture.mjs); then
    fail=1
  fi
fi

# Runtime bridge zero-match check (Issue #24 / B2-R14)
# 1. Backend code must never contain runtime bridges (block_on/run_sync/Tokio Runtime)
BACKEND_BRIDGE_HITS=$(grep -R -n -E --include='*.rs' '\.(block_on|run_sync)\(|tokio::runtime::Runtime' "$BACKEND_ROOT" 2>/dev/null || true)
if [ -n "$BACKEND_BRIDGE_HITS" ]; then
  printf '%s\n' "RUNTIME BRIDGE VIOLATION: backend code contains runtime bridges (zero-tolerance):"
  printf '%s\n' "$BACKEND_BRIDGE_HITS"
  fail=1
fi

# 2. Non-entry files must not construct Tokio Runtime or call block_on/run_sync
NON_ENTRY_BRIDGE_HITS=$(grep -R -n -E --include='*.rs' '\.(block_on|run_sync)\(|tokio::runtime::Runtime' "$DESKTOP_ROOT" 2>/dev/null \
  | grep -v -E "(desktop|src-tauri)/src/(lib|main)\.rs" || true)
if [ -n "$NON_ENTRY_BRIDGE_HITS" ]; then
  printf '%s\n' "RUNTIME BRIDGE VIOLATION: non-entry files contain runtime bridges:"
  printf '%s\n' "$NON_ENTRY_BRIDGE_HITS"
  fail=1
fi

# Backend line limit check: enforce max 500 lines for production files
if [ -d "$BACKEND_ROOT" ]; then
  if ! node "$SCRIPT_DIR/check-backend-line-limits.mjs" "$ROOT"; then
    fail=1
  fi
fi

# Every boundary run prints the per-module retirement table. The accepted
# dirty-worktree snapshot, rather than HEAD, is the monotonic baseline so
# unrelated user work already present when the migration began is preserved.
if [ "${BOUNDARY_REQUIRE_FOUR_BACKEND_ROOTS:-1}" = "1" ] || \
  [ "${BOUNDARY_SKIP_LEGACY_REPORT:-0}" != "1" ]; then
  if [ -f "$BACKEND_ROOT/mod.rs" ] || [ "${BOUNDARY_ALLOW_MISSING_ALLOWLIST:-0}" != "1" ]; then
    if [ "${BOUNDARY_REQUIRE_FOUR_BACKEND_ROOTS:-1}" = "1" ]; then
      if ! BOUNDARY_REQUIRE_NO_LEGACY=1 \
        BOUNDARY_RETIRED_MODULES=agent_market,agents,ai_execution,conversations,dto,error,executor,planner,projection,scanner,search \
        node "$SCRIPT_DIR/report-backend-legacy-modules.mjs" "$ROOT"; then
        fail=1
      fi
    elif ! BOUNDARY_RETIRED_MODULES="${BOUNDARY_RETIRED_MODULES:-planner,executor,conversations,scanner,search}" \
      node "$SCRIPT_DIR/report-backend-legacy-modules.mjs" "$ROOT"; then
      fail=1
    fi
  fi
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
printf '%s\n' 'module boundary checks passed'
