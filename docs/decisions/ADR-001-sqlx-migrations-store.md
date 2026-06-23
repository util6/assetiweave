# ADR-001: Use SQLx migrations for the application database

## Status

Accepted

## Date

2026-06-21

## Context

AssetIWeave uses a local SQLite database as the application catalog for sources,
assets, profiles, deployment state, navigation, app shortcuts, asset groups,
conversation records, web records, and sync metadata.

The original backend mixed repository logic with ad hoc rusqlite helpers and a
large test-only `INIT_SCHEMA` string. That made schema drift easy: production
initialization, tests, and future agents could each create tables through a
different path.

The backend now needs one authoritative schema lifecycle:

- fresh databases must be created from ordered migrations;
- existing pre-migration databases must be adopted without losing rows;
- app-owned persistence paths must use the shared Rust store layer;
- frontend and Go CLI callers must go through Tauri commands or the Engine, not
  write SQLite directly.

## Decision

Use SQLx 0.9 with SQLite and SQLx migrations as the authoritative application
database path.

Concrete rules:

- Application database initialization goes through
  `Database::open_initialized`, which runs the embedded SQLx migrator from
  `src-tauri/migrations`.
- Schema changes are represented as timestamped files in
  `src-tauri/migrations`; do not reintroduce hand-written schema initializer
  constants such as `INIT_SCHEMA`.
- App-owned repository modules use `SqlitePool`, SQLx queries, and SQLx
  transactions. New repository functions should carry the `_sqlx` suffix only
  while there is an active migration boundary; after the legacy path is gone,
  the SQLx implementation is the canonical implementation.
- Repository tests should exercise SQLx-backed behavior against temporary
  databases created through `Database`.
- Remaining `rusqlite` usage is allowed only for explicit boundaries that are
  not app-owned repository writes:
  - external conversation source readers that need to inspect Codex/OpenCode
    SQLite files;
  - test-only verification of migration adoption or backup readability.

## Alternatives Considered

### Keep rusqlite for repositories and add migrations separately

- Pros: smaller dependency change.
- Cons: preserves two database access styles and makes async service code keep
  bridging synchronous database calls.
- Rejected: the goal is a single backend persistence architecture, not only a
  migration runner.

### Keep a hand-written schema initializer for tests

- Pros: quick in-memory test setup.
- Cons: creates a second schema source of truth and hides migration drift.
- Rejected: tests must prove the same initialization path used by the app.

### Move all SQLite access to SQLx, including external Codex/OpenCode readers

- Pros: one SQLite crate in the dependency graph.
- Cons: external reader code is not app-owned persistence and currently depends
  on dynamic introspection of third-party SQLite layouts.
- Deferred: this can be revisited separately if the reader code needs async I/O
  or if removing rusqlite entirely becomes a dependency policy.

## Consequences

- Schema review is now migration review.
- App and test initialization use the same migrator, reducing schema drift.
- Repository code can share pooled async database access with background-capable
  Tauri workflows and Engine calls.
- Future changes that add tables, columns, indexes, triggers, or FTS structures
  must include a migration and regression coverage.
- The codebase may still contain rusqlite at non-repository boundaries, but that
  usage must stay isolated and must not create or mutate the app catalog.
