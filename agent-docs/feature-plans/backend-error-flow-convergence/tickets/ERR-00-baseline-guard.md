# ERR-00：错误边界基线与防增长守卫

**Authority:** 当前代码扫描与逐命中分类决定迁移范围；守卫只防增长，不替代行为测试。

**Files:**

- Modify/Test: `scripts/check-module-boundaries.sh`
- Modify/Test: `scripts/check-module-boundaries.test.sh`
- Modify: `agent-docs/feature-plans/backend-error-flow-convergence/02-current-baseline.md`

## Steps

- [ ] 重跑 `02-current-baseline.md` 四条查询，把每个命中分类为 `cross-module migrate`、`private parser retain`、`test-only retain` 或 `outer transport retain`。
- [ ] 更新基线数字为 B2-G03 后的真实输出；保留当前审计数字作为历史，不覆盖。
- [ ] 在边界脚本增加单调守卫：新的跨模块 `Result<T,String>`、`map_err(AppError::external)` 和已知 typed error 的 `.to_string()` 映射失败。
- [ ] self-test 分别注入三种违规 fixture，证明每种模式 RED；删除 fixture 后 GREEN。
- [ ] 守卫不得只检查固定变量名，不得把整个文件加入 allowlist；保留必须绑定精确函数签名和类别。

## Verify

```bash
bash scripts/check-module-boundaries.test.sh
pnpm check:boundaries
git diff --check
```

提交：`test(errors): 建立错误字符串边界防增长守卫`
