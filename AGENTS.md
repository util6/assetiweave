# Repository Guidelines

## Project Structure & Module Organization

- `frontend/src/` contains the React 19 and TypeScript UI. Put reusable controls in `components/ui`, design-system primitives in `components/foundation`, cross-domain composites in `components/common`, and feature code in domain folders such as `components/groups`.
- `src-tauri/src/` contains the complete Rust backend: shared models, Tauri commands, Engine protocol, persistence, mount operations, and conversation processing.
- `cli/` is the Go/Cobra client. It must call the Rust Engine rather than writing SQLite or mount links directly.
- Tests are colocated with source. Supporting material lives in `docs/`, `specs/`, and `scripts/`. See `docs/repository-structure.md` for ownership and migration guidance.

## Build, Test, and Development Commands

- `pnpm tauri:dev`: run the desktop application with Vite and Tauri.
- `pnpm dev`: run the browser-only frontend preview on `127.0.0.1:1420`.
- `pnpm typecheck && pnpm test && pnpm build`: run frontend type checks, Vitest, and the production build.
- `cargo fmt --all -- --check && cargo test --workspace`: verify Rust formatting and tests.
- `go vet -C cli ./... && go test -C cli -race ./...`: verify the Go CLI.
- `pnpm cli:contract`: rebuild the Rust Engine and regenerate `cli/internal/schema/contract.json`.
- `pnpm cli:test:e2e`: run installed CLI-to-Engine integration tests.

Use Node 22, pnpm 10, Go 1.24, and the stable Rust toolchain.

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

## Testing Guidelines

Name frontend tests `*.test.ts(x)` and Go tests `*_test.go`; keep Rust unit tests near the module under test. Add regression coverage for behavior changes. Use a temporary `ASSETIWEAVE_DB_PATH` for tests that could alter local application state.

## Commit & Pull Request Guidelines

Use concise, imperative Conventional Commit subjects, for example `feat: add source filter` or `fix: refresh mount state`. Keep each commit focused. Pull requests should explain behavior and architectural impact, link the relevant issue or spec, list verification commands, and include screenshots for visible UI changes. Do not commit secrets, local logs, build output, or hand-edited generated contracts.
