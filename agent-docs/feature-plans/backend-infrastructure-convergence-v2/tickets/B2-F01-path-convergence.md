# B2-F01：应用目录与 UTF-8 路径边界收口

**Objective:** 用 directories/camino 减少通用路径样板，同时保持 portable anchors 和非 UTF-8 OS 路径。

**Contracts:** C-PATH-01、C-PRODUCT-01、C-ERROR-01。

**Canonical authority after this card:** `HostPathResolver` 继续拥有 portable contract；directories 拥有应用目录；camino 只拥有 UTF-8 persistence/IPC 表示。

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src-tauri/src/backend/runtime/config.rs`
- Modify: `src-tauri/src/backend/host_paths.rs`
- Modify: `src-tauri/src/backend/host_filesystem.rs`
- Modify: `src-tauri/src/backend/path_utils.rs`
- Modify: affected target/path tests.

## Dependency change

```toml
directories = "=6.0.0"
camino = { version = "=1.2.5", features = ["serde1"] }
```

## Steps

- [ ] Extend current platform-injected tests for config/data/local-data/cache/home/workspace, anchors, Windows case/separators/reserved names, traversal, longest prefix and display round-trip.
- [ ] Add Unix-only fixture containing non-UTF-8 `OsString`; prove file operations and comparisons work without lossy conversion.
- [ ] Replace application directory assembly with `directories::ProjectDirs` while retaining explicit env overrides and current AssetIWeave directory names.
- [ ] Introduce camino only at JSON/database/IPC strings already required to be UTF-8; return Validation/Storage error at failed conversion.
- [ ] Keep `Path/PathBuf` through filesystem, canonicalization, symlink, containment and OS comparisons.
- [ ] Remove lossy conversion from security/identity decisions; lossy text remains allowed only for UI display or diagnostic logging after redaction.
- [ ] Evaluate path-clean/dunce against existing fixture. Add neither unless it deletes a named implementation and all platform tests stay green.

## Tests

```bash
cargo test -p assetiweave backend::runtime::config -- --nocapture
cargo test -p assetiweave backend::host_paths -- --nocapture
cargo test -p assetiweave backend::host_filesystem -- --nocapture
cargo test -p assetiweave backend::path_utils -- --nocapture
cargo check -p assetiweave --all-targets
```

## Delete proof

```bash
rg -n 'dirs::(config_dir|data_dir|data_local_dir|cache_dir)' src-tauri/src/backend --glob '*.rs'
rg -n 'to_string_lossy' src-tauri/src/backend/host_paths.rs src-tauri/src/backend/host_filesystem.rs src-tauri/src/backend/path_utils.rs
```

通过：第一条零命中；第二条仅含明确 display/diagnostic 位置；portable path tests 无格式变化。

**Stop:** 不批量迁移数据库路径字符串，不改变 anchor spelling；需要数据迁移即 DRIFT。

**Commit:** `refactor(paths): 收口应用目录与路径类型边界`

