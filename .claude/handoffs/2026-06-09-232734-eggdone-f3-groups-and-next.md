# Handoff: EggDone F3 Groups Complete And F4 Next

## Session Metadata

- Created: 2026-06-09 23:27:34
- Project: D:\Develop\EggDone
- Branch: main
- Session duration: about 1 day across several continuation turns

### Recent Commits For Context

- a836fff style: 重构分组管理器布局，使用网格和弹性布局，并添加暗色主题支持
- e79113b feat: 实现分组管理（重命名、改色、删除、排序）
- 320ff7b feat: 实现单层分组功能，支持新建、筛选和移动任务到分组
- a64836c feat: 提取提醒时间工具函数并添加稍后提醒功能
- 964e827 feat: 支持为到期任务设置自定义提醒时间

## Handoff Chain

- **Continues from**: [2026-06-09-181258-eggdone-f2-due-date-and-compact-actions.md](./2026-06-09-181258-eggdone-f2-due-date-and-compact-actions.md)
  - Previous title: EggDone F2 Due Date and Compact Actions
- **Supersedes**: None. This handoff extends the chain after F2 and captures F3 group work.

## Current State Summary

EggDone has completed the main F3 single-layer group work. The app now supports group creation, filtering, new tasks inheriting the selected group, moving tasks between groups, group rename, group color presets, group order changes, and soft deletion of groups while preserving todos by moving them to ungrouped. The latest visible UI issue, where the group manager controls were too wide for the 360px panel and the "下移" button could disappear, was fixed by changing the manager to a compact two-row layout with name/save on the first row and color/actions on the second row. The working tree currently only contains this new handoff file.

## Immediate Next Steps

1. Review `FUNCTION_OPTIMIZATION_TODO.md` and decide whether to implement the remaining F3 optional item, "拖动任务到分组", or move directly to F4.
2. If moving to F4, design repeat task storage first: repeat rule, next occurrence, completion behavior, SQLite migration, sync/import/export fields, and tests.
3. If implementing drag-to-group, keep it narrow: drag from todo row to group chip or a small drop target, and preserve the existing action-menu move as the reliable fallback.

## Important Context

Do not recreate the app or restart from MVP. The Tauri/Svelte project is already functional and has tray behavior, SQLite persistence, S3/MinIO sync, JSON import/export, theme switching, search, today view, reminders, snooze actions, completed hiding, pinned tasks, drag reorder, single-instance behavior, startup settings, group filtering, and group management. The current source work is committed through recent commits; the only uncommitted file should be this handoff. Before continuing, run `git status --short`, read `AGENTS.md`, and keep changes small enough for the 360px panel.

## Codebase Understanding

### Architecture Overview

EggDone is a Tauri 2 tray-first desktop Todo app with Svelte and TypeScript on the frontend and Rust plus SQLite on the backend. Frontend components call typed API wrappers in `src/lib/api/`, app state lives in `src/lib/stores/`, reusable UI lives in `src/lib/components/`, and Rust command/database/tray responsibilities are split across `src-tauri/src/commands.rs`, `src-tauri/src/db.rs`, and `src-tauri/src/tray.rs`. Sync uses a versioned document model in Rust and supports S3/MinIO style object storage. SQLite migrations are centralized and must preserve old databases.

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `FUNCTION_OPTIMIZATION_TODO.md` | Functional roadmap | F1, F2, and most F3 items are done. F3 only leaves optional drag-to-group; F4 repeat tasks and quick add syntax are next. |
| `README.md` | User-facing project docs | Updated whenever visible behavior changes. Currently documents group preset colors and limitations. |
| `src/lib/components/TodoPanel.svelte` | Main tray panel UI | Contains group filter row, group manager, search, today view, drag reorder, and footer sync status. |
| `src/lib/components/TodoItem.svelte` | Individual todo row UI | Contains edit, schedule/reminder, pin, group move selector, actions menu, and drag handle. |
| `src/lib/stores/todoStore.ts` | Frontend todo/group state | Coordinates optimistic UI updates, API calls, sorting, refresh, and auto-sync scheduling. |
| `src/lib/api/todoApi.ts` | Tauri command wrapper | Maps frontend operations to Rust command names. Must stay in sync with `lib.rs` generate handler. |
| `src/lib/types.ts` | Shared frontend data shapes | `Todo` and `TodoGroup` fields must match Rust serialized structs. |
| `src/app.css` | Full panel styling | Includes light/dark theme, compact controls, group color preset tokens, and narrow panel layout fixes. |
| `src-tauri/src/commands.rs` | Tauri commands and database operations | Implements todo/group CRUD, sorting, validation, tests, and badge refresh after writes. |
| `src-tauri/src/db.rs` | SQLite schema and migrations | Groups were introduced in migration v8 with `groups` table and `todos.group_uuid`. |
| `src-tauri/src/sync.rs` | S3 sync merge document | Groups and todo group membership participate in sync. |
| `src-tauri/src/data_exchange.rs` | JSON import/export and backup | Groups are included in import/export; import validates UUIDs and group references. |

