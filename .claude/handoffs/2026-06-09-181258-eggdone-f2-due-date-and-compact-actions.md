# Handoff: EggDone F2 Due Date and Compact Actions

## Session Metadata
- Created: 2026-06-09 18:12:58
- Project: D:\Develop\EggDone
- Branch: main
- Session duration: about 1 focused development session

### Recent Commits (for context)
  - 06c8436 feat: 将任务操作整合为弹出菜单
  - e94af18 feat: 为任务添加到期日期和提醒功能
  - a76671c docs: 添加F1完成和F2下一步的交接文档
  - c215bed feat: 添加任务置顶功能
  - 7f9ab87 feat: 实现任务搜索和已完成任务隐藏功能

## Handoff Chain

- **Continues from**: [2026-06-07-213101-eggdone-f1-complete-and-f2-next.md](./2026-06-07-213101-eggdone-f1-complete-and-f2-next.md)
  - Previous title: EggDone F1 Complete and F2 Next
- **Supersedes**: no earlier handoff is deleted, but this is now the latest continuation after F2 date model and compact row actions.

Review the previous handoff for F1 search/show-completed/pinning context and the planned F2 sequence.

## Current State Summary

EggDone has moved into F2. The data model and UI now support date-only due dates on todos, with storage prepared for exact due timestamps and reminder timestamps. JSON import/export and S3/MinIO sync include the new schedule fields. The latest UI adjustment reduces row crowding by replacing the five right-side row action buttons with a single "more" menu; visible metadata is kept as compact badges below the title. The working tree is clean except for this new handoff document.

## Codebase Understanding

### Architecture Overview

The project follows the existing split from AGENTS.md. Frontend Tauri calls stay in `src/lib/api/`, state changes and auto-sync triggers stay in `src/lib/stores/`, reusable Svelte UI stays in `src/lib/components/`, SQLite schema and migration logic stays in `src-tauri/src/db.rs`, frontend commands stay in `src-tauri/src/commands.rs`, and sync/import/export data shapes live in `src-tauri/src/sync.rs` and `src-tauri/src/data_exchange.rs`.

F2 due-date support is deliberately split into two layers:

- Date-only UI is implemented now through `due_date`, using local calendar semantics as `YYYY-MM-DD`.
- Exact timestamp and reminder support are schema-ready through `due_at` and `reminder_at`, but no exact-time picker or system notification runtime is implemented yet.

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| [src-tauri/src/db.rs](../../src-tauri/src/db.rs) | SQLite migrations and schema version | Schema is now version 6 with `due_date`, `due_at`, `reminder_at`, and `reminder_deliveries`. |
| [src-tauri/src/commands.rs](../../src-tauri/src/commands.rs) | Tauri todo commands | Adds `set_todo_schedule`; validates date-only strings and maps schedule fields into Todo responses. |
| [src-tauri/src/sync.rs](../../src-tauri/src/sync.rs) | Versioned sync document and merge rules | Sync document includes schedule fields with serde defaults for old remote files. |
| [src-tauri/src/data_exchange.rs](../../src-tauri/src/data_exchange.rs) | JSON import/export and backup helpers | Export/import includes schedule fields and validates invalid dates/timestamps. |
| [src/lib/api/todoApi.ts](../../src/lib/api/todoApi.ts) | Frontend todo command wrapper | Exposes `setSchedule`. |
| [src/lib/stores/todoStore.ts](../../src/lib/stores/todoStore.ts) | Todo state and business operations | `setSchedule` updates store and schedules auto-sync. |
| [src/lib/components/TodoItem.svelte](../../src/lib/components/TodoItem.svelte) | Row UI | Contains date popover, due/pin badges, and compact action menu. |
| [src/lib/components/TodoPanel.svelte](../../src/lib/components/TodoPanel.svelte) | Panel composition | Passes `scheduleTodo` into each row. |
| [src/app.css](../../src/app.css) | App styling | Contains compact row action menu, date badges, popover, light/dark styles. |
| [FUNCTION_OPTIMIZATION_TODO.md](../../FUNCTION_OPTIMIZATION_TODO.md) | Feature roadmap | F2 data model and date setting items are checked off; system reminders and today view remain open. |

### Key Patterns Discovered

