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

## Testing Guidelines

Name frontend tests `*.test.ts(x)` and Go tests `*_test.go`; keep Rust unit tests near the module under test. Add regression coverage for behavior changes. Use a temporary `ASSETIWEAVE_DB_PATH` for tests that could alter local application state.

## Commit & Pull Request Guidelines

Use concise, imperative Conventional Commit subjects, for example `feat: add source filter` or `fix: refresh mount state`. Keep each commit focused. Pull requests should explain behavior and architectural impact, link the relevant issue or spec, list verification commands, and include screenshots for visible UI changes. Do not commit secrets, local logs, build output, or hand-edited generated contracts.
