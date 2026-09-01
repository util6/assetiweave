# T14：归档旧数据并删除 Legacy Memory 表面

## Outcome

升级时可生成一次应用自有目录中的只读旧 Memory 归档；旧 Dream、旧 Recall、Library、candidate、Evidence 的 UI、Engine、CLI、Skill、后台触发和应用工作流退出，新 Memory 不读取旧自动生成数据。

## Blocked by

T12、T13。所有公共入口已切到新合同。

## Read

- Contracts：C-D02、C-D06、C-A02、C-A03、C-X01–C-X03。
- Seams：S01、S05、S10–S17；Tests TS05–TS07。
- Gates：G0–G7。

## Authority changed

旧数据从可执行产品状态降为只读归档输入；新 Memory authority 不变。

## Red test first

从含旧 Dream/candidate/Evidence 数据的临时数据库升级：归档文件在 app-owned 临时目录生成且可人工读取；新 recent/context/recall 结果与旧表内容无关；registry/router/CLI/Skill 不再暴露旧方法或页面。

## Execution steps

1. 列举 legacy schema、workflow、commands、routes、components、i18n、CLI、Skill 和自动触发。完成标准：清单中每项标注 archive/delete/retain-infrastructure。
2. 实现幂等只读归档。完成标准：不调用模型、不导入新表、不写第三方目录，重复运行结果稳定。
3. 删除旧后台 consumer/timer/workflow 与 public methods。完成标准：应用启动不再创建旧 Dream/Recall work。
4. 删除旧 frontend/CLI/Skill 表面和无引用代码。完成标准：新两页与新 contract tests 全绿。
5. 保留已发布 migration，新增必要 tombstone/metadata migration。完成标准：旧数据库可升级，新数据库可启动，未手改历史 migration。
6. 运行 legacy grep 并逐个分类命中。完成标准：仅 migration、归档 reader 和明确历史文档仍可命中。

## Acceptance

- [ ] 旧数据可一次性只读归档。
- [ ] 归档不参与新 Memory 查询或生成。
- [ ] 旧 UI/API/CLI/Skill/后台路径退出。
- [ ] 已发布 migration 未修改。
- [ ] 新 Memory 无 legacy table read dependency。
- [ ] 可复用 redaction/TaskRuntime/Agent/locator 基础设施继续通过新合同测试。
- [ ] 不保留长期 v1/v2 兼容路由。

## Non-goals

把旧数据导入新模型、修复旧 Dream/Recall、删除所有历史 migration、云备份。

## Ticket-specific stop

如果删除某旧符号会破坏非 Memory 领域的共享基础设施，保留并重命名到中性 seam，报告具体依赖；不复制一份新基础设施。
