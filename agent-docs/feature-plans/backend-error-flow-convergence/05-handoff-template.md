# Issue #2 错误链单卡交接

每卡完成后向 Issue #2 评论：

```markdown
## Ticket / Status
- Ticket: 当前卡 ID
- Status: VERIFIED | DRIFT | BLOCKED
- Start revision: 开始 SHA
- End revision: 提交 SHA
- Commit: 中文 Conventional Commit subject

## Error authority
- Typed error introduced or reused: 类型名
- Source chain preserved from: 原始错误类型列表
- Production consumers migrated: 全部入口
- String boundaries retained: 精确符号与局部原因；没有则写 none

## RED / GREEN / DELETE
- RED: 命令、失败断言、test count
- GREEN: 同一命令、通过断言、test count
- DELETE: 卡片查询与全部输出

## Verification
- Card gate: Gate ID / PASS
- G-RUST-BASE: PASS
- Wire contract diff: no 或生成 diff 摘要
- Warning delta: 修改文件 before/after 数字

## Scope
- Files changed: `git diff --name-only` 的完整输出
- Pre-existing dirty paths preserved: 开始前记录
- Public wire behavior: unchanged 或逐字段变化
- Next ready ticket: `03-ticket-map.md` 下一卡
```

说明文字必须替换为真实值；没有数据的字段写 `none`，不得删除字段。
