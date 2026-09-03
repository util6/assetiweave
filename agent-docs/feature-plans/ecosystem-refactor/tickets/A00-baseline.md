# A00：冻结可验证基线与依赖安装前提

> **Status: PLANNED**。使用 `superpowers:executing-plans`。本卡不安装生产依赖、不重写业务。

**Goal:** 为后续依赖卡给出可信的工作区、工具链和行为基线。
**Depends:** 无。
**Contracts:** C-BASE。
**Gates:** G-BASE。
**Read:** `../00-execution-router.md`、`../02-dependencies.md`、`../05-playbook.md`。

## 文件与接口

- Read: 根 `AGENTS.md`、`package.json`、`pnpm-lock.yaml`、`Cargo.lock`、`src-tauri/Cargo.toml`、`.github/workflows/ci.yml`、当前卡涉及的工作区 diff。
- Modify/Create: 无生产文件。输出为 Issue #22 的基线交接评论。
- Produces: 起始 revision + 工作区成果归属 + 工具链 + 相关测试基线 + 可执行首卡；不是“全仓已完成验收”。

## 步骤

- [ ] **记录而不清理工作区**：运行以下命令，列出已有修改。Memory/Team/adapter 未提交内容保留；不 checkout/reset 或自动 stash。

```sh
git rev-parse HEAD
git status --short
git diff --stat
node --version
pnpm --version
rustc --version
cargo --version
go version
```

- [ ] **核对工具链**：Node 22 >=22.13、pnpm 10、Rust >=1.96.0、Go 1.24；核查 lock 中 React/TS 与 `02-dependencies.md` peer 范围。若另建 worktree，先确认基线包含用户未提交成果，单独 HEAD 不视为等价输入。
- [ ] **记录关键基线**：逐条运行下列命令；失败保留完整名称与错误摘要，不用修改测试或大幅更新包取得“干净基线”。

```sh
pnpm typecheck
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/router frontend/src/hooks/catalog/useCatalogData.test.ts frontend/src/app/backgroundTasks/BackgroundTaskRuntime.test.tsx frontend/src/store/settings
cargo fmt --all -- --check
cargo check --workspace
pnpm check:boundaries
```

- [ ] **API 安装闸门**：把每个新包的首次生产安装 owner 登记为依赖表中对应卡。首次安装必须运行该卡的最小 API 用例、类型检查和相关业务测试；本卡不以 registry 返回版本号代替兼容验证。
- [ ] **计数快照**：仅作为简化对比，记录下列搜索结果和 AppProviders 层数。保留命中的领域机制，不能靠清空搜索目标降数。

```sh
rg -n 'useState|useEffect|setInterval|createContext' frontend/src/hooks/catalog/useCatalogData.ts frontend/src/app/backgroundTasks/BackgroundTaskRuntime.tsx frontend/src/store/settings/AppSettingsProvider.tsx
rg -n 'ureq::|OpenOptions|Condvar' src-tauri/src/backend
rg -n 'createCachedLoader|resolveAppRoute|interpolate' frontend/src/router frontend/src/i18n
```

- [ ] **交接**：使用 `../06-handoff-template.md`；若关键基线可用，下一张按 map 为 A-F01，也可单独安排不冲突的 A-R01。若本卡被环境/基线阻断，记录恢复条件，不开始修改依赖。

## 完成条件

维护者与下一位执行者可从交接重建实际基线，并知道现有失败和用户文件归属。没有任何“未跑却通过”的 gate。无需提交新的基线技术长文，以免形成另一份源码快照。
