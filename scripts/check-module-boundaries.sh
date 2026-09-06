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
  count=$(grep -R -n -E --include='*.rs' "$pattern" "$scope" 2>/dev/null | wc -l | tr -d ' ')
  if [ "$count" -gt "$baseline" ]; then
    printf '%s\n' "BOUNDARY VIOLATION: $pattern count $count exceeds baseline $baseline"
    fail=1
  fi
}

# C-PROCESS-01 & B2-P03C: host_process must not retain sync runner primitives
check_absent 'libc::kill|process_group\(0\)|std::thread::spawn|thread::sleep|std::process::Command' \
  "$ROOT/src-tauri/src/backend/host_process.rs"

# Tauri wrappers must reuse the process runtime and keyed locks, not reopen a
# database or serialize all commands behind the removed global mutex.
check_absent 'state\.lock|AppService::open_with_db_path' "$ROOT/src-tauri/src/adapters"

# The store boundary must not materialize official adapter files or invoke
# filesystem-backed adapter discovery during normal SQL persistence.
check_absent 'ensure_official_conversation_adapters' "$ROOT/src-tauri/src/backend/store/conversation_repo.rs"

# Store must not reach application/bootstrap or non-type conversation modules.
check_absent 'backend::application' "$ROOT/src-tauri/src/backend/store"
check_absent 'backend::conversations::(official|external|harvester|io_utils|package)' \
  "$ROOT/src-tauri/src/backend/store"

# Models remain dependency-free from business/application modules.
check_absent 'backend::(store|application|capabilities|conversations|scanner|planner|executor)' \
  "$ROOT/src-tauri/src/backend/models"

# Projection is a neutral read-model layer: no business modules or filesystem/
# process APIs may leak back into it.
check_absent 'backend::(store|application|capabilities|conversations|scanner|planner|executor|search|agent_market|agents|ai_execution)' \
  "$ROOT/src-tauri/src/backend/projection"
check_absent 'std::fs|tokio::fs|std::process|crate::adapters' \
  "$ROOT/src-tauri/src/backend/projection"

# Generated contract metadata remains the only risk/confirmation source.
check_absent 'SurfaceMapping.*risk|SurfaceMapping.*confirmation' \
  "$ROOT/src-tauri/src/adapters/engine/surface_mapping.rs"

# Application code must use host/runtime seams for platform-sensitive work and
# must not retain a provider-specific Gemini process path.
check_absent 'std::process::Command|process::Command|Command::new' \
  "$ROOT/src-tauri/src/backend/application"
check_absent 'legacy_gemini' "$ROOT/src-tauri/src/backend/application"

# Catalog v2 consumes the version-neutral installer spec directly. A legacy
# catalog item may remain behind the compatibility mapper, but not in v2.
check_absent 'ConversationScriptCatalogItem|ConversationScriptCatalogSource' \
  "$ROOT/src-tauri/src/backend/application/conversation_adapter_catalog_v2.rs"

# The migrated translation application module uses the runtime error contract
# explicitly instead of inheriting the DTO String alias through the prelude.
check_absent '(^|[^A-Za-z0-9_])AppResult<' \
  "$ROOT/src-tauri/src/backend/application/card_translation.rs"
check_absent 'dto::AppResult' \
  "$ROOT/src-tauri/src/backend/application/card_translation.rs"

# Card translation is the first backend/domain typed-error vertical slice;
# transport-facing String conversion must stay outside the backend module.
check_absent 'dto::AppResult' "$ROOT/src-tauri/src/backend/card_translation.rs"
check_absent 'Result<[^>]*, ?String>' "$ROOT/src-tauri/src/backend/card_translation.rs"

# Installer Core is a module-level boundary: it consumes InstallSpec directly
# and must not import legacy Script Catalog item/source types anywhere.
check_absent 'ConversationScriptCatalog(Item|Source)' \
  "$ROOT/src-tauri/src/backend/application/conversation_adapter_installer.rs"