### Key Patterns Discovered

- Keep frontend layers separated: components call store methods, stores call `todoApi`, and only `todoApi` invokes Tauri commands.
- Backend write commands update SQLite, return the fresh row, and call `refresh_badge_after_success` so tray badge stays current.
- SQLite rows are soft-deleted with `deleted_at` for sync compatibility.
- Sort order uses spaced integer values, usually multiples of 1024.
- Sync conflict handling prefers latest `updated_at`, then deterministic tie-breakers such as deletion and device id.
- UI changes must account for a narrow 360px panel. Avoid long single-row toolbars.
- User-visible behavior changes should update README and the corresponding TODO plan.
- Windows is the main validation target, but code should not hardcode user paths.

## Work Completed

### Tasks Finished

- [x] Implemented base single-layer groups with `groups` table and `todos.group_uuid`.
- [x] Added group filter row: 全部, 未分组, and each user group.
- [x] New tasks inherit the currently selected concrete group.
- [x] Todo action menu supports moving a task to another group or ungrouped.
- [x] JSON import/export and S3/MinIO sync include groups and group membership.
- [x] Added group rename, reorder, delete, and color commands in Rust.
- [x] Added frontend API/store methods for group rename, reorder, delete, and color updates.
- [x] Deleting a group preserves todos by setting their `group_uuid` to `NULL`.
- [x] Added six fixed group color presets: yellow, green, blue, peach, lavender, gray.
- [x] Added group color chips and dark theme support.
- [x] Fixed cramped group manager layout by using a two-row card layout.
- [x] Updated README and FUNCTION_OPTIMIZATION_TODO.
- [x] Added/updated frontend store tests and Rust command tests.

### Files Modified In Recent Work

The current working tree has no uncommitted source changes besides this handoff. Recent group work changed these files:

| File | Changes | Rationale |
|------|---------|-----------|
| `FUNCTION_OPTIMIZATION_TODO.md` | Marked F3 group management and preset colors complete | Keep roadmap aligned with implemented behavior. |
| `README.md` | Documented group filtering, management, and preset colors | User-visible feature documentation. |
| `src-tauri/src/commands.rs` | Added group update/delete/reorder/color commands and tests | Backend persistence and validation for group management. |
| `src-tauri/src/lib.rs` | Registered new Tauri commands | Expose backend commands to frontend. |
| `src/lib/api/todoApi.ts` | Added group management API wrappers | Keep frontend command calls centralized. |
| `src/lib/stores/todoStore.ts` | Added group rename/color/delete/reorder store operations | Manage state updates and auto-sync scheduling. |
| `src/lib/stores/todoStore.test.ts` | Added group management test coverage | Guard state behavior for group updates. |
| `src/lib/components/TodoPanel.svelte` | Added group manager UI and compact two-row layout structure | Allow group operations within the small panel. |
| `src/app.css` | Added group color presets, dark theme variants, and compact manager layout | Make the controls visible and usable at 360px. |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Use single-layer groups only | Multi-level groups, tags, projects | Keeps app lightweight and avoids becoming a full project manager. |
| Use fixed group color presets | Free color picker, arbitrary hex strings, no colors | Presets are simpler, themeable, sync-safe, and fit the small UI. |
| Soft-delete groups | Hard-delete groups | Needed for sync conflict resolution and import/export consistency. |
| Preserve todos when deleting a group | Delete grouped todos, block deletion, move to ungrouped | Preserving data is safest for personal todo usage. |
| Compact two-row group manager | One horizontal toolbar, separate modal, right-click-only menu | Two-row layout keeps all controls visible inside the tray panel without extra window complexity. |
| Arrow buttons for group order | Text buttons 上移/下移 | Arrow buttons save width; `title` and `aria-label` preserve meaning. |

## Pending Work

### Immediate Next Steps

