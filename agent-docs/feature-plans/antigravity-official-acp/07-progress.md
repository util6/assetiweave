# 执行进度

| Task | 状态 | Commit | 关键证据 | 阻塞项 |
|---|---|---|---|---|
| AGACP-00 | PASS | `6048bed` | 5平台真实 archive 下载验证完成；SHA-256 与 executable layout 固化 | — |
| AGACP-01 | PASS | `3a0e4d6` | ACP connection 与 model discovery 解耦；空模型仍保持 protocol ready/connected/execution_ready；生命周期测试对齐 C-04 | — |
| AGACP-02 | PASS | `069f0ec` | 启动协调对比 catalog protocol/distribution；失配标记 incompatible 且阻止进入 Registry；update 返回 agent_reinstall_required，放行 reinstall | — |
| AGACP-03 | PASS | `62d1177` | 5平台 ACP binary 锁定；release evidence 完备；防伪造 conformance 门禁通过；前端 presentation 更新为 ACP Agent | — |
| AGACP-04 | PASS | `45d76d9` | 移除 antigravity selector/parse_agy_models/废弃文件；严格走 ACP 路由；source guard 0 匹配通过 | — |
| AGACP-05 | PASS | `230cec7` | 6 大端到端场景覆盖；安装/健康探针/动态Registry/多错误模式全数通过 | — |
| AGACP-06 | PASS | `2145df5` | 官方 1.1.1 Darwin aarch64 真实 smoke 通过；modelDiscovery=true；partial conformance 固化 | — |
| AGACP-07 | NOT_RUN | — | — | AGACP-00..06 |

## 上游制品证据

| Distribution | Size | SHA-256 | Layout | 状态 |
|---|---:|---|---|---|
| binary-darwin-aarch64 | 316014828 | fdfa915652cdb7ba8085cc8fffed072cbe009251aa2c951aabdda07a8c28a189 | agy_acp_server.par (802163856 bytes) | PASS |
| binary-linux-x86_64 | 681969407 | 38f62d01b32deb0907b3d39a71ec301fd36369f6ffd1cf262d4af385177f79df | agy_acp_server.par (1880360328 bytes) | PASS |
| binary-linux-aarch64 | 656572786 | ed69e64b308fcb123ab54bf3277bf9cb0d651064f885ea5aab0ff520c7175398 | agy_acp_server.par (1862073131 bytes) | PASS |
| binary-windows-x86_64 | 468238392 | 47cb50eef14f0a4655d78cfcfda869bcea7aaee5f9787e936bc2935ea612c3b8 | agy_acp_server.exe (430801616 bytes) | PASS |
| binary-windows-aarch64 | 468521191 | 35f4b1f47ba6a3fea7b0a3e30010df5ea73a64b4f0e7cf991cddc673ddfbcafc | agy_acp_server.exe (435075816 bytes) | PASS |

## 决策与偏差日志

| 日期 | Task | 决策/偏差 | 证据 |
|---|---|---|---|
| 2026-09-07 | planning | modelDiscovery 初始 false；真实 1.1.1 session config 非空后才启用 | 未完成真实 smoke |
| 2026-09-07 | planning | Team resume/history/live 暂时关闭；不保留 Direct-CLI fallback | 本专项文本 ACP 范围 |
| 2026-09-07 | planning | connection probe cleanup 与业务 OneShot 删除契约分离 | Issue #18 与官方 delete 能力未确认 |
| 2026-09-07 | AGACP-01 | lifecycle/mod.rs 故障恢复测试故障注入模式由 no_models 调整为 initialize_error | 符合契约 C-04（空模型非协议失败）；经用户决策批准扩展白名单 |
| 2026-09-07 | AGACP-06 | 官方 1.1.1 实测 session/new 返回 11 个模型，启用 modelDiscovery: true；因官方无 close/delete 声明，conformance 状态如实记录 partial，保持 experimental | Darwin aarch64 真实执行回包与 JSON-RPC 追踪 |


