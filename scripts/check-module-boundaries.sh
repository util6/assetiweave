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

# Monotonic migration baselines from SPEC-01/SPEC-02. A touched module may
# reduce these counts, but cannot silently add new synchronous bridges or
# legacy string-error construction sites.
check_max 333 'block_on' src-tauri/src
check_max 18 'Legacy\(' src-tauri/src
check_max 0 'open_with_db_path' src-tauri/src/adapters

rm -f /tmp/assetiweave-boundary-check.out
if [ "$fail" -ne 0 ]; then
  exit 1
fi
printf '%s\n' 'module boundary checks passed'
