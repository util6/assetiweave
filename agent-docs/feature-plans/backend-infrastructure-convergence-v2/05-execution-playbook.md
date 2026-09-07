# Luna / Flash 执行 Playbook

## 1. 开始工作卡

在改文件前输出并保存以下事实到本轮笔记：

```text
Ticket: copy the selected card ID, for example B2-R04
HEAD: paste the output of git rev-parse HEAD
Dirty paths before work: paste the output of git status --short
Contract IDs: copy every Contract ID listed by the selected card
Canonical authority after this card: copy the selected card's Authority sentence
Legacy mechanism removed/delegated: copy the selected card's delete target
Target tests: copy the selected card's test commands
Stop conditions checked: PASS
```

每个说明句必须替换为真实命令输出或卡片原文；不得把示例原样提交。

## 2. 定位而非猜测

1. 先运行卡片的 `rg` 命令得到生产 consumer 清单。
2. 从公开 adapter 或 AppService 方法向内追踪到 repository/OS sink。
3. 把每个命中标为 `migrate`、`delete`、`test-only` 或 `retain with contract reason`。
4. 未分类命中意味着定位未完成，不进入实现。

完成标准：卡片声明范围内每个旧 Authority 命中都有唯一处置。

## 3. RED → GREEN → DELETE

### RED

- 优先扩展卡片点名的现有高层测试文件。
- 测试断言公开结果、数据库副作用、task/dispatcher/process 生命周期或 wire contract。
- 运行最窄测试并记录真实失败输出。
- 若测试在旧代码已通过，说明它不能反驳旧 Authority；加强场景，而不是伪造 RED。
- 测试过滤器执行 0 tests 视为失败；先修正过滤器或写出精确测试名。

### GREEN

- 实现满足当前卡 Contract 的最小 production 变化。
- 沿现有模块边界传递 async、pool、deadline、typed slice 或 span。
- 不引入通用 wrapper 来保留旧 API 形状。

### DELETE

- 切换最后一个 production consumer 后立即删除旧实现。
- 运行卡片的 `rg` 删除断言；对 test-only 命中使用精确 `#[cfg(test)]` 归属。
- 删除行为由高层测试保护；source guard 只是防回潮。

完成标准：RED 证据、GREEN 证据和 DELETE 证据都可复制到 Issue 评论。

## 4. 控制改动尺寸

- 只改卡片 `Files` 中列出的路径。
- 新发现的必需文件先检查是否属于同一 Authority；属于则在交接列出，超过 12 个生产模块触发拆卡。
- 机械签名迁移与行为变化分成相邻提交时，两个提交都必须编译；卡片最终只以最后一个提交交接。
- 不格式化未触及文件，不清理相邻 warning，不更新无关生成物。

## 5. 失败处理

1. 首次失败：保存完整命令、exit code 与首个根因。
2. 缩小到单一测试或编译单元，使用 `rg`/调用链定位。
3. 修复后重新运行最窄失败命令。
4. 目标命令转绿后运行卡片 Gate；同一未改代码不重复运行同一命令。
5. 外部环境或前置卡导致的同一阻塞连续复现时，按 Router 的 DRIFT 模板交接并停止。

## 6. Review Gate

提交前逐项回答：

- 哪个 production consumer 现在使用新 Authority？
- 哪个旧机制已删除；`rg` 输出是什么？
- 哪个行为测试在旧实现上失败、现在通过？
- 是否改变 public contract、migration、portable path 或 task/outbox 语义？
- 是否触及用户原有未提交文件？
- 本卡能否独立 revert？

任一回答为空，不提交。

## 7. Commit 与交接

```bash
git diff --check
git status --short
git diff --stat
```

检查只包含本卡后，以中文 Conventional Commit 提交。随后按 `06-handoff-template.md` 评论 Issue #24，并停止。
