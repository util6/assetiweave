# T08：建立 Hybrid retrieval 与 Recall 只读工具

## Outcome

AppService 提供 tenant-scoped 只读 Recall 工具：解析项目/时间/Agent/文件/命令/错误线索，执行 filter + lexical + semantic + rerank，读取候选现场并返回精确 locator。

## Blocked by

T06。需要索引 freshness 与删除传播。

## Read

- Contracts：C-D01、C-D05、C-A01、C-A02、C-A05、C-R03、C-R04、C-R06、C-S02、C-S03。
- Seams：S01、S02、S08、S09；Tests TS04。
- Gates：G0、G1、G2、G6、G7。

## Authority changed

新增可重建 semantic index 与 Recall read-tool contract；Conversation/Memory 仍是事实来源。

## Red test first

fixture 同时包含精确关键词、同义表达、错误项目、邻近时间和重复 locator。公开 search workflow 返回正确项目内的关键词与语义候选，合并后去重并按相关度重排；删除来源后候选立即不可见；跨 tenant 查询不可见。

## Execution steps

1. 定义查询线索与范围 filter DTO。完成标准：项目、时间、source agent、文件、命令和错误可组合，缺省范围稳定。
2. 扩展派生 semantic index lifecycle。完成标准：revision/delete/invalidation 可重建，不形成事实 Authority。
3. 实现 lexical 与 semantic 独立候选源及确定性融合/rerank。完成标准：重复 locator 去重，稳定 tie-break 可测试。
4. 实现 candidate read/locator resolve 只读工具。完成标准：工具只能读取允许的 Conversation/Memory 字段，tenant/session scope 校验成立。
5. 添加成本/数量边界与 redaction。完成标准：工具输出有界，秘密字段在模型可见前脱敏。

## Acceptance

- [ ] 精确词与同义表达都可召回。
- [ ] 项目/时间/Agent/文件/命令/错误过滤可组合。
- [ ] 重排和去重结果确定性可测试。
- [ ] 删除/失效来源不出现在候选。
- [ ] 工具 tenant-scoped、只读且输出有界。
- [ ] 返回 locator 能由现有导航链解析。

## Non-goals

运行 ACP Agent、Recall Session 持久化、聊天 UI、通用向量数据库产品。

## Ticket-specific stop

如果 semantic provider 只能依赖真实网络才能测试，先抽象本地 deterministic embedding seam；不把 fixture 分数硬编码进生产规则。
