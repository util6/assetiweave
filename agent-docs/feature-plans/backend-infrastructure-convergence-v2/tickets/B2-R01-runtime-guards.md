# B2-R01：建立 AppRuntime 高层 seam 与 Runtime 守卫

**Objective:** 在迁移前锁定 ResidentHost/OneShot 外部行为，并阻止新的内部 runtime bridge。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-PRODUCT-01。

**Canonical authority after this card:** `AppRuntime` lifecycle tests 描述启动角色、共享 pool、任务可见性和关闭结果；边界脚本描述允许继续减少的历史命中。

**Files:**

- Modify: `src-tauri/src/backend/runtime/tests.rs`
- Modify: `scripts/check-module-boundaries.sh`
- Modify: `scripts/check-module-boundaries.test.sh`
- Create: `scripts/rust-runtime-bridge-baseline.txt`

## Steps

- [ ] 从 B2-00 baseline 生成精确 allowlist：每行 `count<TAB>relative-path`，只包含当时存在 `.block_on(`/`.run_sync(` 或 `tokio::runtime::Runtime` 的文件。
- [ ] 在现有边界脚本加入单调下降检查：新文件命中失败；已有文件命中数增长失败；删除命中允许通过。
- [ ] 在边界脚本测试中构造临时 fixture，分别证明“新增文件命中”和“已有文件计数增长”会 RED，“计数下降”会 GREEN。
- [ ] 扩展 runtime 高层测试：同一临时数据库分别 bootstrap OneShot 与 ResidentHost；断言两者通过 AppService 观察相同持久数据，OneShot 无 resident dispatcher，ResidentHost 有可停止的 resident services。
- [ ] 增加“shutdown 调用两次幂等”行为测试；第二次不重启资源、不产生新的任务或事件。
- [ ] 运行最窄测试并记录旧代码是否暴露角色/关闭缺口；本卡只补 seam 和守卫，不改 runtime ownership。

## Tests

```bash
bash scripts/check-module-boundaries.test.sh
cargo test -p assetiweave backend::runtime::tests -- --nocapture
pnpm check:boundaries
```

## Gate G-B2-R01

```bash
git diff --check
pnpm check:boundaries
cargo test -p assetiweave backend::runtime::tests -- --nocapture
```

通过：高层 lifecycle seam 绿；守卫允许命中下降并拒绝任何新增；生产行为未改变。

**Stop:** 测试必须访问私有线程字段才能判断角色时，改从公开 shutdown report/task/事件行为构造 seam；不新增测试专用 production getter。

**Commit:** `test(runtime): 锁定运行时生命周期与同步桥基线`

