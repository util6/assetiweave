#!/bin/sh
set -eu

fail=0
check_absent() {
  pattern=$1
  scope=$2
  if grep -R -n -E "$pattern" "$scope" >/tmp/assetiweave-boundary-check.out 2>/dev/null; then
    cat /tmp/assetiweave-boundary-check.out
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

# Tauri wrappers must reuse the process runtime and keyed locks, not reopen a
# database or serialize all commands behind the removed global mutex.
check_absent 'state\.lock|AppService::open_with_db_path' src-tauri/src/adapters

# The store boundary must not materialize official adapter files or invoke
# filesystem-backed adapter discovery during normal SQL persistence.
check_absent 'ensure_official_conversation_adapters' src-tauri/src/backend/store/conversation_repo.rs

# Store must not reach application/bootstrap or non-type conversation modules.
check_absent 'backend::application' src-tauri/src/backend/store
check_absent 'backend::conversations::(official|external|harvester|io_utils|package)' src-tauri/src/backend/store

# Models remain dependency-free from business/application modules.
check_absent 'backend::(store|application|capabilities|conversations|scanner|planner|executor)' src-tauri/src/backend/models

# Projection is a neutral read-model layer: no business modules or filesystem/
# process APIs may leak back into it.
check_absent 'backend::(store|application|capabilities|conversations|scanner|planner|executor|search|agent_market|agents|ai_execution)' src-tauri/src/backend/projection
check_absent 'std::fs|tokio::fs|std::process|crate::adapters' src-tauri/src/backend/projection

# Generated contract metadata remains the only risk/confirmation source.
check_absent 'SurfaceMapping.*risk|SurfaceMapping.*confirmation' src-tauri/src/adapters/engine/surface_mapping.rs

# Application code must use host/runtime seams for platform-sensitive work and
# must not retain a provider-specific Gemini process path.
check_absent 'std::process::Command|process::Command|Command::new' src-tauri/src/backend/application
check_absent 'legacy_gemini' src-tauri/src/backend/application

# Catalog v2 consumes the version-neutral installer spec directly. A legacy
# catalog item may remain behind the compatibility mapper, but not in v2.
check_absent 'ConversationScriptCatalogItem|ConversationScriptCatalogSource' \
  src-tauri/src/backend/application/conversation_adapter_catalog_v2.rs

# The migrated translation application module uses the runtime error contract
# explicitly instead of inheriting the DTO String alias through the prelude.
check_absent '(^|[^A-Za-z0-9_])AppResult<' \
  src-tauri/src/backend/application/card_translation.rs
check_absent 'dto::AppResult' \
  src-tauri/src/backend/application/card_translation.rs

# Installer Core consumes InstallSpec directly. The legacy catalog item may
# only appear in the reverse-mapping wrapper and catalog discovery code.
if sed -n '/pub(super) fn install_conversation_adapter_package_from_spec/,/^struct InstalledConversationAdapterPackage/p' \
  src-tauri/src/backend/application/conversation_script_catalog.rs \
  | grep -n -E 'ConversationScriptCatalogItem|ConversationScriptCatalogSource'; then
  printf '%s\n' 'BOUNDARY VIOLATION: Installer Core still depends on legacy catalog types'
  fail=1
fi

# The service boundary is runtime-backed in both production and test builders.
check_absent 'runtime: Option<Arc<AppRuntime>>' src-tauri/src/backend/application/service.rs

# Monotonic migration baselines from SPEC-01/SPEC-02. A touched module may
# reduce these counts, but cannot silently add new synchronous bridges or
# legacy string-error construction sites.
check_max 333 'block_on' src-tauri/src
check_max 18 'Legacy\(' src-tauri/src
check_max 0 'open_with_db_path' src-tauri/src/adapters
# Application has no explicit Result<T, String> declarations; the remaining
# DTO alias is a tracked compatibility seam until the transport-wide migration.
check_max 0 'Result<[^>]*, ?String>' src-tauri/src/backend/application
check_max 1 'type AppResult<T> = Result<T, String>' src-tauri/src/backend/dto

rm -f /tmp/assetiweave-boundary-check.out
if [ "$fail" -ne 0 ]; then
  exit 1
fi
printf '%s\n' 'module boundary checks passed'
