# ADR-003: Bundle product-owned Skills as read-only system assets

## Status

Accepted

## Date

2026-07-22

## Context

AssetIWeave uses Skills both as catalog assets that can be mounted into AI applications and as stable agent-facing instructions for product workflows. Product-owned Skills such as conversation organization and Memory must stay synchronized with the Rust Engine contract and ship with the application.

The existing `assetiweave-library-skills` source is a tenant-scoped user backup library. Its root can be changed or migrated, and its contents can be imported, backed up, or deleted. Those lifecycle rules are incompatible with application-controlled assets.

## Decision

- Keep product-owned Skill source files under `src-tauri/builtin-assets/skills/`.
- Compile their files into both the desktop application and standalone Engine.
- Install them into `~/.assetiweave/skills/.system` on desktop or Engine startup.
- Register the shared directory in every tenant as the protected `assetiweave-system-skills` source with `SourceOrigin::AssetiweaveSystem`.
- Use a content fingerprint to skip unchanged installs and atomically replace stale or modified installations.
- Prevent editing, deletion, and backup of system Skills. Exposing a system Skill to a target application remains an explicit `asset_mounts` decision.
- Keep business logic in `AppService` and Engine contracts. A bundled `SKILL.md` is an agent-facing adapter to those contracts, not an alternative persistence or workflow engine.

## Alternatives Considered

### Store product Skills in `util6-agents`

Rejected because application releases could not guarantee a matching Skill version, offline availability, or installation path.

### Put system Skills in the tenant backup library

Rejected because custom backup migration, deletion, backup status, and user edits would conflict with application upgrades.

### Bundle only as Tauri resources

Rejected because the standalone Engine must install and expose the same assets without depending on a Tauri application resource directory.

## Consequences

- Adding or changing a built-in Skill changes the application binary fingerprint and is delivered through the normal release process.
- `SourceOrigin` and generated Engine contracts include `assetiweave_system`.
- Built-in executable resources must be validated and kept synchronized with their canonical product implementation.
- User-created and AI-generated Skills remain mutable tenant library assets; they are never written into `.system`.
- The Memory feature can depend on a stable built-in Skill while retaining Card, Question, Session, and Memory records in SQLite.
