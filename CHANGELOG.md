# Changelog

## v0.5.1

- Add debounced search controls and tighter toolbar layouts across catalog, source, group, mount, manual, conversation, and prompt workflows.
- Remove project paths from web conversation records with a forward SQLite migration and aligned search behavior.
- Improve ChatGPT and Gemini web harvesting through reusable CDP browser helpers and stronger authentication probing.
- Improve Codex conversation extraction, preserve useful browse results, and filter internal adapter truncation markers from content cards.
- Enhance conversation browsing with denser navigation, collapsible question lists, richer result context, and improved filtering.
- Add native prompt image clipboard support, pasted image attachments, draft recovery, tag libraries, project selection, and card interaction refinements.
- Add Windows frameless window chrome with native drag, minimize, maximize, and close controls.

## v0.5.0

- 新增灵感夜航（promptStudio）主题作为默认主题，面板卡片采用渐变背景，按钮使用渐变高亮。
- 新增提示词工作台页面，支持快速记录灵感、Feature 想法和 Prompt 草稿，并在卡片上直接复制、翻译和优化。
- 翻译功能扩展为 provider/cli/model/prompt 四层配置模式，支持 opencode/gemini CLI 工具、模型列表查询、连接检测和自定义提示词模板。
- conversation_parts / web_record_parts 新增 translated_text 持久化列，翻译结果可直接写入数据库。
- 内容搜索支持按 question/answer/tool/command/code/result 卡片类型筛选，支持 Enter 和按钮即时提交，防抖调整为 700ms。
- 同步进度新增 advice 字段，部分来源失败时给出修复建议。
- 新增 groupSourceDisplayAssets 工具函数，SourceList 复用分组逻辑。
- harvester 支持适配器目录执行，register_external_adapter 注册前执行 probe 验证适配器可用性。
- 新增 refreshCatalogAndMountState 批量刷新方法，CatalogPage 和 SourcesPage 使用并行刷新。
- Engine 注册 translate_conversation_card、test_conversation_translation_connection、list_conversation_translation_models 等新命令。

## v0.2.0

- Remove built-in Claude Code adapter; Claude Code, ZCode and similar sources now sync through external plugin adapters with manifest-based execution.
- Add conversation import dialog for selecting external adapter manifests and local record sources with guided step-by-step flow.
- Split large backend `mod.rs` files (application, conversations, capabilities, scanner, etc.) into focused submodules for maintainability.
- Add `harvester` module to run external adapters before sync, ensuring fresh local records.
- Adapter `readSession` output now filters empty turns and assigns `turn_index` for stable ordering.
- Sync flow gains `record_kind` awareness so session and web record pages show independent progress and dismiss states.
- Introduce `PageMetrics` component; move page-level metrics from toolbar into header area.
- Add web record sync i18n copy (14 new translation keys for phase/description/summary).
- `preferredConversationQuestionId` selects the first question with assistant content by default.
- Bump content card schema versions for Claude Code (v3), Codex (v4), OpenCode (v3), and web adapters (v2).

## v0.1.4

- Migrate the Rust backend persistence layer to SQLx migrations, repositories, and application services while removing legacy store/service paths.
- Add SQLx-backed regression coverage for asset catalogs, mount operations, skill groups, backups, web records, and conversation workflows.
- Expand CLI and Engine coverage for conversation and web harvester workflows, including bundled Gemini and Qwen web harvester templates.
- Improve background task progress handling, dialog/tooling consistency, conversation navigation, source/group workflows, and update/release validation.
- Require Rust 1.96 for the backend toolchain.

## v0.1.3

- Fix CLI release archive builds so `scripts/build-cli.js` resolves custom output paths from the repository root before running from the `cli/` package.
- Supersede the failed `v0.1.2` draft release attempt with a clean release build.

## v0.1.2

- Add Skill search/acquire surfaces, group bulk workflows, and richer source management controls.
- Add the conversation session browser with normalized content blocks, export controls, and manual guidance for adapters and aICLI/assetiweave-cli usage.
- Remove the standalone conversation source and adapter page routes; keep source and adapter operations in the CLI/sync layer.
- Refresh CLI contract, release audit coverage, and updater-ready release metadata.

## v0.1.1

- Add signed in-app update checks, downloads, installation, and restart.
- Publish `latest.json` and updater signatures from the GitHub Release workflow.
- Publish updater-enabled releases as stable releases so the configured `/releases/latest` endpoint resolves.

## v0.1.0

- Initial GitHub release.
- Desktop installers for Windows and Linux, macOS app archives, and optional macOS DMGs are produced by GitHub Actions.
- CLI tool archives include `assetiweave-cli` and `assetiweave-engine` for supported platforms.
