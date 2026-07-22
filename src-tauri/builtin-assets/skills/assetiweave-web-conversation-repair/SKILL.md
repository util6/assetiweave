---
name: assetiweave-web-conversation-repair
description: 诊断并修复 AssetIWeave 对话采集脚本，重点处理 Qwen、Gemini 等网页记录 Harvester 因 Node 路径、版本、脚本缺失或漂移、文件权限、浏览器认证过期而失效的问题。用户反馈网页对话无法同步、采集脚本突然失败、换环境后不能运行，或要求安全重装并验证采集链路时使用。
---

# Web Conversation Repair

先诊断，再预览修复，最后验证完整采集链路。保留 `requests/` 中的认证状态和 `output/` 中的既有采集结果；不要通过删除整个 Harvester 目录解决问题。

## 状态机

按 `DIAGNOSE -> PLAN -> CONFIRM -> REPAIR -> AUTH -> RUN -> IMPORT -> VERIFY` 顺序执行。任何写操作前先展示计划，需要 `--yes` 时必须等待用户明确确认。

### 1. 诊断

```bash
assetiweave-cli harvester doctor <harvester-id>
```

根据结构化检查区分：

- `package`：Manifest、入口脚本、Adapter 或网页配置缺失/漂移
- `runtime`：Node/Python/Bash 不存在或版本不满足
- `auth`：认证请求尚未配置、已过期或浏览器中没有可读取登录状态
- `output`：脚本运行成功但没有生成标准化 Session 文件

不要在缺少诊断证据时直接重装。

### 2. 预览并修复官方脚本

```bash
assetiweave-cli harvester repair <harvester-id> --dry-run
assetiweave-cli harvester repair <harvester-id> --yes
```

Repair 只恢复官方模板拥有的静态文件和执行权限，并保留认证请求与输出。社区 Harvester 没有可信安装来源时，停止并要求用户提供原始 `--from` 地址或目录。

Repair 后重新运行 `harvester doctor`。如果仍是 runtime 问题，不要反复重装；报告诊断中的实际程序、版本要求和设置提示。

### 3. 恢复认证

先检查现有认证：

```bash
assetiweave-cli conversation web auth-check <harvester-directory>
```

只有认证无效时，才从本机已登录浏览器重新检测：

```bash
assetiweave-cli conversation web auth-detect <harvester-directory> --browser auto --domain <domain> --credential auto
```

不要输出 Cookie、Token 或 `requests/auth-probe.json` 内容。认证仍失败时，引导用户在受支持浏览器中登录对应网站后重试。

### 4. 运行与导入

执行本地脚本属于可信代码执行，先取得确认：

```bash
assetiweave-cli harvester run <harvester-id> --yes
```

运行成功后，确认输出中的 `normalized_file`、`session_count` 和 `turn_count`。随后同步对应 Conversation Source：

```bash
assetiweave-cli conversation sync --source <source-id> --dry-run
assetiweave-cli conversation sync --source <source-id>
```

### 5. 验证

```bash
assetiweave-cli conversation web-record list --source <source-id> --limit 5
assetiweave-cli conversation web-record get <record-id>
```

只有 Doctor 无阻断项、Harvester 成功生成标准化文件、Source 同步成功且能读取至少一条网页记录，才报告修复完成。网站接口变化导致解析仍失败时，保留 raw 输出并报告需要升级官方模板，不修改来源网站或伪造记录。
