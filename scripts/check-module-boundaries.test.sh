#!/bin/sh
set -eu

ROOT=$(mktemp -d "${TMPDIR:-/tmp}/assetiweave-boundaries.XXXXXX")
trap 'rm -rf "$ROOT"' EXIT HUP INT TERM

run_clean_fixture() {
  BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >/dev/null
}

run_rejected_fixture() {
  name=$1
  path=$2
  content=$3
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$(dirname "$ROOT/$path")"
  printf '%s\n' "$content" >"$ROOT/$path"
  if BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/$name.out" 2>&1; then
    cat "$ROOT/$name.out"
    printf '%s\n' "self-test failed: $name fixture was accepted"
    exit 1
  fi
}

run_clean_fixture
run_rejected_fixture runtime-dependency \
  src-tauri/src/backend/runtime/mod.rs \
  'use crate::backend::application::AppService;'
run_rejected_fixture application-command \
  src-tauri/src/backend/application/mod.rs \
  'let _command = std::process::Command::new("fixture");'
run_rejected_fixture agent-market-legacy \
  src-tauri/src/backend/application/agent_market.rs \
  'let _error = AppError::Legacy("fixture".to_string());'
run_rejected_fixture application-legacy-result-alias \
  src-tauri/src/backend/application/prelude.rs \
  'use crate::backend::dto::LegacyResult;'
run_rejected_fixture dto-result-alias \
  src-tauri/src/backend/dto/types.rs \
  'pub(crate) type AppResult<T> = Result<T, String>;'
run_rejected_fixture dto-legacy-result-alias \
  src-tauri/src/backend/dto/types.rs \
  'pub(crate) type LegacyResult<T> = Result<T, String>;'
run_rejected_fixture new-legacy-site \
  src-tauri/src/backend/runtime/new_error.rs \
  'let _error = AppError::Legacy("fixture".to_string());'
run_rejected_fixture new-legacy-result-site \
  src-tauri/src/backend/runtime/new_result.rs \
  'fn fixture() -> LegacyResult<()> { Ok(()) }'
run_rejected_fixture new-sync-bridge \
  src-tauri/src/backend/target_catalog.rs \
  'tokio::runtime::Handle::current().block_on(async {});'
run_rejected_fixture application-target-catalog-bypass \
  src-tauri/src/backend/application/service.rs \
  'let _catalog = TargetCatalog::builtin();'
run_rejected_fixture host-process-libc-kill \
  src-tauri/src/backend/host_process.rs \
  'unsafe { libc::kill(1, 9); }'
run_rejected_fixture host-process-process-group-zero \
  src-tauri/src/backend/host_process.rs \
  'command.process_group(0);'
run_rejected_fixture host-process-thread-spawn \
  src-tauri/src/backend/host_process.rs \
  'std::thread::spawn(|| {});'
run_rejected_fixture host-process-thread-sleep \
  src-tauri/src/backend/host_process.rs \
  'thread::sleep(Duration::from_millis(15));'
run_rejected_fixture host-process-std-command \
  src-tauri/src/backend/host_process.rs \
  'let _ = std::process::Command::new("sh");'
run_rejected_fixture error-boundary-cross-module-string \
  src-tauri/src/backend/runtime/new_service.rs \
  'pub fn fixture_op() -> Result<(), String> { Ok(()) }'
run_rejected_fixture error-boundary-map-err-external \
  src-tauri/src/backend/runtime/app_runtime.rs \
  'let _ = op().map_err(AppError::external);'
run_rejected_fixture error-boundary-known-typed-to-string \
  src-tauri/src/backend/runtime/new_service.rs \
  'let _ = op().map_err(|e: sqlx::Error| e.to_string());'

# Test runtime bridge zero-match checks (Issue #24 / B2-R14)
# 1. Backend file with runtime bridge is rejected
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src/backend"
printf '%s\n' 'let _ = rt.block_on(async {});' > "$ROOT/src-tauri/src/backend/store.rs"
if BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/backend-bridge.out" 2>&1; then
  cat "$ROOT/backend-bridge.out"
  printf '%s\n' "self-test failed: backend runtime bridge was accepted"
  exit 1
fi

# 2. Non-entry file with runtime bridge is rejected
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src"
printf '%s\n' 'let _ = rt.block_on(async {});' > "$ROOT/src-tauri/src/untracked.rs"
if BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/non-entry-bridge.out" 2>&1; then
  cat "$ROOT/non-entry-bridge.out"
  printf '%s\n' "self-test failed: non-entry runtime bridge was accepted"
  exit 1
fi

# 3. Entry file (src-tauri/src/lib.rs) with runtime bridge is accepted
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src"
printf '%s\n' 'let _ = rt.block_on(async {});' > "$ROOT/src-tauri/src/lib.rs"
if ! BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/entry-bridge.out" 2>&1; then
  cat "$ROOT/entry-bridge.out"
  printf '%s\n' "self-test failed: entry runtime bridge was rejected"
  exit 1
fi

printf '%s\n' 'module boundary self-tests passed'
