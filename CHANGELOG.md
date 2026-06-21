# Changelog

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
