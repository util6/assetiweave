# LUNA-11：Agent Catalog 发布证据重新闭环

> **For Luna / Flash:** 一轮只执行本卡。状态 Authority 是 GitHub Issue #1。

**Objective:** 让 bundled Catalog、release evidence、真实制品身份和 release tests 在同一提交重新一致。

**Files:**

- Modify: `builtin-assets/agent-market/release-evidence-v1.json`
- Modify only when检查证明 Catalog 本身错误: `builtin-assets/agent-market/catalog-v1.json`
- Modify only when gate 不能表达既定契约: `scripts/check-agent-catalog-release.mjs`
- Test: `scripts/check-agent-catalog-release.test.mjs`

## 当前失败

```text
catalog SHA256: 2953ae1a57e5a9b3d2c3586ce97fb15647291ae35b39d0580bae5c2032e65ef6
evidence SHA256 field: 69628e68a5a52900e9216934cc475444ee8944d936f220e740866021137a23a3
```

## Steps

- [ ] 运行 `git diff bc5c14e..HEAD -- builtin-assets/agent-market/catalog-v1.json`，逐项分类为观察元数据、能力声明、distribution identity 或生命周期门禁变化。
- [ ] 运行 release 与 unit tests，保存 `catalogContentSha256` 的 RED。
- [ ] distribution URL/SHA/size/package/bin 变化时，重新运行 network 与真实 ACP E2E；能力声明变化时运行对应 capability 行为测试；只有纯观察元数据变化可以复用相同 artifact evidence。
- [ ] 使用 `shasum -a 256 builtin-assets/agent-market/catalog-v1.json` 取得当前文件哈希，更新 evidence 的 `catalogContentSha256` 与 `capturedAt`；不得修改 evidence 来掩盖 distribution 或 conformance 不一致。
- [ ] 运行四条 Gate；全部通过后在 Issue #1 写入 Catalog revision、hash、真实 E2E identity、命令和测试数。
- [ ] 更新 `00-overview.md` 状态时同时检查 #1 的其他未完成 requirement；本卡只证明发布链，不单独关闭 #1。

## Gate

```bash
node --test scripts/check-agent-catalog-release.test.mjs
node scripts/check-agent-catalog-release.mjs --static
node scripts/check-agent-catalog-release.mjs --release
cargo test -p assetiweave agent_market_lifecycle_e2e_install_update_failure_recovery_and_cancel -- --nocapture
```

若 Catalog diff 涉及 distribution identity，再运行：

```bash
node scripts/check-agent-catalog-release.mjs --release --network --e2e
```

提交：`fix(agent): 更新目录发布证据并恢复发布门禁`
