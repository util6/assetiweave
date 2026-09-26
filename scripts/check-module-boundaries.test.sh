#!/bin/sh
set -eu

ROOT=$(mktemp -d "${TMPDIR:-/tmp}/assetiweave-boundaries.XXXXXX")
trap 'rm -rf "$ROOT"' EXIT HUP INT TERM
export BOUNDARY_ALLOW_MISSING_ALLOWLIST=1

run_clean_fixture() {
  BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >/dev/null
}

run_backend_root_layout_fixture() {
  mode=$1
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$ROOT/src-tauri/src/backend/application" \
    "$ROOT/src-tauri/src/backend/domain" \
    "$ROOT/src-tauri/src/backend/infrastructure" \
    "$ROOT/src-tauri/src/backend/store"
  printf '%s\n' \
    'pub(crate) mod application;' \
    'pub(crate) mod domain;' \
    'pub(crate) mod infrastructure;' \
    'pub(crate) mod store;' >"$ROOT/src-tauri/src/backend/mod.rs"
  case "$mode" in
    legacy-module)
      mkdir -p "$ROOT/src-tauri/src/backend/legacy"
      printf '%s\n' 'pub(crate) mod legacy;' >>"$ROOT/src-tauri/src/backend/mod.rs"
      ;;
    stray-directory)
      mkdir -p "$ROOT/src-tauri/src/backend/legacy"
      ;;
    fifth-root)
      mkdir -p "$ROOT/src-tauri/src/backend/fifth_root"
      printf '%s\n' 'pub(crate) mod fifth_root;' >>"$ROOT/src-tauri/src/backend/mod.rs"
      ;;
    test-support-without-cfg)
      mkdir -p "$ROOT/src-tauri/src/backend/test_support"
      printf '%s\n' 'pub(crate) mod test_support;' >>"$ROOT/src-tauri/src/backend/mod.rs"
      ;;
    cross-layer-reexport)
      printf '%s\n' 'pub(crate) use application::AppService;' >>"$ROOT/src-tauri/src/backend/mod.rs"
      ;;
    cfg-test-support)
      mkdir -p "$ROOT/src-tauri/src/backend/test_support"
      printf '%s\n' '#[cfg(test)]' 'pub(crate) mod test_support;' >>"$ROOT/src-tauri/src/backend/mod.rs"
      ;;
    cfg-test-support-file)
      printf '%s\n' '#[cfg(test)]' 'pub(crate) mod test_support;' >>"$ROOT/src-tauri/src/backend/mod.rs"
      printf '%s\n' 'pub(crate) struct TestSupport;' >"$ROOT/src-tauri/src/backend/test_support.rs"
      ;;
  esac

  case "$mode" in
    clean|cfg-test-support|cfg-test-support-file)
      if ! node scripts/check-backend-root-layout.mjs "$ROOT" >"$ROOT/root-layout-$mode.out" 2>&1; then
        cat "$ROOT/root-layout-$mode.out"
        printf '%s\n' "self-test failed: valid backend root layout rejected: $mode"
        exit 1
      fi
      ;;
    *)
      if node scripts/check-backend-root-layout.mjs "$ROOT" >"$ROOT/root-layout-$mode.out" 2>&1; then
        cat "$ROOT/root-layout-$mode.out"
        printf '%s\n' "self-test failed: invalid backend root layout accepted: $mode"
        exit 1
      fi
      ;;
  esac
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

run_allowed_fixture() {
  name=$1
  path=$2
  content=$3
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$(dirname "$ROOT/$path")"
  printf '%s\n' "$content" >"$ROOT/$path"
  if ! BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/$name.out" 2>&1; then
    cat "$ROOT/$name.out"
    printf '%s\n' "self-test failed: $name fixture was unexpectedly rejected"
    exit 1
  fi
}

run_rejected_layer_dependency_fixture() {
  name=$1
  path=$2
  content=$3
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$(dirname "$ROOT/$path")"
  printf '%s\n' "$content" >"$ROOT/$path"
  if BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 \
    BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS=1 \
    BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/$name.out" 2>&1; then
    cat "$ROOT/$name.out"
    printf '%s\n' "self-test failed: reverse dependency fixture was accepted: $name"
    exit 1
  fi
}

