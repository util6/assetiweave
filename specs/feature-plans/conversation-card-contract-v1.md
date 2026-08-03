# Conversation Card Contract v1 实施规格

## Objective

将 Conversation 内容卡片从全局固定的 `answer/tool/command/code/result` 集合，迁移为由每个 App adapter 声明语义类型、由 AssetIWeave Core 提供稳定渲染能力的扩展协议。

完成后：

- 新增语义卡片类型不需要修改 Rust 枚举、前端 JSX、全局设置 schema 或搜索白名单。
- 新增真正的渲染能力仍需要 Core/前端发布，并接受安全与兼容审查。
- 旧 adapter、旧数据库及旧 `metadata_json.content_card` 继续工作。
- 搜索、导出、Memory 和 UI 消费同一份 Core 卡片投影。

## Assumptions and invariants

1. 一个 Conversation Part 最多投影一张内容卡片；需要多张卡片时，adapter 必须拆分为多个 Part。
2. `ConversationPartKind` 描述来源结构，Card `kind` 描述产品语义，两者不得合并。
3. Card `kind` 是开放且稳定的标识符；renderer 是 Core 支持的受控集合。
4. Adapter 不得注入 React 组件、HTML、CSS、脚本或任意 renderer 实现。
5. Parser 输出可重建；译文、注释和其他用户派生数据不可因重解析而丢失。
6. 不建立 `legacy/new/v2` 平行目录；新契约在现有 `backend/conversations`、DTO、store 和前端 Conversation 模块内逐步替换旧路径。

## Contract

### Adapter manifest card kinds

Manifest 可以通过可选字段声明该 App 使用的语义类型：

```json
{
  "card_contract_version": 1,
  "card_kinds": [
    {
      "id": "claude-code.reasoning",
      "semantic_role": "reasoning",
      "label": "Reasoning",
      "default_renderer": "markdown",
      "allowed_renderers": ["markdown"],
      "icon_hint": "brain"
    }
  ]
}
```

规则：

- `id` 必须是稳定、小写、可持久化的 `<adapter-id>.<kind>` 命名空间标识符；旧五类内置 kind 仅由兼容层读取。
- 跨 App 聚合使用可选 `semantic_role`（例如 `reasoning`），不能用无命名空间 kind 代替。
- `allowed_renderers` 限制 Part descriptor 可覆盖的 renderer；未覆盖时使用 `default_renderer`。
- 改名通过 alias/canonicalization 迁移，不直接改变已持久化 ID。
- label 和 icon 只是显示提示，不能包含 HTML/CSS。

### Normalized Part card descriptor

```json
{
  "role": "assistant",
  "kind": "text",
  "text": "分析过程……",
  "content_card": {
    "schema_version": 1,
    "kind": "claude-code.reasoning",
    "renderer": "markdown"
  },
  "metadata_json": {
    "source_type": "thinking"
  }
}
```

首批 renderer：

- `markdown`
- `plain`
- `json`
- `code`
- `command`
- `terminal_output`

旧字段兼容：

```text
content_card.type   -> content_card.kind
content_card.format -> content_card.renderer
contentCard         -> content_card
```

新字段与旧字段同时存在时，Core 必须验证两者语义一致；冲突时拒绝同步，不能静默选择其中之一。

### Core output projection

所有内部消费者使用结构化输出，而不重新解析 `metadata_json`：

```json
{
  "card_id": "conversation-part-...",
  "part_id": "conversation-part-...",
  "adapter_id": "claude-code",
  "kind": "claude-code.reasoning",
  "semantic_role": "reasoning",
  "renderer": "markdown",
  "role": "assistant",
  "body": "分析过程……",
  "language": null,
  "cwd": null,
  "status": null,
  "exit_code": null,
  "translated_body": null,
  "legacy_anchor_ids": ["conversation-part-...-reasoning"]
}
```

`card_id` 恒等于稳定的 `part_id`，不包含 kind、renderer 或旧 suffix；重新分类不得改变 Card 身份。旧 `${part_id}-${type}`/suffix 仅作为 `legacy_anchor_ids` 深链接兼容信息。

## Module boundaries

```text
App source
  -> adapter source parser
  -> external protocol validation
  -> backend/conversations card normalizer
  -> normalized Session/Turn/Part
  -> SQLite
  -> single Card projection
      -> Tauri/Engine DTO
      -> search/index/facets
      -> Markdown export
      -> Memory
      -> frontend renderer registry
```

Recommended locations:

- `src-tauri/src/backend/models/conversation_card.rs`: contract/value types only.
- `src-tauri/src/backend/conversations/cards.rs`: validation, legacy compatibility and projection.
- `frontend/src/components/conversations/`: renderer registry and renderer components.
- Existing repositories call the Core projection; they must not implement their own card metadata parser.

## Validation and security

External adapter output is untrusted even when the executable is trusted.

- Validate descriptor shape, schema version, kind identifier, namespace ownership, renderer and optional field lengths at the external adapter boundary.
- A newly emitted undeclared kind is a conformance error.
- An unsupported renderer is rejected during package validation/runtime gating; `min_core_version` must protect newer renderers.
- Historical database rows with an unknown kind remain visible through a generic safe renderer and produce diagnostics.
- Rendered content remains React-escaped; adapters cannot request raw HTML.
- Card metadata must stay within existing adapter output size limits.

## Persistence and derived data

Migration uses additive canonical-write/dual-read behavior:

1. Core projection initially reads legacy metadata without a database change.
2. Add nullable canonical `content_card_json` columns only after projection behavior is proven；`conversation_adapters` 同时持久化 `card_kinds_json`。
3. Card Contract v1 Adapter 的新导入只写 canonical `content_card_json`；普通来源 metadata 继续保留，但不再复制 Card descriptor。历史 Adapter 与历史数据库仍走 legacy metadata 回退；不执行危险的一次性 JSON SQL 回填。
4. Reads prefer canonical data and fall back to legacy metadata.
5. Old fields are not removed until all consumers and first-party adapters have migrated for at least two release cycles.

Adapter upgrades must participate in hydration identity. Observation state must distinguish:

- source version;
- adapter content hash;
- Core card contract version;
- dirty/reparse state.

Rehydration must preserve user-derived data. Near term, stable Part IDs are upserted and translations survive when the source body hash is unchanged. Long term, translations move out of the replaceable Part row into a dedicated derivation table with `source_body_hash` so stale translations can be identified.

## Consumer migration

### Frontend

- Render by `renderer`, not by semantic `kind`.
- Known kinds use i18n labels/icons; custom kinds use manifest definitions or a safe humanized fallback.
- Color settings become a validated `Record<string, string>` with built-in defaults and generic fallback.
- Visibility filters derive from cards present in the current scope, not a hard-coded global list.

### Search

- Search Card kind becomes a validated string/newtype rather than a closed enum.
- Question documents remain a separate search document variant rather than pretending to be a Card kind.
- Facets are derived from indexed/registered kinds.
- Existing string filters and block IDs remain compatible during migration.

### Export and Memory

- Core owns the default Markdown exporter and renders the ordered Card stream.
- Adapter `export_markdown` remains a temporary compatibility override.
- New Memory/search logic consumes the generic ordered Card stream, not `answer_text/code_text/command_text`.
- Legacy aggregate columns remain maintained until their active consumers reach zero.

## Testing strategy

### Unit

- Parse all legacy five kinds and formats.
- Accept a custom kind with a supported renderer.
- Preserve `json` rather than degrading it to Markdown.
- Reject invalid identifiers, conflicting old/new descriptors and unsupported renderers.
- Use safe plain fallback for unknown historical renderer/kind data.

### Integration

- Adapter -> protocol -> normalized Part -> repository -> DTO projection.
- Session and Web record repositories produce identical Card semantics.
- Search indexes and locates a custom kind and returns dynamic facets.
- Parser/adapter hash change rehydrates an unchanged source session.
- Reclassification with unchanged body preserves translation.

### Frontend

- Unknown semantic kind renders with its declared renderer and generic presentation metadata.
- Filters and colors work without editing a global type union.
- Existing five kinds retain current visual behavior；新 DOM ID 使用稳定 `card_id`，同时解析旧 deep-link IDs。

### Conformance probe

Add a fixture kind such as `claude-code.reasoning` with `semantic_role: reasoning`. Supporting it end to end must not require changing a Rust kind enum, a frontend kind union or a hard-coded search list.

## Commands

```bash
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
pnpm conversation-adapters:check
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
```

Run `pnpm cli:contract` only when Engine-exposed types or methods change; do not edit generated contracts by hand.

## Phased tasks

### Phase 1: Contract and Core projection

- Add contract/legacy compatibility tests first.
- Introduce one Rust card projection module.
- Replace repository and search-index duplicate parsing without changing visible behavior.

### Phase 2: Additive adapter protocol

- Add optional structured `content_card` and manifest `card_kinds`.
- Validate cards during `try-run`, register and sync.
- Keep legacy metadata dual-read.

### Phase 3: Structured DTO and renderer registry

- Expose projected Cards additively in Conversation detail DTOs.
- Migrate frontend rendering to a renderer registry with legacy fallback.
- Convert colors and visibility to dynamic maps.

### Phase 4: Dynamic consumers

- Migrate search type filters/facets.
- Move default Markdown export into Core.
- Migrate Memory to ordered Cards.

### Phase 5: Persistence and hydration

- Add canonical card persistence and backfill.
- Add adapter-aware hydration/reparse state.
- Protect translations and other derived records.

### Phase 6: First-party migration and retirement

- Update first-party adapters and generate bundled copies from one source of truth.
- Measure legacy reads and remove duplicate parsers/exporters only after zero active use.
- 每次同步在结果与 `conversation.sync` operation log 中聚合 `legacy_cards_upgraded`；每次 Session/Web/dry-run 导出在结果与 `conversation.export` operation log 中记录 `legacy_adapter_exporter_used`。
- 旧 `content_types` 参数、metadata 双读、历史锚点和 legacy exporter 至少保留两个发布周期；仅在随后连续一个完整发布周期聚合计数为零时退役。历史数据库读取兼容层等待独立数据迁移，不随代码清理删除。

## Success criteria

- Adding `claude-code.reasoning + semantic_role=reasoning + markdown` changes only an adapter manifest/parser and tests.
- Adding a renderer changes one controlled renderer registry path and declares a compatible minimum Core version.
- Unknown historical card content never disappears.
- Malformed new descriptors fail at the adapter boundary with a precise error.
- Search, export, Memory and UI contain no five-kind parsing whitelist.
- Adapter code changes automatically reparse history even when source files are unchanged.
- Reclassification does not lose a valid translation.
- Current adapters, databases, block IDs and five built-in card behaviors remain compatible.

## Boundaries

- Always: contract tests before behavior changes; additive migrations; parameterized SQL; external input validation; one post-batch search/catalog refresh.
- Ask first: new dependencies, arbitrary plugin UI execution, removal of legacy fields, destructive data migration.
- Never: parallel `v2` implementation tree; hand-edited generated contracts; silent unknown-kind dropping; deletion of user-derived data during parser refresh.