check_absent 'pub\(super\) fn install_conversation_adapter_package_from_spec' \
  "$ROOT/src-tauri/src/backend/application/conversation_script_catalog.rs"

# TaskRuntime is the only lifecycle authority. The extension coordinator may
# translate keys, but it must not grow a second reservation map or projection
# cleanup path.
check_absent 'collections::(HashMap|BTreeMap)|sync::.*(Mutex|RwLock)' \
  "$ROOT/src-tauri/src/backend/extension_kernel/lifecycle.rs"
check_absent 'lifecycle\.(release|finish_projection)' \
  "$ROOT/src-tauri/src/adapters/tauri/background_tasks.rs"

# The service boundary is runtime-backed in both production and test builders.
check_absent 'runtime: Option<Arc<AppRuntime>>' \
  "$ROOT/src-tauri/src/backend/application/service.rs"

# Runtime is the lower-layer resource owner. Keep the dependency direction
# executable even while the compatibility bootstrap wrapper remains in the
# Application module.
check_absent 'backend::application|crate::backend::application' \
  "$ROOT/src-tauri/src/backend/runtime"
check_absent 'impl From<String> for AppError|impl From<&str> for AppError' \
  "$ROOT/src-tauri/src/backend/runtime/error.rs"

# Application workflows and their prelude must not consume the DTO transport
# alias or its explicitly named legacy infrastructure alias.
check_absent 'dto::.*(AppResult|LegacyResult)|dto::\{[^}]*([^A-Za-z0-9_]|^)(AppResult|LegacyResult)([^A-Za-z0-9_]|$)' \
  "$ROOT/src-tauri/src/backend/application"
check_absent 'type (AppResult|LegacyResult)<T> = Result<T, String>' \
  "$ROOT/src-tauri/src/backend/dto"

# Agent Market must preserve typed errors across the Application boundary.
check_absent 'AppError::Legacy|map_err\([^)]*to_string' \
  "$ROOT/src-tauri/src/backend/application/agent_market.rs"

# BA-020 removes the provider-specific CLI execution compatibility seam.
check_max 0 'legacy_gemini' "$ROOT/src-tauri/src/backend/ai_execution"
check_max 0 'configured_agent_capability' "$ROOT/src-tauri/src/backend/ai_execution"
check_max 0 'AiCliRuntime' "$ROOT/src-tauri/src/backend/ai_execution"
check_max 0 'AiStructuredTextRequest' "$ROOT/src-tauri/src/backend/ai_execution"
check_max 0 'execute_structured_text' "$ROOT/src-tauri/src/backend/ai_execution"
check_max 0 'run_cli_command' "$ROOT/src-tauri/src/backend/ai_execution"

# TargetCatalog is runtime-owned. Production defaults/path/application code
# must receive the active catalog instead of constructing a hidden builtin.
check_max 0 'TargetCatalog::builtin\(' "$ROOT/src-tauri/src/backend/app_paths.rs"
check_max 0 'TargetCatalog::builtin\(' "$ROOT/src-tauri/src/backend/defaults.rs"
check_absent 'TargetCatalog::builtin\(' "$ROOT/src-tauri/src/backend/application"

# Monotonic migration baselines from SPEC-01/SPEC-02.
# In B2-R14, all backend modules have been fully migrated to async with 0 block_on.
check_max 20 'block_on' "$ROOT/src-tauri/src"
check_max 0 'Legacy\(' "$ROOT/src-tauri/src"
check_absent '(^|[^A-Za-z0-9_])LegacyResult([^A-Za-z0-9_]|$)' \
  "$ROOT/src-tauri/src"
check_max 0 'open_with_db_path' "$ROOT/src-tauri/src/adapters"
# Application has no explicit Result<T, String> declarations or legacy result
# aliases; compatibility string errors stay below the application boundary.
check_max 0 'Result<[^>]*, ?String>' "$ROOT/src-tauri/src/backend/application"
check_absent '(^|[^A-Za-z0-9_])LegacyResult([^A-Za-z0-9_]|$)' \
  "$ROOT/src-tauri/src/backend/application"