run_rejected_layer_dependency_scanner_fixture() {
  name=$1
  path=$2
  content=$3
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$(dirname "$ROOT/$path")"
  printf '%s\n' "$content" >"$ROOT/$path"
  if BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS=1 \
    node scripts/check-backend-layer-dependencies.mjs "$ROOT" >"$ROOT/$name.out" 2>&1; then
    cat "$ROOT/$name.out"
    printf '%s\n' "self-test failed: reverse dependency scanner accepted fixture: $name"
    exit 1
  fi
}

run_accepted_test_only_layer_dependency_fixture() {
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$ROOT/src-tauri/src/backend/store"
  cat >"$ROOT/src-tauri/src/backend/store/settings_repo.rs" <<'EOF'
#[cfg(test)]
#[path = "settings_repo_tests.rs"]
mod tests;
EOF
  printf '%s\n' 'use crate::backend::infrastructure::app_settings::AppLocale;' \
    >"$ROOT/src-tauri/src/backend/store/settings_repo_tests.rs"
  mkdir -p "$ROOT/src-tauri/src/backend/infrastructure/runtime"
  cat >"$ROOT/src-tauri/src/backend/infrastructure/runtime/mod.rs" <<'EOF'
#[cfg(test)]
mod tests;
EOF
  printf '%s\n' 'use crate::backend::application::AppService;' \
    >"$ROOT/src-tauri/src/backend/infrastructure/runtime/tests.rs"
  cat >"$ROOT/src-tauri/src/backend/store/database.rs" <<'EOF'
#[cfg(test)]
async fn initialize_test_database() {
    crate::backend::infrastructure::bootstrap::init_test_database().await;
}
EOF
  if ! BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 \
    BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS=1 \
    BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/test-only-dependency.out" 2>&1; then
    cat "$ROOT/test-only-dependency.out"
    printf '%s\n' 'self-test failed: test-only reverse dependency was treated as production'
    exit 1
  fi
}

run_rejected_ungated_test_filename_fixture() {
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$ROOT/src-tauri/src/backend/infrastructure/runtime"
  cat >"$ROOT/src-tauri/src/backend/infrastructure/runtime/mod.rs" <<'EOF'
const TEST_DECLARATION_EXAMPLE: &str = r#"#[cfg(test)] #[path = "tests.rs"] mod tests;"#;
mod tests;
EOF
  printf '%s\n' 'use crate::backend::application::AppService;' \
    >"$ROOT/src-tauri/src/backend/infrastructure/runtime/tests.rs"
  if BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 \
    BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS=1 \
    BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/ungated-test-filename.out" 2>&1; then
    cat "$ROOT/ungated-test-filename.out"
    printf '%s\n' 'self-test failed: unconditionally compiled tests.rs bypassed production checks'
    exit 1
  fi
}

run_rejected_legacy_report_fixture() {
  name=$1
  content=$2
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$ROOT/src-tauri/src/backend/application"
  printf '%s\n' "$content" >"$ROOT/src-tauri/src/backend/application/new_module.rs"
  if BOUNDARY_RETIRED_MODULES=planner node scripts/report-backend-legacy-modules.mjs "$ROOT" >"$ROOT/$name.out" 2>&1; then
    cat "$ROOT/$name.out"
    printf '%s\n' "self-test failed: legacy reporter accepted fixture: $name"
    exit 1
  fi
}

run_accepted_inline_test_only_legacy_reference_fixture() {
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$ROOT/src-tauri/src/backend/application"
  cat >"$ROOT/src-tauri/src/backend/application/new_module.rs" <<'EOF'
#[cfg(test)]
mod tests {
    use crate::backend::planner::build_plan;
}
EOF
  if ! BOUNDARY_RETIRED_MODULES=planner \
    node scripts/report-backend-legacy-modules.mjs "$ROOT" >"$ROOT/inline-test-only-reference.out" 2>&1; then
    cat "$ROOT/inline-test-only-reference.out"
    printf '%s\n' 'self-test failed: inline cfg(test) retired reference was counted as production'
    exit 1
  fi
}

