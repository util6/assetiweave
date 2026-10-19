# 规格说明书：对话用量统计新增项目（Project）维度

## Problem Statement

As a developer using multiple AI assistants across various repositories and directories, I currently cannot track how many tokens or how much money each individual project consumes. The existing Conversation Usage Dashboard only aggregates usage by time range, model, and data source instance.

When sessions are created inside subdirectories of a Git repository (such as monorepos or nested packages), their paths are fragmented. Furthermore, if a session is subsequently deleted from the workspace, there is a risk that historical project usage could be lost or unrepresented. Users have no way to audit project-level AI spending, compare consumption across repositories, or identify sessions that have no assigned project.

## Solution

Extend the Conversation Usage Dashboard with a first-class Project dimension:
1. Provide a project selector in the toolbar to filter the entire dashboard (hero metrics, daily trend, model breakdown, and instance breakdown) by a specific project or by unassigned sessions.
2. Introduce a "By Project" breakdown table alongside the existing model and instance breakdown tables, presenting request counts, token volumes, and estimated costs per project, with one-click row filtering to drill down immediately.
3. Apply automatic, standardized project path resolution: Git repositories automatically collapse subdirectories into their repository root, whereas non-Git directories remain independent paths.
4. Guarantee that historical usage is permanently preserved: sessions marked as missing or deleted still retain their historical token usage under their original project, and sessions without project metadata are cleanly gathered into an "Unassigned" group.

## User Stories

1. As a project lead, I want to filter the usage dashboard by a specific project path, so that I can see the total token consumption and monetary costs incurred by that project.
2. As a developer, I want to view an overview table comparing token usage and costs across all my projects, so that I can identify which repositories consume the most AI budget.
3. As a developer working in a monorepo or project subdirectory, I want sessions started inside subfolders to automatically roll up into the root Git repository's usage, so that my repository's stats are not fragmented across packages.
4. As a researcher working on standalone folders without Git, I want those distinct folders to be tracked independently, so that non-repository projects are not arbitrarily merged together.
5. As an auditor, I want to see an "Unassigned" project group for sessions without a project path, so that global or ad-hoc chats are accounted for and can be reviewed separately.
6. As a developer, I want to filter the dashboard specifically to the "Unassigned" project group, so that I can inspect the token consumption of sessions that are not tied to any project.
7. As a developer, I want historical usage data to remain visible under its associated project even if the session has been deleted, so that project audit records and financial tracking remain accurate over time.
8. As a developer, I want to click on a project row in the breakdown table, so that the entire dashboard immediately filters to that project without me having to locate it in the dropdown.
9. As a user, I want the project filter dropdown in the toolbar to display home-abbreviated paths (e.g. `~/code/my-repo`), so that paths are concise and easy to read.
10. As a developer, I want to select "All Projects" from the project dropdown, so that I can return to the global dashboard view showing aggregated data across all projects.
11. As a developer, I want the daily trend chart to reflect only the selected project's usage when a project filter is active, so that I can analyze the temporal burn rate of that specific project.
12. As a developer, I want the model breakdown table to show which models were used within the selected project, so that I can evaluate whether expensive models are being overused in that project.
13. As a user, I want the summary hero overview cards to show total tokens and costs for the selected project, so that I can quickly reference total project expenditure.
14. As a user, I want paths with tildes, relative segments, or symlinks to be normalized consistently, so that sessions pointing to the same directory under different representations are grouped together.
15. As a developer, I want the project breakdown table to sort by total token consumption descending by default, so that high-consumption projects appear at the top.
16. As a user, I want the project breakdown table to clearly mark whether an item is the "Unassigned" group, so that I can visually distinguish named projects from orphan sessions.
17. As a user, I want the project selection to coexist harmoniously with date range and instance filters, so that I can query "Project A on Data Source B in the last 7 days".
18. As a user, I want project queries to remain fast even with tens of thousands of historical usage events, so that the dashboard loads smoothly without UI lag.

## Implementation Decisions

- **Domain Filter Extension**: Add an optional `project_path` field to the usage dashboard filter DTO.
- **Project Breakdown Structure**: Introduce a project breakdown DTO containing project path, display name, unassigned flag, token counts (input, output, cache, total), request count, and currency costs.
- **Dashboard Response Extension**: Include a list of project breakdown summaries in the main usage dashboard DTO.
- **Shared Path Normalization**: Place project path resolution in the shared path utilities. For Git repositories, resolve to the Git root directory via upward marker discovery. For non-Git directories, retain the normalized working directory. Cache path resolutions in memory during dashboard aggregation to minimize redundant filesystem access.
- **Relational Session Association**: Associate usage events with session metadata via left outer join on tenant, source, and session identifier (supporting both external identifiers emitted by adapters and internal stable identifiers). Do not filter out missing or soft-deleted sessions, ensuring historical token events remain mapped to their project.
- **Unassigned Sentinel Handling**: Map missing, null, or unresolvable project paths to a standardized unassigned sentinel value (`__unassigned__`).
- **Dashboard Aggregation Pass**: Compute project breakdown in the same aggregation pass as model, source, and date breakdowns. When a project filter is specified, restrict hero metrics, daily trends, model summaries, and source summaries to events matching that project.
- **Toolbar & Table Interaction**: Add a project dropdown in the toolbar alongside the instance selector, formatted with path abbreviations. Add a project breakdown card table alongside model and instance tables. Clicking a row in the project table triggers the project filter.

## Testing Decisions

- **Testing Philosophy**: Verify external behavior through public queries and component interactions rather than internal helper methods or intermediate variables.
- **Store Query Layer Testing**: Using an initialized test database, ingest usage events across different session configurations (Git root session, Git subpackage session, non-Git session, deleted/missing session with `missing = 1`, and session without path), then assert:
  - Query without filter aggregates all usage and returns proper project breakdown list.
  - Query with Git root project filter matches both root and nested subpackage sessions.
  - Query with non-Git project filter matches only that exact directory.
  - Query with unassigned project filter matches only unassigned/null-path events.
  - Usage of deleted sessions remains present under the original project.
- **Frontend Component Testing**: Render `UsageToolbar` and `UsageBreakdownTables` with mock project data, testing that:
  - Project dropdown options render with home abbreviations.
  - Selecting a project triggers the change callback.
  - Clicking a project row in the table triggers project selection.
  - The "Unassigned" project row renders with distinct label.

## Out of Scope

- Modifying raw adapter extraction logic (adapters continue emitting raw `session_id`).
- Adding new persistent columns or database migrations to `conversation_usage_events` (dynamic join and in-memory aggregation is sufficient).
- Manual re-assignment of sessions to different projects from the usage UI.
- Budgeting, quotas, or threshold alerting per project.

## Further Notes

- In compliance with ADR-0017 and project engineering guidelines, all UI components adhere to the AssetIWeave UI design system (pill tabs, rounded glassmorphic cards, standard button heights).