1. Decide whether to implement the remaining F3 optional item, "拖动任务到分组". It is useful but not required for basic grouping because the row action menu already moves tasks.
2. If skipping drag-to-group, start F4 repeat tasks with a small schema-first design: repeat rule, next occurrence semantics, completion behavior, sync/import/export format, and migration tests.
3. After repeat tasks, implement quick-add parsing for simple Chinese date/time words such as 今天, 明天, 周五, and 明天 10:00, without adding a large NLP dependency.

### Blockers/Open Questions

- [ ] Notification click-to-open behavior is still pending from F2. Need confirm whether it matters before F4, because platform notification APIs can add complexity.
- [ ] Repeat task behavior needs product decisions before code: whether completing overdue repeat tasks creates only one next instance, and how monthly repeats handle missing dates like the 31st.
- [ ] Dragging tasks into groups needs a UX decision: drag to group chip, drop zone, or keep only action-menu move for simplicity.

### Deferred Items

- Drag task to group: deferred because action-menu move already covers the core need and avoids complex drag/drop interactions in a small panel.
- Notification click positioning: deferred because reminders already fire; click routing can be platform-specific.
- Notification action buttons: deferred because platform support differs and can complicate MVP behavior.
- F5 enhancements such as notes, batch operations, configurable default view, keyboard navigation, and archiving remain optional.

## Context For Resuming Agent

### Important Context

Do not restart the project or recreate the Tauri scaffold. The app is already functional and has many features beyond MVP: tray behavior, SQLite persistence, S3/MinIO sync, JSON import/export, theme switching, search, today view, reminders, snooze actions, completed hiding, pinned tasks, drag reorder, single-instance behavior, startup settings, group filtering, and group management. The current code is committed through recent commits, and the only uncommitted file should be this handoff. Before continuing, run `git status --short` and read `FUNCTION_OPTIMIZATION_TODO.md`. Keep changes narrow and update README whenever user-visible behavior changes.

### Assumptions Made

- The app remains for personal use; avoid account systems, team features, heavy UI frameworks, and complex project-management concepts.
- SQLite remains the source of truth, with S3/MinIO sync exchanging versioned JSON documents.
- F3 is effectively complete for normal usage even though drag-to-group is unchecked.
- The user prefers practical incremental improvements over large architectural rewrites.
- Windows is the primary validation platform.

### Potential Gotchas

- Tauri command names in `src/lib/api/todoApi.ts` must match registrations in `src-tauri/src/lib.rs`.
- Rust structs are serialized with snake_case field names; frontend TypeScript interfaces use the same names.
- Any new database field needs migration, JSON import/export support, sync support, tests for new and old databases, and README/plan updates.
- Search and today view disable drag reorder intentionally to avoid reordering a partial filtered list.
- The panel is small. Avoid adding horizontal control rows that exceed 360px.
- `groups.color` is intentionally constrained to preset strings. Do not accept arbitrary colors unless the sync/import validation story is updated.
- Deleting a group must not delete todos.
- The reminder delivery table is local-only and should not participate in sync.
- On Windows, helper scripts may need `$env:PYTHONUTF8='1'` to avoid GBK decode failures when reading git output.

## Environment State

### Tools/Services Used

- PowerShell in `D:\Develop\EggDone`.
- `pnpm` for frontend checks, tests, dev, and build.
- Cargo for Rust format/check/test.
- Tauri 2 with Svelte/TypeScript.
- SQLite via Rust backend.
- S3/MinIO sync code exists but no credentials are stored in this handoff.
- `session-handoff` skill script was run with `PYTHONUTF8=1`.

### Active Processes

- No long-running dev server or Tauri process was left running by this handoff creation.

### Environment Variables

- `PYTHONUTF8` was set temporarily while running the handoff scaffold script.
- No secret values were inspected or recorded.

## Validation Performed Recently

After the group management and color work:

- `pnpm check` passed.
- `pnpm test -- --run` passed: 6 test files, 26 tests.
- `pnpm build` passed.
- `cargo fmt -- --check` passed after formatting.
- `cargo check` passed.
- `cargo test` passed: 49 tests.

After the compact group manager layout fix:

- `pnpm check` passed.
- `pnpm build` passed.

## Related Resources

- [Project README](../../README.md)
- [Function optimization plan](../../FUNCTION_OPTIMIZATION_TODO.md)
- [Engineering optimization plan](../../OPTIMIZATION_TODO.md)
- [Project agent instructions](../../AGENTS.md)
- [Previous handoff](./2026-06-09-181258-eggdone-f2-due-date-and-compact-actions.md)

---

**Security Reminder**: This handoff intentionally contains no credentials, API keys, access keys, secret keys, passwords, tokens, or local database contents.