run_clean_fixture
run_backend_root_layout_fixture clean
if ! BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 \
  BOUNDARY_REQUIRE_FOUR_BACKEND_ROOTS=1 \
  BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/root-layout-final-gate.out" 2>&1; then
  cat "$ROOT/root-layout-final-gate.out"
  printf '%s\n' 'self-test failed: final four-root gate rejected a valid layout'
  exit 1
fi
printf '%s\n' 'use crate::backend::planner::build_plan;' \
  >"$ROOT/src-tauri/src/backend/application/legacy_reference.rs"
if BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 \
  BOUNDARY_REQUIRE_FOUR_BACKEND_ROOTS=1 \
  BOUNDARY_SKIP_LEGACY_REPORT=1 \
  BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/root-layout-retired-reference.out" 2>&1; then
  cat "$ROOT/root-layout-retired-reference.out"
  printf '%s\n' 'self-test failed: final four-root gate accepted a retired root reference'
  exit 1
fi
run_backend_root_layout_fixture cfg-test-support
run_backend_root_layout_fixture cfg-test-support-file
run_backend_root_layout_fixture legacy-module
run_backend_root_layout_fixture stray-directory
run_backend_root_layout_fixture fifth-root
run_backend_root_layout_fixture test-support-without-cfg
run_backend_root_layout_fixture cross-layer-reexport
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src/backend"
printf '%s\n' 'pub(crate) use error::AppError;' >"$ROOT/src-tauri/src/backend/mod.rs"
if BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/backend-root-reexport.out" 2>&1; then
  cat "$ROOT/backend-root-reexport.out"
  printf '%s\n' 'self-test failed: backend root cross-layer re-export was accepted'
  exit 1
fi

run_rejected_fixture runtime-dependency \
  src-tauri/src/backend/infrastructure/runtime/mod.rs \
  'use crate::backend::application::AppService;'
run_rejected_fixture runtime-default-data-orchestration \
  src-tauri/src/backend/infrastructure/runtime/app_runtime.rs \
  'store::open_migrated_pool(&db_path).await?; TargetCatalog::load_with_overrides(&target_catalog_dir)?; seed_defaults_with_catalog(&pool, &catalog).await?; agent_runtime_manager.recover_startup(&runtime_root).await?;'
run_rejected_fixture infrastructure-grouped-application-import \
  src-tauri/src/backend/infrastructure/runtime/new_module.rs \
  'use crate::backend::{ application::AppService };'
run_rejected_layer_dependency_fixture infrastructure-aliased-application-import \
  src-tauri/src/backend/infrastructure/runtime/new_module.rs \
  'use crate::backend::application as application_layer;'
run_rejected_layer_dependency_scanner_fixture infrastructure-backend-root-alias \
  src-tauri/src/backend/infrastructure/runtime/new_module.rs \
  'use crate::backend as backend_root; use backend_root::application::AppService;'
run_rejected_layer_dependency_scanner_fixture infrastructure-grouped-backend-root-alias \
  src-tauri/src/backend/infrastructure/runtime/new_module.rs \
  'use crate::{ backend as backend_root }; use backend_root::{ application::AppService };'
run_rejected_layer_dependency_scanner_fixture infrastructure-commented-cfg-marker \
  src-tauri/src/backend/infrastructure/runtime/new_module.rs \
  '// #[cfg(test)]
use crate::backend::application::AppService;'
run_rejected_layer_dependency_fixture store-infrastructure-import \
  src-tauri/src/backend/store/system/new_repo.rs \
  'use crate::backend::infrastructure::app_settings::AppLocale;'
run_rejected_layer_dependency_fixture store-grouped-infrastructure-reexport \
  src-tauri/src/backend/store/system/new_repo.rs \
  'pub use crate::backend::{ infrastructure::app_settings::AppLocale };'
