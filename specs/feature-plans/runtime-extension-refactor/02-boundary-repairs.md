# SPEC-02:Store 边界修复、Projection 中立化与依赖守卫(P0)

- 状态:Draft v3(v1 审计 #10/#11;v2 复审 #4/#10 修订)
- 前置:无(可与 SPEC-01 并行;§3 第 4 步与 SPEC-01 bootstrap 有交接点)
- 交付物:`application/bootstrap.rs`、`backend/projection/` 新模块、store 清理、`scripts/check-module-boundaries.sh` + CI 接线

---

## 1. 目标

1. `backend/store/` 回归纯持久化职责:只接收已准备好的数据,MUST NOT 执行文件系统副作用、manifest 校验或调用运行时/引导逻辑。
2. 会话卡片 projection 迁入**中立 read-model 模块**,store 与 application 都只作为其调用方。
3. 用 CI 守卫固化依赖方向,防止边界回潮。

### 非目标

- 不重构 store 的 repo 划分与 SQL;不改 `conversations/cards.rs` 的投影算法本身(只搬家、不改逻辑);不处理 store→conversations 的**纯类型**引用(共享类型引用是合法的,见 §5 规则 3)。

## 2. 现状(证据,按符号定位)

| 问题 | 位置 | 行为 |
|---|---|---|
| store 内做官方 adapter 物化 | `store/conversation_repo.rs` → `seed_builtin_conversation_adapters_sqlx` 调 `conversations::ensure_official_conversation_adapters()` | `conversations/official.rs` 中 `create_dir_all`、`write_if_missing`、`set_mode(0o755)` |
| store 内做外部 manifest 校验 | 同文件 → `conversations::validate_external_adapter(...)` | 读文件并校验 |
| store 反向调用投影 | `store/search_index_repo.rs` → `conversations::cards::project_persisted_content_card(...)` | 持久化层依赖领域投影实现 |

叠加 SPEC-01 的发现:上述 seed 逻辑处于 `open_with_db_path` 链路上,基线里被 136 个入口反复触发。

## 3. 设计与实施步骤

### 步骤 1:新建中立投影模块 `backend/projection/`

- 文件:`backend/projection/mod.rs`、`backend/projection/conversation_cards.rs`。
- 将 `conversations/cards.rs` 中被 store 使用的投影入口(`project_persisted_content_card`、`PersistedConversationCardProjectionSource` 及其直接依赖的纯函数)**整体搬移**至 `projection/conversation_cards.rs`;`conversations/cards.rs` 保留 `pub(crate) use crate::backend::projection::conversation_cards::*;` 转发一版,待调用方全部改引后删除转发。
- 依赖约束:`backend/projection/` MUST 只依赖 `backend/models/`、`backend/dto/`、标准库与 serde;MUST NOT 依赖 `store`、`application`、`conversations` 的 IO 部分。
- 搬移是机械操作:函数体逐字保留,仅调整 `use` 路径。若发现被搬函数依赖了 conversations 内的 IO 工具,则该函数不属于投影,停止并上报,不得强搬。

### 步骤 2:adapter 物化上移到 `application/bootstrap.rs`

新建 `backend/application/bootstrap.rs`:

```rust
/// 应用引导期的一次性物化与登记。由 AppRuntime::bootstrap(SPEC-01 §4 第 5 步)在其
/// tokio Runtime 上 await;SPEC-01 未合入期间,由 open_with_db_path 在现调用点经
/// Database::block_on 等价调用(行为不变)。
/// 修订(审计 #10):不接收 Database——生产路径中 Database 将降级为测试工具(SPEC-01),
/// 本函数只依赖 SqlitePool;同步文件物化经 spawn_blocking 隔离。
pub(crate) async fn materialize_and_seed_builtin_adapters(
    pool: &SqlitePool, tenant_id: &str,
) -> AppResult<()> {
    // 1) 文件物化 + manifest 读取(原 ensure_official_conversation_adapters,仍住
    //    conversations/official.rs;阻塞 IO,spawn_blocking 包裹)
    let adapters = tokio::task::spawn_blocking(
        crate::backend::conversations::ensure_official_conversation_adapters,
    )
    .await
    .map_err(map_join_error)??;   // 修订(v2 复审 #10):JoinError 不能经 ? 自动转换;
                                  // map_join_error 区分「任务被取消」与「闭包 panic」两种失败
    // 2) 外部 adapter 校验(原 store 内联逻辑上移至此,同为阻塞 IO,spawn_blocking 包裹)
    let validated = validate_external_adapters_blocking(...)
        .await
        .map_err(map_join_error)??;
    // 3) 纯数据交给 store 落库
    store::seed_builtin_conversation_adapters_sqlx(pool, tenant_id, adapters, validated).await
}
```

- `store::seed_builtin_conversation_adapters_sqlx` 改签名:接收 `Vec<ConversationAdapter>`(及外部校验结果),内部只剩 upsert 决策(保留现有 trust_state/enabled 合并规则,逐字保留)。
- `builtin_sources` 等纯数据 seed 留在 store,合法。
- **seed 拆分(修订,v2 复审 #4)**:`seed_tenant_defaults_sqlx`(`store/database.rs`)现于内部直接调用 adapter seed——MUST 移除该调用,通用 seed 与 adapter seed 彻底分离;其全部调用点(基线 4 处:bootstrap、新建 tenant、system reset 等)同步改为"通用 seed → `materialize_and_seed_builtin_adapters`"两段式,并为每个调用点补回归测试(建 tenant 后 adapter 仍就位)。
- **事件追加边界注记(v2 复审 #1 配套)**:store 允许在自己的事务内**追加由上层构造好的 `DomainEvent` 行**(SPEC-04 §4)——事件是数据不是行为,不属于本 SPEC 禁止的越界;守卫规则不拦 `backend::events` 的类型引用。

### 步骤 3:store 清理

- 删除 `store/conversation_repo.rs` 对 `ensure_official_conversation_adapters`、`validate_external_adapter` 的 import 与调用。
- `store/search_index_repo.rs` 的投影调用改为 `crate::backend::projection::conversation_cards::...`。

### 步骤 4:与 SPEC-01 的交接

- 若 SPEC-01 已合入:`AppRuntime::bootstrap` 第 5 步调用 `materialize_and_seed_builtin_adapters`,全进程一次。
- 若 SPEC-01 未合入:在 `application/system.rs` 现 seed 调用点替换为新函数,行为等价(仍每次 open 执行,由 SPEC-01 收敛为一次)。两个 SPEC 的合入顺序任意,交接点只有这一处。

### 步骤 5:依赖守卫脚本

新建 `scripts/check-module-boundaries.sh`(POSIX sh + grep,零依赖),CI 与 `package.json` 增加 `pnpm check:boundaries`:

```sh
#!/bin/sh
# 模块边界守卫。规则违例 → 非零退出。数字基线只许减不许增。
set -e
fail() { echo "BOUNDARY VIOLATION: $1"; exit 1; }
S=src-tauri/src

# R1 store 禁止引用引导/运行时/校验逻辑
grep -rn "ensure_official_conversation_adapters\|validate_external_adapter" $S/backend/store/ && fail "store must not call bootstrap/validation" || true
# R2 store 禁止引用 application 与 projection 之外的 conversations 非类型模块
grep -rn "backend::conversations::\(official\|external\|harvester\|io_utils\|package\)" $S/backend/store/ && fail "store->conversations io modules" || true
grep -rn "backend::application" $S/backend/store/ && fail "store->application" || true
# R3 models 保持无业务依赖
grep -rn "backend::\(store\|application\|capabilities\|conversations\|scanner\|planner\|executor\)" $S/backend/models/ && fail "models must stay dependency-free" || true
# R4 projection 只依赖 models/dto:全量禁止清单 + 禁 IO(审计 #11:弱于声明的守卫会立即回潮)
grep -rn "backend::\(store\|application\|capabilities\|conversations\|scanner\|planner\|executor\|search\|agent_market\|agents\|ai_execution\)" $S/backend/projection/ && fail "projection must stay neutral" || true
grep -rn "std::fs\|tokio::fs\|std::process\|crate::adapters" $S/backend/projection/ && fail "projection must not do IO" || true
# R5 计数基线(维护于本脚本;修复后手动下调,禁止上调)
check_max() { n=$(grep -rn "$2" $3 | wc -l | tr -d ' '); [ "$n" -le "$1" ] || fail "$2 count $n exceeds baseline $1"; }
check_max 304 "block_on" "$S"                        # SPEC-01 §6
check_max 999 "Legacy(" "$S"                         # SPEC-01 §7,首次落地后改为实际值
check_max 0   "open_with_db_path" "$S/adapters"      # SPEC-01 完成后启用;之前设为 136
```

规则 3 说明:store 引用 `conversations::types::*` 这类**纯类型**是允许的,守卫只拦 IO/引导/校验模块。

## 4. 验收标准

- `pnpm check:boundaries` 在 CI 绿;人为制造一条违例(临时提交)能红。
- `grep -rn "create_dir_all\|fs::write\|set_mode" src-tauri/src/backend/store/` 输出为空。
- 既有测试全绿;`conversations/tests.rs` 与 `application/tests.rs` 中涉及 seed 与搜索投影的用例不改断言即通过(允许只改 use 路径与调用点)。
- 新增测试:
  - `application::bootstrap::tests::seed_receives_prepared_data_and_writes_nothing_to_fs`(用只读临时目录证明 store 路径无文件写);
  - `projection::conversation_cards::tests::projection_is_pure`(同输入两次调用结果字节相等)。

## 5. 风险与回滚

- 风险:投影函数隐藏 IO 依赖导致搬移受阻 → 按步骤 1 的停止条款上报,缩小搬移范围为"store 实际调用的最小闭包"。
- 回滚:三个步骤各自独立 PR,任一可单独 revert;守卫脚本最后合入。
