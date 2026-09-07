# 执行进度

| Task | 状态 | Commit | 关键证据 | 阻塞项 |
|---|---|---|---|---|
| AGACP-00 | NOT_RUN | — | Registry 1.1.1 metadata 已审计；五平台 SHA-256 尚未计算 | 需下载约 2.6 GB 制品 |
| AGACP-01 | NOT_RUN | — | 当前 connection 仍依赖 parse_session_models | — |
| AGACP-02 | NOT_RUN | — | 当前 startup recovery 未比对 catalog protocol/distribution | — |
| AGACP-03 | NOT_RUN | — | 当前 catalog 仍是 native/system agy | AGACP-00..02 |
| AGACP-04 | NOT_RUN | — | 当前 Native backend 仍有 antigravity selector | AGACP-03 |
| AGACP-05 | NOT_RUN | — | 现有 fake ACP 可扩展 | AGACP-04 |
| AGACP-06 | NOT_RUN | — | 旧 RC01 initialize 调查存在；1.1.1 E2E 未执行 | AGACP-05、credential |
| AGACP-07 | NOT_RUN | — | — | AGACP-00..06 |

## 上游制品证据

| Distribution | Size | SHA-256 | Layout | 状态 |
|---|---:|---|---|---|
| binary-darwin-aarch64 | 316014828 | MISSING_EVIDENCE | NOT_RUN | NOT_RUN |
| binary-linux-x86_64 | 681969407 | MISSING_EVIDENCE | NOT_RUN | NOT_RUN |
| binary-linux-aarch64 | 656572786 | MISSING_EVIDENCE | NOT_RUN | NOT_RUN |
| binary-windows-x86_64 | 468238392 | MISSING_EVIDENCE | NOT_RUN | NOT_RUN |
| binary-windows-aarch64 | 468521191 | MISSING_EVIDENCE | NOT_RUN | NOT_RUN |

## 决策与偏差日志

| 日期 | Task | 决策/偏差 | 证据 |
|---|---|---|---|
| 2026-09-07 | planning | modelDiscovery 初始 false；真实 1.1.1 session config 非空后才启用 | 未完成真实 smoke |
| 2026-09-07 | planning | Team resume/history/live 暂时关闭；不保留 Direct-CLI fallback | 本专项文本 ACP 范围 |
| 2026-09-07 | planning | connection probe cleanup 与业务 OneShot 删除契约分离 | Issue #18 与官方 delete 能力未确认 |