run_accepted_test_only_layer_dependency_fixture
run_rejected_ungated_test_filename_fixture
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
  src-tauri/src/backend/infrastructure/runtime/new_error.rs \
  'let _error = AppError::Legacy("fixture".to_string());'
run_rejected_fixture new-legacy-result-site \
  src-tauri/src/backend/infrastructure/runtime/new_result.rs \
  'fn fixture() -> LegacyResult<()> { Ok(()) }'
run_rejected_fixture new-sync-bridge \
  src-tauri/src/backend/application/mounting/target_catalog.rs \
  'tokio::runtime::Handle::current().block_on(async {});'
run_rejected_fixture application-target-catalog-bypass \
  src-tauri/src/backend/application/service.rs \
  'let _catalog = TargetCatalog::builtin();'
run_rejected_fixture legacy-planner-import \
  src-tauri/src/backend/application/catalog/assets.rs \
  'crate::backend::planner::build_plan_with_catalog();'
run_rejected_legacy_report_fixture legacy-planner-root-alias \
  'use crate::backend as backend_root; use backend_root::planner::build_plan;'
run_accepted_inline_test_only_legacy_reference_fixture
run_rejected_fixture legacy-executor-import \
  src-tauri/src/backend/application/mounting/mounts.rs \
  'crate::backend::executor::execute_deployment_plan();'
run_rejected_fixture legacy-agent-process-import \
  src-tauri/src/backend/ai_execution/backends/acp.rs \
  'use crate::backend::{ agents::{ process::{ManagedAgentProcess} } };'
run_rejected_fixture legacy-agent-process-direct-import \
  src-tauri/src/backend/ai_execution/backends/native.rs \
  'use crate::backend::agents::process::ManagedAgentProcess;'
run_rejected_fixture infrastructure-agent-definition-legacy-import \
  src-tauri/src/backend/infrastructure/agent_execution/managed_process_support.rs \
  'use crate::backend::agents::types::AgentDefinition;'
run_rejected_fixture legacy-agent-acp-protocol-import \
  src-tauri/src/backend/ai_execution/backends/acp.rs \
  'use crate::backend::{ agents::{ protocol::acp::AcpProtocol } };'
run_rejected_fixture legacy-agent-acp-event-import \
  src-tauri/src/backend/ai_execution/backends/acp_aggregator.rs \
  'use crate::backend::agents::protocol::acp::AcpRuntimeEvent;'
run_rejected_fixture legacy-agent-registry-direct-import \
  src-tauri/src/backend/ai_execution/executor.rs \
  'use crate::backend::agents::registry::AgentRegistry;'
run_rejected_fixture legacy-agent-registry-grouped-import \
  src-tauri/src/backend/agent_market/runtime.rs \
  'use crate::backend::{ agents::{ registry::{AgentRegistry} } };'
run_rejected_fixture tauri-log-infrastructure-bypass \
  src-tauri/src/adapters/tauri/commands.rs \
  'crate::backend::infrastructure::logs::logs_get_snapshot(None, None);'
run_rejected_fixture tauri-log-module-alias-bypass \
  src-tauri/src/adapters/tauri/commands.rs \
  'use crate::backend::infrastructure::logs as log_ops;'
run_rejected_fixture tauri-log-module-import-bypass \
  src-tauri/src/adapters/tauri/commands.rs \
  'use crate::backend::infrastructure::logs;'
run_rejected_fixture tauri-log-nested-grouped-import-bypass \
  src-tauri/src/adapters/tauri/commands.rs \
  'use crate::backend::infrastructure::{
    logs::{logs_get_snapshot as get_snapshot}
};'
run_rejected_fixture tauri-log-nested-grouped-reexport-bypass \
  src-tauri/src/adapters/tauri/commands.rs \
  'pub use crate::backend::{
    infrastructure::logs::{logs_write_operation as write_operation}
};'
run_rejected_fixture tauri-agent-execution-runtime-bypass \
  src-tauri/src/adapters/tauri/commands.rs \
  'runtime.execute(request);'
