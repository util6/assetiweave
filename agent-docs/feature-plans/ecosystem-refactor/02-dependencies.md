# 依赖锁定表与官方依据

查证日期：2026-09-03。版本来自 npm registry / crates.io 元数据；**元数据兼容不是本仓编译通过的证明**。A00 核对执行时工具链，安装卡用精确版本解析并提交锁文件，运行本卡 API/业务测试后才确认采用。普通执行不自动追踪 latest。

## 前端：按所属卡安装，不一次性堆包

命令在仓库根执行；表内运行依赖使用 `pnpm add -E`，开发依赖使用 `pnpm add -DE`。同一个包只由首张 owner 卡安装。

| Owner | 精确包名/版本 | 替换结果与依据 |
|---|---|---|
| A-F01 | eslint@10.9.1、@eslint/js@10.0.1、typescript-eslint@8.69.0、eslint-plugin-react-hooks@7.1.1（dev） | [flat config](https://eslint.org/docs/latest/use/configure/configuration-files)、[TS 集成](https://typescript-eslint.io/getting-started/)；Node 22.13+，TS 5.8.3 落在 >=4.8.4 <6.1 peer 范围 |
| A-F02 | prettier@3.9.6、eslint-config-prettier@10.1.8（dev） | [分别运行 formatter 和 linter](https://prettier.io/docs/integrating-with-linters)；不安装 eslint-plugin-prettier |
| A-F03 | @tanstack/react-router@1.170.32 | [memory history](https://tanstack.com/router/latest/docs/guide/history-types)；React >=18、Node >=20.19 |
| A-F04 | @tanstack/react-query@5.102.8 | [query lifecycle](https://tanstack.com/query/latest/docs/framework/react/overview)；React 18/19 |
| A-F09 | zustand@5.0.15 | [selector store](https://zustand.docs.pmnd.rs/learn/getting-started/introduction)；不为可选 peer 自动增加 immer |
| A-F10 | i18next@26.4.1、react-i18next@17.0.13 | [React 集成](https://react.i18next.com/getting-started)；两者配套，后者要求 i18next >=26.2.0 |
| A-F11 | react-hook-form@7.87.0、@hookform/resolvers@5.9.1 | [Zod resolver](https://github.com/react-hook-form/resolvers#zod)；复用已安装 Zod 4，不安装其他可选校验 peer |
| A-F13 | react-resizable-panels@4.12.3 | [官方 API](https://github.com/bvaughn/react-resizable-panels)；4.x Group/Panel/Separator，核对 pixels/percent，别抄 2.x API |
| A-F14 | react-markdown@10.1.0、remark-gfm@4.0.1、remark-math@6.0.0、rehype-katex@7.0.1 | [官方 pipeline](https://github.com/remarkjs/react-markdown)；复用现有 KaTeX/Mermaid/Diff |

安装示例（其余卡替换为本表 owner 行的完整包名，不能跨卡安装）：

```sh
# A-F01
pnpm add -DE eslint@10.9.1 @eslint/js@10.0.1 typescript-eslint@8.69.0 eslint-plugin-react-hooks@7.1.1
# A-F03
pnpm add -E @tanstack/react-router@1.170.32
# A-F04
pnpm add -E @tanstack/react-query@5.102.8
```

ESLint 浏览器/Node globals 用当前 config 的显式 environment 范围定义；若采用额外 `globals` 包，先记录 exact version/理由、更新本表再安装，不隐式依赖转接包。Oxlint 留为独立测量候选，不默认与 ESLint 重复跑同一规则；当前 [官方迁移文档](https://oxc.rs/docs/guide/usage/linter/migrate-from-eslint) 已有 plugin/type-aware 能力，取舍看实际规则覆盖而非陈旧结论。

## Rust：合并到已有 dependencies，不复制整份 Cargo.toml

```toml
# A-R01
config = { version = "=0.15.25", default-features = false }
# A-R03
tracing = "=0.1.44"
tracing-subscriber = { version = "=0.3.23", features = ["env-filter"] }
tracing-appender = "=0.2.5"
# A-R04
anyhow = "=1.0.104"
# A-R05
validator = { version = "=0.21.0", features = ["derive"] }
# A-R06：替换已有这一行，保留 compat
tokio-util = { version = "=0.7.19", features = ["compat", "rt"] }
# A-R08：保留原 ureq 默认 gzip/no-proxy 行为
reqwest = { version = "=0.13.4", default-features = false, features = ["blocking", "json", "rustls", "gzip"] }
```

- 官方依据：[config](https://docs.rs/config/latest/config/)、[tracing](https://docs.rs/tracing/latest/tracing/)、[subscriber](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)、[appender](https://docs.rs/tracing-appender/latest/tracing_appender/)、[anyhow](https://docs.rs/anyhow/latest/anyhow/)、[validator](https://docs.rs/validator/latest/validator/)、[TaskTracker](https://docs.rs/tokio-util/latest/tokio_util/task/struct.TaskTracker.html)、[reqwest blocking](https://docs.rs/reqwest/latest/reqwest/blocking/)。
- 这些新 crate 声明的 rust-version 不高于当前 1.96.0；validator 0.21 为 1.88、config/reqwest 为 1.85。传递依赖与平台构建仍需实测。
- reqwest 0.13 TLS feature 是 `rustls`，不是旧文档的 `rustls-tls`。任务一采用 blocking API 的有限桥接；禁止因为使用该包顺带新建 Tokio runtime/HTTP 微框架。
- thiserror 2、semver 1、SQLx 0.9、Tokio、Serde 已经存在。按现有 Cargo.lock 复用，不做无关版本升级。ureq 2 在 A-R10 完成所有迁移后移除。
- config 暂不启用文件格式 features，不创建与 SQLite settings 竞争的 config 文件。

## 兼容失败的确定动作

卡片停在安装/探针步骤，交接包含完整 peer/feature/编译错误、工具链和官方版本信息；将最小兼容修正写入当前卡与本表，经审查后继续。不得升级 React/TypeScript/Tauri 来掩盖单库问题，不使用 `--force`/忽略 peer 作为验收。