- Any Todo field added to SQLite must also be propagated through commands, TS types, store tests, JSON import/export, and sync. Missing any one of those can cause data loss through import or cloud sync.
- `serde(default)` is used for newly added fields in sync/export structs so old JSON and old remote `todos.json` files can still be read.
- Store methods call `scheduleAutoSync` after successful local mutations. Date changes follow the same pattern as edit, pin, complete, reorder, delete, and restore.
- Date-only values must not be converted to midnight UTC. `due_date` is a local calendar string, separate from `due_at`.
- Row actions should keep low visual weight. The current approach uses a single more button plus metadata badges for already-active states.

## Work Completed

### Tasks Finished

- [x] Added SQLite migration v6 for todo schedule fields.
- [x] Added local-only `reminder_deliveries` table for future notification de-duplication.
- [x] Added `set_todo_schedule` command and frontend `todoApi.setSchedule`.
- [x] Added `TodoScheduleInput` and TypeScript schedule fields on `Todo`.
- [x] Added store support and tests for schedule updates.
- [x] Extended JSON import/export with schedule fields and backward compatibility.
- [x] Extended S3/MinIO sync document and merge persistence with schedule fields and backward compatibility.
- [x] Added date-only UI entry with Today, Tomorrow, Next Week, Custom, and Clear.
- [x] Added due-date badges and overdue/today visual treatment.
- [x] Reworked row actions into a compact overflow menu to free title space.
- [x] Updated README and functional roadmap for completed F2 subitems.

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| [FUNCTION_OPTIMIZATION_TODO.md](../../FUNCTION_OPTIMIZATION_TODO.md) | Checked off F2 data model and date-setting items | Keep roadmap current. |
| [README.md](../../README.md) | Documented due-date feature, schema fields, and current limitations | User-visible behavior changed. |
| [src-tauri/src/db.rs](../../src-tauri/src/db.rs) | Schema v6, due/reminder columns, reminder deliveries, migration tests | Prepare reliable data foundation. |
| [src-tauri/src/commands.rs](../../src-tauri/src/commands.rs) | Todo response fields, `set_todo_schedule`, date validation, tests | Expose schedule edits to frontend. |
| [src-tauri/src/data_exchange.rs](../../src-tauri/src/data_exchange.rs) | Import/export schedule fields, validation, tests | Preserve schedule data in JSON exchange. |
| [src-tauri/src/sync.rs](../../src-tauri/src/sync.rs) | Sync schedule fields, persistence, validation, tests | Preserve schedule data across S3/MinIO sync. |
| [src-tauri/src/lib.rs](../../src-tauri/src/lib.rs) | Registered `set_todo_schedule` command | Make backend command callable. |
| [src/lib/types.ts](../../src/lib/types.ts) | Added schedule fields to `Todo` | Keep frontend typing accurate. |
| [src/lib/api/todoApi.ts](../../src/lib/api/todoApi.ts) | Added `TodoScheduleInput` and `setSchedule` | Tauri command wrapper. |
| [src/lib/stores/todoStore.ts](../../src/lib/stores/todoStore.ts) | Added `setSchedule` mutation and auto-sync trigger | State update and sync consistency. |
| [src/lib/stores/todoStore.test.ts](../../src/lib/stores/todoStore.test.ts) | Added schedule fields and schedule test | Prevent type and behavior regressions. |
| [src/lib/utils/todoFilters.test.ts](../../src/lib/utils/todoFilters.test.ts) | Added schedule defaults in test factories | Keep tests compiling with new Todo shape. |
| [src/lib/components/TodoPanel.svelte](../../src/lib/components/TodoPanel.svelte) | Passes `scheduleTodo` to row component | Connect UI to store. |
| [src/lib/components/TodoItem.svelte](../../src/lib/components/TodoItem.svelte) | Date popover, badges, compact action menu | Date UI and row-space optimization. |
| [src/app.css](../../src/app.css) | Styles for date UI, badges, compact action menu, dark theme | Keep UI compact and readable. |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Store date-only due dates as `YYYY-MM-DD` in `due_date` | Store as midnight UTC timestamp, local midnight timestamp, or string | String preserves local calendar semantics and avoids timezone drift. |
| Add `due_at` and `reminder_at` now without exact-time UI | Delay schema fields until notification work, or add all scheduling fields now | Sync/import schema changes are expensive; adding fields now gives F2 a stable data contract. |
| Keep `reminder_deliveries` local-only | Sync notification-fired records, or keep per-device | A reminder fired on one device should not suppress another device. |
| Use serde defaults for new sync/export fields | Bump format version and reject old clients/files, or default missing fields | Backward compatibility matters for personal sync and exported backups. |
| Replace row action buttons with a single overflow menu | Keep hidden buttons, move actions to context menu only, or overflow menu | Hidden buttons still consumed layout width; context-menu-only is undiscoverable. Overflow menu restores title width while keeping operations accessible. |
| Show active state through badges | Keep active date/pin icons in the right action area | Badges communicate state without consuming the fixed right action width. |