run_rejected_fixture tauri-agent-runtime-state-bypass \
  src-tauri/src/adapters/tauri/commands.rs \
  'state.agent_runtime.execute(request);'
run_rejected_fixture tauri-agent-runtime-state-field \
  src-tauri/src/adapters/app_state.rs \
  'agent_runtime: Arc<dyn AgentExecutionRuntime>,'
run_rejected_fixture host-process-libc-kill \
  src-tauri/src/backend/infrastructure/host_process.rs \
  'unsafe { libc::kill(1, 9); }'
run_rejected_fixture host-process-process-group-zero \
  src-tauri/src/backend/infrastructure/host_process.rs \
  'command.process_group(0);'
run_rejected_fixture host-process-thread-spawn \
  src-tauri/src/backend/infrastructure/host_process.rs \
  'std::thread::spawn(|| {});'
run_rejected_fixture host-process-thread-sleep \
  src-tauri/src/backend/infrastructure/host_process.rs \
  'thread::sleep(Duration::from_millis(15));'
run_rejected_fixture host-process-std-command \
  src-tauri/src/backend/infrastructure/host_process.rs \
  'let _ = std::process::Command::new("sh");'
run_rejected_fixture error-boundary-cross-module-string \
  src-tauri/src/backend/infrastructure/runtime/new_service.rs \
  'pub fn fixture_op() -> Result<(), String> { Ok(()) }'
run_rejected_fixture error-boundary-map-err-external \
  src-tauri/src/backend/infrastructure/runtime/app_runtime.rs \
  'let _ = op().map_err(AppError::external);'
run_rejected_fixture error-boundary-known-typed-to-string \
  src-tauri/src/backend/infrastructure/runtime/new_service.rs \
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

# The retirement reporter compares against an explicitly accepted worktree
# snapshot, not HEAD, so changes already present when this migration started
# remain intact while new growth is rejected.
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src/backend/agents" "$ROOT/src-tauri/src/backend/application"
printf '%s\n' 'pub(crate) struct LegacyAgent;' > "$ROOT/src-tauri/src/backend/agents/mod.rs"
node scripts/report-backend-legacy-modules.mjs "$ROOT" --capture "$ROOT/legacy-baseline.json" >/dev/null
printf '%s\n' 'use crate::backend::agents::LegacyAgent;' > "$ROOT/src-tauri/src/backend/application/agents.rs"
if node scripts/report-backend-legacy-modules.mjs "$ROOT" --baseline "$ROOT/legacy-baseline.json" >"$ROOT/legacy-growth.out" 2>&1; then
  cat "$ROOT/legacy-growth.out"
  printf '%s\n' 'self-test failed: legacy module growth was accepted'
  exit 1
fi
rm -rf "$ROOT/src-tauri/src/backend/agents" "$ROOT/src-tauri/src/backend/application"
if ! BOUNDARY_REQUIRE_NO_LEGACY=1 node scripts/report-backend-legacy-modules.mjs "$ROOT" --baseline "$ROOT/legacy-baseline.json" >"$ROOT/legacy-zero.out" 2>&1; then
  cat "$ROOT/legacy-zero.out"
  printf '%s\n' 'self-test failed: retired module topology was rejected'
  exit 1
fi

# Test-support modules are compiled only under cfg(test), so their imports
# must not inflate production legacy caller/reference counts.
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src/backend/agents" "$ROOT/src-tauri/src/backend/store"
printf '%s\n' 'pub(crate) struct LegacyAgent;' >"$ROOT/src-tauri/src/backend/agents/mod.rs"
cat >"$ROOT/src-tauri/src/backend/store/mod.rs" <<'EOF'
#[cfg(test)]
pub(crate) mod test_support;
EOF
printf '%s\n' 'use crate::backend::agents::LegacyAgent;' \
  >"$ROOT/src-tauri/src/backend/store/test_support.rs"
node scripts/report-backend-legacy-modules.mjs "$ROOT" --capture "$ROOT/test-support-baseline.json" >/dev/null
node --input-type=module -e '
  import fs from "node:fs";
  const baseline = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  const agents = baseline.modules.agents;
  if (agents.callerFiles !== 0 || agents.references !== 0) {
    throw new Error(`cfg(test) helper counted as production: ${JSON.stringify(agents)}`);
  }
