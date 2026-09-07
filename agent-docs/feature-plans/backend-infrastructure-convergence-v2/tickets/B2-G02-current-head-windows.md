# B2-G02：当前 HEAD 三平台与 Windows Process 验收

**Authority:** GitHub Actions 中与待验收 commit SHA 完全一致的 run 是跨平台证据 Authority。

**Contract:** C-PROCESS-01、C-RUNTIME-01、C-SHUTDOWN-01。

**Files:**

- Modify only when a real platform failure requires it: `.github/workflows/ci.yml`
- No production refactor;失败返回对应 owner 卡。

## Steps

- [ ] 记录 `git rev-parse HEAD`，推送含 B2-P03C、L02、F02、D02 的提交后触发 CI。
- [ ] Windows job 必须执行 `cargo test --workspace`，不能只编译；确认运行 `contract_windows_job_object_reaps_descendant_held_pipe`。
- [ ] macOS/Linux 必须运行 HostProcess descendant-held-pipe、timeout、cancel 和 shutdown deadline 测试。
- [ ] 使用 `gh run view` 核对 run 的 `headSha` 与本卡 HEAD 完全一致。
- [ ] 任一平台失败时评论 `DRIFT`，附 job URL、step、首个根因并返回 owner 卡；不得修改测试为 ignore、放宽 timeout 或只重跑到偶然通过。
- [ ] 三平台成功后把 run URL、job 名、测试数和 SHA 写入 Issue #24。

## Verify

```bash
head_sha="$(git rev-parse HEAD)"
run_id="$(gh run list --commit "$head_sha" --json databaseId,headSha | jq -r --arg sha "$head_sha" 'map(select(.headSha == $sha))[0].databaseId // empty')"
gh run view "$run_id" --json headSha,conclusion,jobs,url
```

`run_id` 为空时本卡状态是 `BLOCKED`；提交到 Issue 的内容必须使用命令返回的真实数字和 URL。

本卡通常不产生代码提交；CI 证据评论本身完成该卡。
