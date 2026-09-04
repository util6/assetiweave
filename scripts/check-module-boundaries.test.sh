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

# Test runtime bridge monotonic baseline checks
TEST_BASELINE="$ROOT/test-bridge-baseline.txt"
printf '%s\t%s\n' "2" "src-tauri/src/existing.rs" > "$TEST_BASELINE"

# 1. New file with runtime bridge hit is rejected
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src"
printf '%s\n' 'let _ = rt.block_on(async {});' > "$ROOT/src-tauri/src/untracked_file.rs"
if BOUNDARY_ALLOWLIST="$TEST_BASELINE" BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/new-bridge-file.out" 2>&1; then
  cat "$ROOT/new-bridge-file.out"
  printf '%s\n' "self-test failed: untracked runtime bridge file was accepted"
  exit 1
fi

# 2. Existing file with increased count (3 > 2) is rejected
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src"
cat << 'EOF' > "$ROOT/src-tauri/src/existing.rs"
let _ = rt.block_on(async {});
let _ = rt.block_on(async {});
let _ = rt.block_on(async {});
EOF
if BOUNDARY_ALLOWLIST="$TEST_BASELINE" BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/increased-bridge-count.out" 2>&1; then
  cat "$ROOT/increased-bridge-count.out"
  printf '%s\n' "self-test failed: increased runtime bridge count was accepted"
  exit 1
fi

# 3. Existing file with decreased count (1 <= 2) is accepted
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src"
cat << 'EOF' > "$ROOT/src-tauri/src/existing.rs"
let _ = rt.block_on(async {});
EOF
if ! BOUNDARY_ALLOWLIST="$TEST_BASELINE" BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/decreased-bridge-count.out" 2>&1; then
  cat "$ROOT/decreased-bridge-count.out"
  printf '%s\n' "self-test failed: decreased runtime bridge count was rejected"
  exit 1
fi

printf '%s\n' 'module boundary self-tests passed'
