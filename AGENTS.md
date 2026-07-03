# Repository Guidelines

## Project Structure & Module Organization

- `frontend/src/` contains the React 19 and TypeScript UI. Put reusable controls in `components/ui`, design-system primitives in `components/foundation`, cross-domain composites in `components/common`, and feature code in domain folders such as `components/groups`, `components/assets`, `components/sources`, and `components/conversations`.
- `frontend/src/services/` is the only frontend boundary for Tauri/Engine calls. Pages, hooks, components, schemas, and utils must not bypass it with direct `invoke(...)` calls.
- `src-tauri/src/` contains the complete Rust backend. `adapters/` exposes Tauri commands, stdio Engine protocol, platform glue, CLI tooling, and background task state. `backend/` contains application workflows, capabilities, models, scanner, planner, executor, persistence, conversations, settings, backups, logs, and targeting.
- `cli/` is the Go/Cobra client. It must call the Rust Engine rather than writing SQLite or mount links directly. CLI-specific code may own command UX, plugins, policies, output formatting, update flow, and external harvester orchestration.
- Tests are colocated with source. Supporting material lives in `docs/`, `specs/`, and `scripts/`. See `docs/repository-structure.md` for ownership and migration guidance.

## Runtime Architecture & Data Flow

- Desktop UI flow: `React pages/hooks -> frontend services -> Tauri commands -> AppService -> backend capabilities/store -> SQLite and filesystem`.
- CLI flow: `Go Cobra command -> Engine client -> assetiweave-engine stdio JSON protocol -> command registry/runtime -> AppService -> same backend capabilities/store`.
- `AppService` in `src-tauri/src/backend/application/` is the shared application workflow boundary for Tauri and Engine. Do not create frontend-only or CLI-only business workflows when the action changes persisted app state.
- SQLite is the source of truth for catalog state, tenants, sources, assets, profiles, groups, mounts, navigation, app shortcuts, conversation records, settings, backup metadata, operation logs, and remote Skill records. Schema changes go through `src-tauri/migrations/`.
- `cli/internal/schema/contract.json` is generated from the Rust Engine contract. When Engine methods, DTOs, risks, confirmation requirements, or exposure change, run `pnpm cli:contract`; do not hand-edit generated contracts.
- Browser preview may use `frontend/src/mock/` fallbacks. Mock data must not become a second persistence or rule engine.

## Build, Test, and Development Commands

- `pnpm tauri:dev`: run the desktop application with Vite and Tauri.
- `pnpm dev`: run the browser-only frontend preview on `127.0.0.1:1420`.
- `pnpm typecheck && pnpm test && pnpm build`: run frontend type checks, Vitest, and the production build.
- `cargo fmt --all -- --check && cargo test --workspace`: verify Rust formatting and tests.
- `go vet -C cli ./... && go test -C cli -race ./...`: verify the Go CLI.
- `pnpm cli:contract`: rebuild the Rust Engine and regenerate `cli/internal/schema/contract.json`.
- `pnpm cli:test:e2e`: run installed CLI-to-Engine integration tests.

Use Node 22, pnpm 10, Go 1.24, and a stable Rust toolchain satisfying `src-tauri/Cargo.toml` (`rust-version = "1.96.0"` at the current release line).

## Product Architecture Constraints

- AssetIWeave is a local-first AI file asset mount manager. It manages prompts, rules, memory, skills, MCP config, agent definitions, commands, workflows, conversation records, and related metadata as local assets and records.
- Source directories are read-only by default. Metadata, labels, groups, mount intent, observations, conversation indexes, and settings belong in SQLite or app-owned backup/library directories, not in third-party source repos.
- Default deployment is a single symlink from target App directory to the real source asset. Do not introduce an intermediate symlink pool unless a spec explicitly changes the product decision.
- `asset_mounts` is the single source of mount intent for asset/profile relationships. Catalog quick icons, mount cards, source-level batch mount, group batch mount, group exclusive mount, CLI commands, plan generation, and execution must converge on this model.
- App-local or app-owned source directories have stricter mount rules: avoid directly cross-mounting one App's target directory into another App unless the backend policy explicitly allows it.
- Long-running work such as scanning, backup, import/export, remote acquisition, conversation sync, catalog refresh, batch mount/unmount, and network calls must be background-capable and must not hold the global app lock while doing blocking I/O.
- Conversation records are a separate domain, not Catalog assets. They flow through adapters/sources into normalized Session, Turn, Part, Question, and QuestionTurn records, then search/export/grouping surfaces.
- Remote Skill discovery/import is not a marketplace and not a trust shortcut. GitHub/provider results must be previewed, confirmed, imported into an app-owned library/backup path, recorded with remote metadata, and scanned before use.

