# Handoff: EggDone F1 Complete and F2 Next

## Session Metadata

- Created: 2026-06-07 21:31:01
- Project: `D:\Develop\EggDone`
- Branch: `main`
- Session duration: approximately 45 minutes

### Recent Commits

- `c215bed` feat: 添加任务置顶功能
- `7f9ab87` feat: 实现任务搜索和已完成任务隐藏功能
- `3859a5a` feat: 添加功能优化规划文档和交接记录
- `1e9bc22` feat: 添加Windows安装器自动验证脚本及相关文档
- `898a559` feat: 添加MIT许可证、Windows构建配置及发布文档

## Handoff Chain

- **Continues from**: [2026-06-07-210627-eggdone-sync-release-and-feature-roadmap.md](./2026-06-07-210627-eggdone-sync-release-and-feature-roadmap.md)
- **Supersedes**: None

The previous handoff covers synchronization, Windows release work, and the initial product roadmap. This handoff records the completed F1 feature phase and prepares the F2 date/reminder phase.

## Current State Summary

F1 “查找与列表整理” is complete and committed. EggDone now supports collapsible title search, persisted show/hide-completed preference, distinct filtered empty states, and synced task pinning. Search remains a display-only filter and disables reorder while active. Pinning is persisted through SQLite schema version 5, JSON exchange, and S3/MinIO synchronization, with backward compatibility for old databases and documents. The worktree was clean before this handoff file was created. The next planned work is F2, starting with a carefully defined due-date model and migration before adding reminder notifications or today view UI.

## Codebase Understanding

## Architecture Overview

- `TodoPanel.svelte` owns panel-level view state: search expansion, search query, completed visibility, drag preview, overlays, theme, and sync status.
- `TodoItem.svelte` owns row-level actions, now including pin/unpin alongside edit, completion, reorder, and delete.
- `todoFilters.ts` is the pure display-filter boundary. Search and completed visibility do not mutate store order.
- `todoStore.ts` owns persisted frontend operations. Pin changes call the API, reorder the local list, and schedule automatic sync.
- `commands.rs` exposes the Rust Todo command boundary. `set_todo_pinned` updates `updated_at` and `updated_by`.
- `db.rs` now has schema version 5. Migration 5 adds `pinned` and updates the active-order index.
- `data_exchange.rs` includes `pinned` in JSON transfer records with `#[serde(default)]`.
- `sync.rs` includes `pinned` in remote records with `#[serde(default)]`, merge tie-breaking, and SQLite upsert.
- Task ordering is consistent across Rust and TypeScript: incomplete before completed, pinned before unpinned within each completion group, then `sort_order`.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `AGENTS.md` | Repository rules and required checks | Read before F2 implementation |
| `FUNCTION_OPTIMIZATION_TODO.md` | Feature roadmap and F1/F2 status | F1 is complete; F2 starts here |
| `src/lib/components/TodoPanel.svelte` | Main panel filtering and orchestration | Future today view and date controls integrate here |
| `src/lib/components/TodoItem.svelte` | Todo row UI | Due-date labels and lightweight date entry may affect this component |
| `src/lib/utils/todoFilters.ts` | Pure local filtering | Extend carefully for today view rather than mixing persistence into UI |
| `src/lib/stores/todoStore.ts` | Frontend Todo mutations and sorting | New date/reminder mutations should follow existing methods |
| `src/lib/types.ts` | Shared frontend Todo shape | F2 fields must be added here |
| `src-tauri/src/db.rs` | Schema and ordered migrations | Current schema is version 5; F2 starts at migration 6 |
| `src-tauri/src/commands.rs` | Todo CRUD and list ordering | Add due/reminder commands and mappings here |
| `src-tauri/src/data_exchange.rs` | Versioned JSON exchange | New synchronized fields need compatible defaults |
| `src-tauri/src/sync.rs` | Remote document and deterministic merge | New task date fields must participate in record sync |
| `src/lib/sync/autoSync.ts` | Automatic sync scheduling | Every persisted F2 task mutation should trigger it |

## Key Patterns Discovered