' "$ROOT/test-support-baseline.json"

# Retired root names are rejected for direct/aliased paths, grouped imports,
# and re-exports even when the baseline already contains the fixture path.
check_retired_module_path_fixture() {
  name=$1
  contents=$2
  rm -rf "$ROOT/src-tauri"
  mkdir -p "$ROOT/src-tauri/src/backend/application"
  printf '%s\n' "$contents" > "$ROOT/src-tauri/src/backend/application/fixture.rs"
  node scripts/report-backend-legacy-modules.mjs "$ROOT" --capture "$ROOT/legacy-path-baseline.json" >/dev/null
  if BOUNDARY_RETIRED_MODULES=executor node scripts/report-backend-legacy-modules.mjs "$ROOT" --baseline "$ROOT/legacy-path-baseline.json" >"$ROOT/$name.out" 2>&1; then
    cat "$ROOT/$name.out"
    printf '%s\n' "self-test failed: retired module path form was accepted: $name"
    exit 1
  fi
}

check_retired_module_path_fixture grouped-import \
  'use crate::backend::{ executor::DeploymentError };'
check_retired_module_path_fixture direct-import \
  'use crate::backend::executor::DeploymentError;'
check_retired_module_path_fixture aliased-import \
  'use crate::backend::executor as legacy_executor;'
check_retired_module_path_fixture grouped-reexport \
  'pub use crate::backend::{ executor::DeploymentError };'

# Cross-layer glob imports are rejected by production boundary checks
run_rejected_fixture cross-layer-glob-domain-import \
  src-tauri/src/backend/application/new_feature.rs \
  'use crate::backend::domain::model::*;'
run_rejected_fixture cross-layer-glob-infrastructure-import \
  src-tauri/src/backend/application/new_feature.rs \
  'use crate::backend::infrastructure::service::*;'
run_rejected_fixture cross-layer-glob-store-import \
  src-tauri/src/backend/application/new_feature.rs \
  'use crate::backend::store::repo::*;'
run_rejected_fixture cross-layer-glob-grouped-import \
  src-tauri/src/backend/application/new_feature.rs \
  'use crate::backend::{domain::model::*};'
run_rejected_fixture cross-layer-glob-grouped-self-star \
  src-tauri/src/backend/application/new_feature.rs \
  'use crate::backend::domain::{self, *};'
run_rejected_fixture cross-layer-glob-grouped-other-star \
  src-tauri/src/backend/application/new_feature.rs \
  'use crate::backend::domain::{foo, *};'
run_rejected_fixture cross-layer-glob-multiline \
  src-tauri/src/backend/application/new_feature.rs \
  "$(printf '%s\n' 'use crate::backend::domain::{' '    self, *' '};')"

# Infrastructure bridge re-exports or modules for Domain/Store are rejected
run_rejected_fixture infrastructure-conversations-bridge-cards \
  src-tauri/src/backend/infrastructure/conversations/mod.rs \
  'pub mod cards;'
run_rejected_fixture infrastructure-conversations-bridge-pricing \
  src-tauri/src/backend/infrastructure/conversations/mod.rs \
  'mod pricing;'
run_rejected_fixture infrastructure-conversations-bridge-usage-repo \
  src-tauri/src/backend/infrastructure/conversations/mod.rs \
  'pub use crate::backend::store::conversations::usage_repo;'
run_rejected_fixture infrastructure-bridge-pub-crate-use-domain \
  src-tauri/src/backend/infrastructure/service.rs \
  'pub(crate) use crate::backend::domain::model::Item;'
run_rejected_fixture infrastructure-bridge-pub-use-store \
  src-tauri/src/backend/infrastructure/service.rs \
  'pub use crate::backend::store::repo::ItemRepo;'
run_rejected_fixture infrastructure-bridge-grouped \
  src-tauri/src/backend/infrastructure/service.rs \
  'pub(crate) use crate::backend::{domain::Item};'

