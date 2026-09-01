# T11：实现 Memory 使用反馈与治理设置核心

## Outcome

Context Resolver 或 Recall 真正采用某条 Memory 时记录 usage；“生成 Memory”“使用 Memory”、Session/来源排除和四类 action assignment 作为持久设置影响后台生成与读路径，并保持 tenant/redaction 边界。

## Blocked by

T05、T10。需要 Context 与 Recall 两条真实使用路径。

## Read

- Contracts：C-A01、C-A02、C-A05、C-M06、C-R01、C-S01–C-S03。
- Seams：S01、S05、S06、S08、S16；Tests TS01、TS03。
- Gates：G0、G1、G2、G7。

## Authority changed

新增 Memory usage 与持久治理设置的 AppService 语义；settings 系统仍是配置 Authority。

## Red test first

仅搜索到候选但未进入最终 Context/Recall reference 时 usage 不变；最终采用后 count +1 且 last used 更新。关闭生成后 Conversation commit 不入新 Job；关闭使用后 Context/Recall 不返回 Memory；排除 Session 后其 Conversation 仍存在但 Memory 被失效。

## Execution steps

1. 建立 tenant-scoped usage 记录与幂等 use event。完成标准：一次 Context/Recall 响应对同一 Memory 最多计一次，重放 response 不重复。
2. 在 Context 和 Recall 最终引用提交点记录 usage。完成标准：候选/preview 不计，失败/cancel 不计。
3. 接入生成/使用独立设置与 Session/来源排除。完成标准：设置经 AppService 持久化，生成和读取路径分别执行。
4. 注册 Phase 1、Project、Global、Recall 四类 action assignment。完成标准：每类可选择不同 Agent/model，不按 vendor 分支。
5. 将 usage 纳入保留/排序的确定性信号。完成标准：只影响派生层优先级，不修改 Conversation/Memory 正文。

## Acceptance

- [ ] 只记录实际使用，不记录候选曝光。
- [ ] usage tenant-scoped、幂等且可重开数据库读取。
- [ ] 生成与使用开关互不替代。
- [ ] 排除不删除 Conversation，解除后可重建。
- [ ] 四类 action assignment 独立。
- [ ] settings/usage 不泄漏 secrets 或正文。

## Non-goals

设置 UI、CLI flags、云同步、用户自定义 Memory pipeline。

## Ticket-specific stop

如果 usage 必须通过读取 UI 状态推断，或开关只能硬编码在 frontend，停止并报告。