- Device-local view preferences use local storage. `eggdone-show-completed` is not synchronized.
- Search is intentionally a transient view filter. It never changes persisted task order.
- Search disables drag and keyboard up/down movement because reordering a partial result would be ambiguous.
- A filtered empty result is distinct from an actual empty database.
- Persisted Todo fields must be updated across Rust structs, SQL queries, TypeScript types, JSON exchange, sync documents, tests, and documentation.
- Backward-compatible optional fields use Serde defaults so old JSON and remote documents remain readable.
- New database fields require a forward migration plus fresh-schema, previous-schema upgrade, and idempotence coverage.
- Reorder groups use both `completed` and `pinned`. Dragging never crosses these group boundaries.
- Synced mutations update `updated_at` and `updated_by`, then schedule automatic sync.
- Light theme, dark theme, reduced motion, and the 360px panel width remain required UI constraints.

## Work Completed

## Tasks Finished

- [x] Added collapsible title search.
- [x] Added case-insensitive local filtering with Chinese text support.
- [x] Added Escape behavior: clear query first, close search second.
- [x] Added a persisted show/hide-completed preference.
- [x] Added clear empty states for no search match and hidden completed-only lists.
- [x] Disabled reorder controls while a search query is active.
- [x] Added a low-visual-weight pin/unpin action to each Todo.
- [x] Added SQLite migration 5 and defaulted existing tasks to unpinned.
- [x] Added pin persistence, frontend sorting, and automatic sync scheduling.
- [x] Added pin support to JSON import/export and S3/MinIO synchronization.
- [x] Added compatibility tests for old JSON and old remote documents without `pinned`.
- [x] Updated README and marked all F1 tasks complete.

## Files Modified

All product changes are committed. The current handoff file is the only expected untracked file.

| Commit | Main Files | Changes |
|--------|------------|---------|
| `7f9ab87` | `TodoPanel.svelte`, `TodoItem.svelte`, `todoFilters.ts`, CSS | Search, completed visibility, reorder guard, empty states |
| `c215bed` | `db.rs`, `commands.rs`, `data_exchange.rs`, `sync.rs`, Todo frontend layers | Pin migration, command, UI, sorting, JSON and sync compatibility |

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Search is display-only | Reorder filtered subset, disable reorder, reorder full hidden list | Disabling reorder avoids ambiguous partial-order persistence |
| Completed visibility is local storage | SQLite, synchronized setting, local storage | It is device-local presentation state |
| Completion grouping precedes pinning | All pinned above all tasks, completion first then pinning | Completed tasks should not jump above active work |
| Pin order is four groups | Global pin group, no grouping, completion × pin groups | Preserves clear active/completed separation and predictable drag boundaries |
| Pinning is synchronized | Device-local pin, synchronized pin | Importance is task data and should follow the task across devices |
| Keep sync format version at 1 with default field | Increment immediately, reject old documents, default missing field | Additive optional field remains backward-readable by current code |

## Pending Work

## Immediate Next Steps

1. Start F2 with a data-model design pass before editing UI. Define exact storage for date-only due dates, timed due dates, and reminder timestamps.
2. Add migration 6 with nullable date/reminder fields and tests for fresh databases and version-5 upgrades.
3. Extend Rust/TypeScript Todo shapes, JSON exchange, and sync documents using backward-compatible defaults.
4. Add focused commands/store methods for setting and clearing due information; avoid overloading title editing.
5. Only after persistence and synchronization tests pass, add lightweight date controls and the today/overdue filter.
6. Add system notifications after date semantics and today view are stable.

## Blockers/Open Questions

- [ ] Choose the concrete date-only representation. Recommended: ISO local calendar date such as `YYYY-MM-DD` in a nullable text column.
- [ ] Decide whether an exact due time needs a separate mode field or can be inferred from `due_at` versus `due_date`.
- [ ] Decide default reminder behavior when a user sets only a date. Recommended: no automatic reminder unless explicitly selected.
- [ ] Decide whether “下周” means next Monday or seven days from today. Recommended: next Monday, with explicit UI wording.
- [ ] Notification action support varies by platform; Windows-first behavior needs an in-panel fallback.

## Deferred Items

- Windows signing, automatic updates, full DPI matrix, and macOS/Linux verification remain deferred for personal use.
- F3 grouping waits until F2 date/reminder behavior is stable.
- Recurring tasks and natural-language quick add remain F4 because they depend on the final date model.
- Complex projects, subtasks, tag systems, boards, collaboration, and attachments remain out of scope.

## Context for Resuming Agent

## Important Context

