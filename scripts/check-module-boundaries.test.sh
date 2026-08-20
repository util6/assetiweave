#!/bin/sh
set -eu

ROOT=$(mktemp -d "${TMPDIR:-/tmp}/assetiweave-boundaries.XXXXXX")
trap 'rm -rf "$ROOT"' EXIT HUP INT TERM

run_clean_fixture() {
  BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >/dev/null
}

run_rejected_fixture() {
  name=$1
  path=$2
  content=$3
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$(dirname "$ROOT/$path")"
  printf '%s\n' "$content" >"$ROOT/$path"
  if BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/$name.out" 2>&1; then
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
run_rejected_fixture new-legacy-site \
  src-tauri/src/backend/runtime/new_error.rs \
  'let _error = AppError::Legacy("fixture".to_string());'
run_rejected_fixture new-sync-bridge \
  src-tauri/src/backend/target_catalog.rs \
  'tokio::runtime::Handle::current().block_on(async {});'

printf '%s\n' 'module boundary self-tests passed'