## Coding Style & Naming Conventions

Use two-space indentation, strict TypeScript, semicolons, and double quotes. React components and types use `PascalCase`; functions, variables, and hooks use `camelCase`, with hooks prefixed by `use`. Format Rust with `rustfmt` and Go with `gofmt`. UI colors, borders, and shadows must use semantic theme tokens or foundation components, not raw palette values. Extend the current architecture instead of creating parallel `legacy`, `new`, or `v2` trees.

## Product & Frontend Preferences

Use cockpit-tools, VS Code, and Codex App as fast product anchors when starting new UI work: quiet, dense, operational, and built for repeated use. Treat those as style references, not feature requirements.

- Prefer workspace-style layouts with side navigation, top/sub navigation, toolbars, and Finder-like column or list views. Long lists must keep critical controls such as splitters, footers, and toolbar actions reachable without scrolling to the bottom.
- Organize complex records into progressive levels instead of flat lists. Conversation flows should support paths like app -> project folder -> session -> question -> card, so users can move from overview to exact content.
- Keep reusable UI surfaces consistent. Toolbars, dialogs, forms, cards, empty states, settings rows, and detail panels should share foundation/common components and one design language instead of per-page variants.
- Persist durable user preferences in the settings system instead of hard-coding them. Theme, typography, app icons/colors, card colors, preview folding, backup directories, and similar long-lived choices should be configurable and validated.
- Design for power-user workflows: batch selection, bulk mount/unmount, import/export, filtering, syncing, backup, recovery, and review. Avoid single-item-only flows when the domain naturally operates on sets.
- Important app operations should be coverable by the Go CLI through the Rust Engine. Do not let the frontend become the only surface for a workflow that AI agents or scripts need to drive.

## Long-Running Feature Design

Any feature that can scan directories, copy files, sync external records, refresh large catalogs, import/export batches, run network I/O, or touch many database rows must be designed as a background-capable workflow from the start.

- Do not model long-running work as a normal button click that `await`s the whole operation while a page-level `busy` flag disables unrelated UI.
- Tauri commands for long-running work should return a task snapshot quickly, run blocking work through a background task, and expose a read command for the current task state.
- Frontend code should centralize task state in a provider, subscribe to backend events, and use polling as a fallback so missed events do not leave stale progress.
- Every long-running task needs visible progress in the initiating surface and, when the user can navigate away, a global progress indicator.
- Batch workflows must deduplicate inputs, load shared data once, avoid per-item full refreshes, and perform one catalog/status refresh after the batch unless correctness requires narrower updates.
- Backend commands must avoid holding the global app lock while copying files, scanning sources, syncing records, or doing other long-running I/O. Use independent service/database connections and bounded task registries instead.
- While a background task is running, disable only conflicting actions for that task. Filtering, navigation, settings, viewing details, and unrelated CRUD should remain usable.
- App close/exit paths must check running background tasks and warn the user before interrupting work that may leave partial files or database state.
- Engine/CLI contracts must be updated when adding app-visible commands, and CLI-accessible workflows must go through the Rust Engine rather than duplicating persistence or filesystem behavior.
- Regression tests should prove both behavior and responsiveness: task deduplication, progress updates, event/polling fallback, batch refresh count or equivalent effect, and that unrelated UI controls remain enabled.

## Testing Guidelines

Name frontend tests `*.test.ts(x)` and Go tests `*_test.go`; keep Rust unit tests near the module under test. Add regression coverage for behavior changes. Use a temporary `ASSETIWEAVE_DB_PATH` for tests that could alter local application state.

For behavior that crosses UI, Engine, and filesystem boundaries, prefer layered coverage: pure utility tests first, backend repository/service tests next, CLI contract/e2e tests when public commands change, and browser/Tauri manual verification for visible UI or desktop APIs.

## Documentation & Specs

- `docs/` describes current maintenance and usage facts.
- `specs/requirements.md` describes product goals, non-goals, acceptance criteria, and current product phase.
- `specs/design.md` describes the intended architecture and should be kept aligned with real module boundaries.
- `specs/tasks.md` tracks milestone state; update it when Git history or implemented code proves a task changed state.
- When code and specs disagree, inspect code and Git first. Mark unfinished goals as pending rather than documenting them as current behavior.

## Commit & Pull Request Guidelines

Use concise, imperative Conventional Commit subjects, for example `feat: add source filter` or `fix: refresh mount state`. Keep each commit focused. Pull requests should explain behavior and architectural impact, link the relevant issue or spec, list verification commands, and include screenshots for visible UI changes. Do not commit secrets, local logs, build output, or hand-edited generated contracts.