- Read `AGENTS.md`, this handoff, and `FUNCTION_OPTIMIZATION_TODO.md` before coding.
- Branch `main` is clean at commit `c215bed` before this handoff file.
- Do not redo F1. Search, completed visibility, and pinning are implemented and committed.
- Search state lives in `TodoPanel.svelte`; the pure filter is `todoFilters.ts`.
- Search with a non-empty query disables drag handles and move buttons. Preserve this behavior when adding today view or other filters.
- Pin order is: incomplete pinned, incomplete normal, completed pinned, completed normal.
- Drag and move groups match both `completed` and `pinned`; do not regress to completion-only grouping.
- Current database schema is version 5. The next migration number is 6.
- Date-only values must retain local calendar semantics. Do not encode a date-only task as midnight UTC because it can display as the previous or next day in another time zone.
- Exact times should use UTC milliseconds and render in the device’s local time zone.
- Reminder-fired state must be device-local. If one device fires a notification, it must not suppress another device’s notification after sync.
- Synced date/reminder task fields need Serde defaults for old remote and exported documents.
- Every task mutation that affects synchronized data must update `updated_at`, `updated_by`, and trigger automatic sync.
- Never include S3 credentials or secrets in code, tests, logs, exports, or handoffs.

## Assumptions Made

- Windows 10/11 remains the primary platform.
- SQLite remains the offline source of truth.
- S3/MinIO record synchronization remains enabled and must support all new task fields.
- F2 should stay compact and optional; users without dates should see almost the same panel as today.
- Date and reminder features should not block normal add/edit/complete operations.

## Potential Gotchas

- Adding a field only to the frontend or commands layer is insufficient. Trace it through SQLite, migration, all SELECT mappings, JSON exchange, sync records, tests, and README.
- SQL column-order mappings are positional and easy to break when adding fields.
- Browser preview lacks the Tauri bridge. It can validate layout but not SQLite, notifications, tray behavior, or sync.
- `pnpm tauri dev` starts hidden by design; use the tray icon to open the panel.
- Keep `127.0.0.1:1420` aligned between Tauri and Vite.
- Do not hold the SQLite mutex during network requests.
- Background sync refresh must continue using the non-loading refresh path to avoid panel flashing.
- Reminder polling or timers must stop cleanly and must not keep multiple concurrent loops after frontend remounts.
- Laptop sleep/resume can skip timer deadlines; reminders need a catch-up query on startup/resume.
- Handoff scripts require `PYTHONUTF8=1` because Chinese commit messages can fail under Windows GBK decoding.

## Environment State

## Tools/Services Used

- Windows PowerShell
- Node.js 24.16.0
- pnpm 11.3.0
- Rust 1.94.0 stable with MSVC
- Tauri 2.11.2
- Svelte 5, SvelteKit, Vite 6, Vitest 4
- SQLite via bundled `rusqlite`
- S3-compatible synchronization for AWS S3 and MinIO

## Active Processes

- No project dev server, EggDone process, or Cargo build was intentionally left running.

## Environment Variables

- `TAURI_DEV_HOST` is supported by Vite configuration.
- `CARGO_TARGET_DIR` can isolate build output.
- `PYTHONUTF8=1` is required for handoff scripts on this machine.
- No secret environment-variable values are recorded here.

## Verification Status

The latest complete validation passed after pinning:

```text
pnpm release:check
```

Results:

- Svelte check: 0 errors and 0 warnings.
- Frontend tests: 14 passed.
- Frontend production build: passed.
- `cargo fmt -- --check`: passed.
- `cargo check`: passed.
- Rust tests: 40 passed.

The Rust suite includes schema-v5 creation, v4-to-v5 migration, pin CRUD/order, JSON round-trip, old JSON compatibility, sync persistence, and old sync-document compatibility.

## Related Resources

- [Project README](../../README.md)
- [Feature optimization roadmap](../../FUNCTION_OPTIMIZATION_TODO.md)
- [Engineering optimization roadmap](../../OPTIMIZATION_TODO.md)
- [Previous handoff](./2026-06-07-210627-eggdone-sync-release-and-feature-roadmap.md)
- [Todo filter helper](../../src/lib/utils/todoFilters.ts)
- [Database migrations](../../src-tauri/src/db.rs)
- [Todo commands](../../src-tauri/src/commands.rs)
- [Sync document](../../src-tauri/src/sync.rs)

---

**Security check required**: validate this document before using it as the next-session source of truth.