# Domain dynamic env reads are rejected by domain purity check
run_rejected_fixture domain-env-var-read \
  src-tauri/src/backend/domain/conversations/fingerprint.rs \
  'let _ = std::env::var("HOME");'
run_rejected_fixture domain-env-vars-read \
  src-tauri/src/backend/domain/conversations/fingerprint.rs \
  'let _ = std::env::vars();'
run_rejected_fixture domain-aliased-env-var-read \
  src-tauri/src/backend/domain/model.rs \
  'use std::env as sys_env;
pub fn get_path() { let _ = sys_env::var("HOME"); }'
run_rejected_fixture domain-grouped-aliased-env-vars-read \
  src-tauri/src/backend/domain/model.rs \
  'use std::{collections::HashMap, env as process_env};
pub fn get_vars() { let _ = process_env::vars(); }'
run_rejected_fixture domain-direct-env-import-read \
  src-tauri/src/backend/domain/model.rs \
  'use std::env;
pub fn get_home() { let _ = env::var("HOME"); }'

# Allowed boundary fixtures: pure comments, strings, constants, private use, intra-layer glob, and test-only code
run_allowed_fixture allowed-domain-comments \
  src-tauri/src/backend/domain/clean.rs \
  '// std::env::var("HOME") is forbidden in domain
/* use std::env as sys_env; sys_env::var("HOME"); */
pub struct CleanDomain;'
run_allowed_fixture allowed-domain-string-literals \
  src-tauri/src/backend/domain/clean.rs \
  'pub fn text() -> String { "std::env::var and backend::store are not code here".to_string() }'
run_allowed_fixture allowed-domain-env-consts \
  src-tauri/src/backend/domain/platform.rs \
  'pub fn os_name() -> String { std::env::consts::OS.to_string() }'
run_allowed_fixture allowed-infrastructure-comments \
  src-tauri/src/backend/infrastructure/service.rs \
  '// pub(crate) use crate::backend::domain::Item;'
run_allowed_fixture allowed-infrastructure-private-use \
  src-tauri/src/backend/infrastructure/service.rs \
  'use crate::backend::domain::model::Item; pub(crate) fn do_something(_item: Item) {}'
run_allowed_fixture allowed-application-prelude-glob \
  src-tauri/src/backend/application/service.rs \
  'use crate::backend::application::prelude::*;'
run_allowed_fixture allowed-domain-explicit-import \
  src-tauri/src/backend/application/service.rs \
  'use crate::backend::domain::{Entity, ValueObject};'
run_allowed_fixture allowed-domain-test-cfg \
  src-tauri/src/backend/domain/model.rs \
  'pub struct Model;
#[cfg(test)]
mod tests {
  use crate::backend::domain::*;
  #[test]
  fn test_read() { let _ = std::env::var("TEST"); }
}'

# Adding a fifth root directory fails boundary check under default configuration
rm -rf "$ROOT/src-tauri"
mkdir -p "$ROOT/src-tauri/src/backend/application" \
  "$ROOT/src-tauri/src/backend/domain" \
  "$ROOT/src-tauri/src/backend/infrastructure" \
  "$ROOT/src-tauri/src/backend/store" \
  "$ROOT/src-tauri/src/backend/fifth_root"
printf '%s\n' \
  'pub(crate) mod application;' \
  'pub(crate) mod domain;' \
  'pub(crate) mod infrastructure;' \
  'pub(crate) mod store;' \
  'pub(crate) mod fifth_root;' >"$ROOT/src-tauri/src/backend/mod.rs"
if BOUNDARY_ALLOW_MISSING_ALLOWLIST=1 BOUNDARY_ROOT="$ROOT" sh scripts/check-module-boundaries.sh >"$ROOT/fifth-root-default.out" 2>&1; then
  cat "$ROOT/fifth-root-default.out"
  printf '%s\n' 'self-test failed: fifth backend root was accepted under default configuration'
  exit 1
fi

node scripts/check-backend-line-limits.test.mjs

printf '%s\n' 'module boundary self-tests passed'