check_absent 'type (AppResult|LegacyResult)<T> = Result<T, String>' \
  "$ROOT/src-tauri/src/backend/dto"

# Keep synchronous bridges monotonic per module, not only in the aggregate.
# All backend modules are strictly zero.
while IFS='|' read -r scope baseline; do
  [ -z "$scope" ] && continue
  case "$scope" in '#'*) continue ;; esac
  check_max "$baseline" 'block_on' "$ROOT/$scope"
done <<'EOF'
src-tauri/src/adapters|10
src-tauri/src/backend/agent_market|0
src-tauri/src/backend/ai_execution|0
src-tauri/src/backend/application|0
src-tauri/src/backend/capabilities|0
src-tauri/src/backend/data_backup.rs|0
src-tauri/src/backend/events|0
src-tauri/src/backend/runtime|0
src-tauri/src/backend/search|0
src-tauri/src/backend/store|0
src-tauri/src/backend/target_catalog.rs|0
EOF

# Monotonic error flow guards (Issue #2 / ERR-00)
# 1. Global bounds prevent untyped and string-mapped error expansion
check_max 80 'Result<[^>]*, ?String>' "$ROOT/src-tauri/src"
check_max 850 'map_err\(AppError::external\)' "$ROOT/src-tauri/src/backend"
check_max 77 'AppError::External\([^)]*\.to_string\(\)' "$ROOT/src-tauri/src/backend"

# 2. Runtime layer must not introduce cross-module String results
check_absent 'Result<[^>]*, ?String>' "$ROOT/src-tauri/src/backend/runtime"

# 3. Runtime layer must not expand map_err(AppError::external), and runtime core forbids it entirely
check_max 13 'map_err\(AppError::external\)' "$ROOT/src-tauri/src/backend/runtime"
check_absent 'map_err\(AppError::external\)' "$ROOT/src-tauri/src/backend/runtime/app_runtime.rs"
check_absent 'map_err\(AppError::external\)' "$ROOT/src-tauri/src/backend/runtime/error.rs"

# 4. Known typed errors must not be mapped to string losing source
check_absent '(sqlx::Error|io::Error|HostProcessError).*to_string\(\)' "$ROOT/src-tauri/src/backend/runtime"

# Frontend architecture boundaries: enforce services-only Tauri IPC via ESLint
if [ -f "$ROOT/package.json" ]; then
  if ! (cd "$ROOT" && node scripts/lint-architecture.mjs); then
    fail=1
  fi
fi

# Runtime bridge zero-match check (Issue #24 / B2-R14)
# 1. Backend code must never contain runtime bridges (block_on/run_sync/Tokio Runtime)
BACKEND_BRIDGE_HITS=$(grep -R -n -E --include='*.rs' '\.(block_on|run_sync)\(|tokio::runtime::Runtime' "$ROOT/src-tauri/src/backend" 2>/dev/null || true)
if [ -n "$BACKEND_BRIDGE_HITS" ]; then
  printf '%s\n' "RUNTIME BRIDGE VIOLATION: backend code contains runtime bridges (zero-tolerance):"
  printf '%s\n' "$BACKEND_BRIDGE_HITS"
  fail=1
fi

# 2. Non-entry files must not construct Tokio Runtime or call block_on/run_sync
NON_ENTRY_BRIDGE_HITS=$(grep -R -n -E --include='*.rs' '\.(block_on|run_sync)\(|tokio::runtime::Runtime' "$ROOT/src-tauri/src" 2>/dev/null \
  | grep -v -E "src-tauri/src/(lib|main)\.rs" || true)
if [ -n "$NON_ENTRY_BRIDGE_HITS" ]; then
  printf '%s\n' "RUNTIME BRIDGE VIOLATION: non-entry files contain runtime bridges:"
  printf '%s\n' "$NON_ENTRY_BRIDGE_HITS"
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
printf '%s\n' 'module boundary checks passed'

