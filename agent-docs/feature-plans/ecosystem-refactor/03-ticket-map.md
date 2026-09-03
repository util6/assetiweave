# 任务一：单卡队列与依赖图

当前共 **32 张卡**，全部为编制时 PLANNED。Issue #22 最新交接保存执行状态；每轮只选择前置已 VERIFIED 的一张。以下是默认串行次序，不要求模型自行并行。

| 顺序 | Ticket | 结果 | 前置 |
|---|---|---|---|
| 1 | [A00](tickets/A00-baseline.md) | 冻结可验证基线与依赖安装前提 | 无 |
| 2 | [A-F01](tickets/A-F01-eslint.md) | ESLint 接管前端静态规则 | A00 |
| 3 | [A-F02](tickets/A-F02-prettier.md) | Prettier 接管格式并建立检查入口 | A-F01 |
| 4 | [A-F03](tickets/A-F03-router.md) | TanStack Router 接管实际工作区导航 | A-F02 |
| 5 | [A-R01](tickets/A-R01-runtime-config-loader.md) | 用 config 建立四字段启动配置解析器 | A00 |
| 6 | [A-R02](tickets/A-R02-runtime-config-adoption.md) | 接入启动配置并删除散落 env 解析 | A-R01 |
| 7 | [A-F04](tickets/A-F04-query-catalog.md) | Query 接管租户与 Catalog 读取 | A-F03 |
| 8 | [A-F05](tickets/A-F05-query-mutations.md) | Query 接管 Catalog 写入和失效 | A-F04 |
| 9 | [A-C01](tickets/A-C01-locale-contract.md) | SQLite 语言/分栏设置与原子语言导入契约 | A00、A-R02、A-F04 |
| 10 | [A-F06](tickets/A-F06-query-settings.md) | Query 接管应用设置读取与保存 | A-F05、A-C01 |
| 11 | [A-F07](tickets/A-F07-query-task-pilot.md) | Query 接管搜索索引后台任务 | A-F06 |
| 12 | [A-F08](tickets/A-F08-query-task-consumers.md) | 剩余后台任务迁移并删除自研请求运行时 | A-F07 |
| 13 | [A-F09](tickets/A-F09-zustand-ui.md) | Zustand 接管共享UI状态 | A-F08 |
| 14 | [A-R03](tickets/A-R03-tracing-logging.md) | 用 tracing 接管日志写入和上下文 | A-R02 |
| 15 | [A-R04](tickets/A-R04-typed-error-sources.md) | thiserror 错误派生、source 保留与受限 anyhow | A-R03 |
| 16 | [A-R05](tickets/A-R05-validator-boundaries.md) | validator 接管通用 DTO 字段校验 | A-R04 |
| 17 | [A-R06](tickets/A-R06-task-tracker.md) | TaskTracker 替换手工活动计数与 Condvar | A-R04 |
| 18 | [A-R07](tickets/A-R07-cancellation-token-bridge.md) | 移除 HostProcess 取消镜像线程 | A-R06 |
| 19 | [A-F10](tickets/A-F10-i18next.md) | react-i18next 接管翻译并接通SQLite语言偏好 | A-F09、A-C01、A-F06 |
| 20 | [A-F11](tickets/A-F11-source-forms.md) | React Hook Form接管来源表单 | A-F10 |
| 21 | [A-F12](tickets/A-F12-group-forms.md) | React Hook Form接管分组表单 | A-F11 |
| 22 | [A-F13](tickets/A-F13-resizable-panels.md) | 成熟分栏组件接管 resize 与尺寸偏好 | A-F12、A-F06、A-C01 |
| 23 | [A-F14](tickets/A-F14-markdown-pipeline.md) | Markdown AST 生态替代手写解析器 | A-F13 |
| 24 | [A-F15](tickets/A-F15-query-cache-consumers.md) | 迁移剩余共享缓存消费者并删除 asyncCache | A-F14、A-F05、A-F08 |
| 25 | [A-F16](tickets/A-F16-frontend-convergence.md) | 前端成熟生态收口与 CI 门禁 | A-F15 |
| 26 | [A-R08](tickets/A-R08-reqwest-client-boundary.md) | 建立 reqwest blocking 客户端与确定的生命周期 | A-R02、A-R04、A-R07 |
| 27 | [A-R09](tickets/A-R09-reqwest-catalog-json.md) | 迁移 catalog 与 GitHub JSON 请求 | A-R08 |
| 28 | [A-R10](tickets/A-R10-reqwest-artifacts-remove-ureq.md) | 迁移工件下载并移除 ureq | A-R09 |
| 29 | [A-R11](tickets/A-R11-semver-runtime.md) | 复用 semver 替代 Runtime 版本比较器 | A-R04 |
| 30 | [A-R12](tickets/A-R12-sqlx-rows.md) | SQLx typed row 接管机械数据库映射 | A-R04 |
| 31 | [A-C02](tickets/A-C02-unified-contracts.md) | 依赖接管后的跨层契约统一 | A-F16、A-R03、A-R04、A-R05、A-R06、A-R07、A-R10、A-R11、A-R12、A-C01 |
| 32 | [A-G01](tickets/A-G01-acceptance.md) | 任务一独立验收与交付 | A-C02 |

## 结构与收口

- A00 是工作区/行为基线，不以安装包作为完成。
- A-F01/F02 建检查入口；Router + Config 优先落地。
- A-F04–F16、A-R03–R12 按明确领域接管成熟库；A-C01 为 i18n/设置提供必要共享字段，不是提前建设统一平台。
- **全部依赖切换后才执行 A-C02 统一跨层契约，再由 A-G01 独立验收。**
- A-G01 通过后，才能在另一个目录调度任务二 B00。任务二的完成不是本任务验收前提。

## 写入互斥

`package.json/pnpm-lock.yaml`、`Cargo.toml/Cargo.lock`、`AppProviders.tsx`、`settingsSchema.ts`、`runtime/app_runtime.rs`、AppService/Engine registry、生成 contract 都是共享文件。上述卡默认串行；只有维护者明确分配不相交的文件所有权才并行。先写“共用接口将由另一位实现”不等于前置已满足。

发生源码漂移或卡内改动需新增生产文件时，先更新该卡的 Modify/Create/Test 和依赖，经审查再执行；不要跳过小卡边界变成整仓重写。