## Pending Work

## Immediate Next Steps

1. Implement "全部 / 今天" compact view toggle in the panel.
2. Define a shared frontend date helper for today/overdue calculations so `TodoItem` and the upcoming today filter use the same local-date semantics.
3. After today view is stable, add tray menu entry/tooltip count for today and overdue tasks.

## Blockers/Open Questions

- [ ] Decide whether the "今天" view should include completed tasks when "显示已完成" is on, or always show only incomplete due/overdue tasks. Current roadmap says today view contains today's due and overdue incomplete tasks.
- [ ] Decide whether exact time UI belongs in the date popover before system notifications, or should wait until notification support starts.
- [ ] The compact action menu should be manually checked in `pnpm tauri dev` for click/blur interactions inside the frameless tray panel.

## Deferred Items

- System notification plugin/runtime: deferred until date model and today view are stable.
- Snooze actions such as "稍后 10 分钟" and "今天晚些时候": requires notification runtime and local reminder state.
- Exact-time picker for `due_at`: schema is ready, but UI was kept date-only for simplicity.
- Grouping, repeat rules, and quick-add natural language parsing: planned for later F3/F4 after F2.

## Context for Resuming Agent

## Important Context

The latest functional state is represented by commits `e94af18` and `06c8436`. The only untracked file at handoff creation is this handoff document. Do not reimplement date schema or compact actions from scratch; continue from the committed state. The next planned product step is today view, not system notifications. Today view should reuse local date-only semantics: `due_date` is a local calendar date string and must not be interpreted as UTC midnight.

## Assumptions Made

- User prefers a compact personal tray todo UI over exposing every action inline.
- Date-only due dates are more useful than exact reminders as the first F2 slice.
- S3/MinIO sync remains enabled as a first-class constraint, so all new Todo fields must sync.
- Windows remains the primary validation platform.

## Potential Gotchas

- `due_date` and `due_at` are mutually exclusive in backend validation. Keep that invariant unless the product model changes.
- There are three copies of date validation in Rust (`commands.rs`, `data_exchange.rs`, `sync.rs`). This duplication is intentional for now but could be extracted later if it grows.
- `reminder_deliveries` must not be included in sync/export documents.
- The action menu and schedule popover both live inside `TodoItem.svelte`; blur/focus behavior in the Tauri panel should be manually tested because the app hides on external focus loss.
- Git may print warnings about inaccessible `C:\Users\caozhipeng\.config\git\ignore`; this has not blocked status/diff/log commands.

## Environment State

### Tools/Services Used

- PowerShell shell in `D:\Develop\EggDone`.
- `pnpm check`, `pnpm build`, `pnpm test -- --run`.
- `cargo fmt -- --check`, `cargo check`, `cargo test`.
- `session-handoff` skill scripts from `C:\Users\caozhipeng\.agents\skills\session-handoff\scripts`.

### Active Processes

- No dev server or Tauri process was intentionally left running by this handoff.

### Environment Variables

- `PYTHONUTF8` was set for handoff script execution to avoid Windows GBK decode errors. No secrets were used or recorded.

## Related Resources

- [FUNCTION_OPTIMIZATION_TODO.md](../../FUNCTION_OPTIMIZATION_TODO.md)
- [README.md](../../README.md)
- [AGENTS.md](../../AGENTS.md)
- [Previous handoff](./2026-06-07-213101-eggdone-f1-complete-and-f2-next.md)

---

Security note: validation should pass with no TODO placeholders and no secrets.
