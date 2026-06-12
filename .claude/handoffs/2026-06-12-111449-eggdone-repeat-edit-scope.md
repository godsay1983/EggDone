# Handoff: EggDone Repeat Edit Scope

## Session Metadata
- Created: 2026-06-12 11:14:49
- Project: D:\Develop\EggDone
- Branch: main
- Session duration: about 1 hour

## Handoff Chain
- Continues from: [.claude/handoffs/2026-06-12-023206-eggdone-function-optimization.md](./2026-06-12-023206-eggdone-function-optimization.md)
- Supersedes: none

## Recent Commits
- b2c02ed docs: 添加EggDone功能优化交接文档
- 83acc56 feat: 添加归档已完成任务功能
- 7582da3 feat: 增加托盘菜单预览今日任务和同步去重优化
- 0649e0e feat: 实现任务批量操作：完成、移动分组和删除
- 917bebd style: 提升弹出菜单和操作面板的层级

## Current State Summary
The current session implemented the remaining F4 repeat-task requirement: editing a repeating task can now apply either to only the current instance or to future instances in the same series. The work is complete and validated, but not committed. The working tree has nine modified project files plus this new handoff file. The next agent should review the diff, optionally run a quick manual Tauri smoke test, then commit the repeat edit scope change.

## Architecture Overview
EggDone remains a Tauri 2 desktop tray app with Svelte/TypeScript on the frontend and SQLite/Rust commands on the backend. Frontend API wrappers in `src/lib/api/` call Tauri commands only. Store state and business updates live in `src/lib/stores/`. Reusable UI is under `src/lib/components/`. Backend command logic is centralized in `src-tauri/src/commands.rs`; SQLite schema and migrations stay in `src-tauri/src/db.rs`; tray behavior stays in `src-tauri/src/tray.rs`.

This change extends existing edit commands rather than adding a parallel command set. Rust commands now return a `TodoEdit { updated_todos: Vec<Todo> }` for title, note, schedule, and group edits. The Svelte store replaces all returned todos by ID, so the frontend can handle one or many changed instances through the same path.

## Critical Files
| File | Purpose | Relevance |
|------|---------|-----------|
| `src-tauri/src/commands.rs` | Tauri todo commands and backend command tests | Added `TodoEdit`, `RepeatEditScope`, repeat future selection, batch edit SQL updates, and Rust tests. |
| `src/lib/api/todoApi.ts` | Frontend Tauri command wrapper | Added `RepeatEditScope` parameters and `TodoEditResult` return shape. |
| `src/lib/stores/todoStore.ts` | Frontend todo state and business operations | Replaces all updated todos from edit results and passes edit scope through store methods. |
| `src/lib/components/TodoItem.svelte` | Per-task UI and action popovers | Prompts users to choose current vs future edits for repeated tasks. |
| `src/lib/components/TodoPanel.svelte` | Panel-level callbacks and orchestration | Passes repeat edit scope from `TodoItem` to the store. |
| `src/lib/types.ts` | Shared frontend types | Added `RepeatEditScope = "single" | "future"`. |
| `src/lib/stores/todoStore.test.ts` | Frontend store tests | Updated mocks for edit result shape and added multi-update store coverage. |
| `README.md` | User-facing project documentation | Documents repeat edit scope support and removes the prior limitation. |
| `FUNCTION_OPTIMIZATION_TODO.md` | Functional roadmap | Marks repeat edit scope as complete. |

## Key Patterns Discovered
- Repeat deletion already used explicit scope strings; edit scope now follows the same pattern but with `"single"` and `"future"`.
- Existing frontend store tests use a mock `typeof todoApi`; when API return shapes change, mock functions must be updated or `pnpm check` will fail.
- Store update code should replace returned todos by ID instead of assuming a single changed item.
- Rust command tests use in-memory SQLite and should verify both normal behavior and sync-safe fields.
- User-facing behavior should remain simple; current UI uses `window.confirm` for scope choice to avoid adding another heavy custom dialog.

## Tasks Finished
- Added backend `TodoEdit` result type for commands that can update multiple todos.
- Added `RepeatEditScope` parsing and validation on the Rust side.
- Implemented future repeat edit selection for title, note, schedule/repeat rule, and group movement.
- Defined future scope as same repeat series, active/not archived, incomplete, and due date not earlier than the edited task. The edited target is always included.
- Updated frontend API, store, and component callback signatures to pass scope.
- Added a repeat-task UI prompt: confirm means apply to future tasks; cancel means only this task.
- Updated store tests for multi-update edit results.
- Added Rust test coverage for future repeat editing.
- Updated README and the functional roadmap.
- Ran full validation successfully.

## Files Modified
| File | Changes | Rationale |
|------|---------|-----------|
| `src-tauri/src/commands.rs` | Added `TodoEdit`, `RepeatEditScope`, `repeat_edit_ids`, `find_todos_by_ids`; changed title/note/schedule/group commands to return batch edit results; updated tests. | Enables current vs future repeat edits while preserving one code path for single and multi updates. |
| `src/lib/api/todoApi.ts` | Added `TodoEditResult`; edit API methods accept `repeatScope` and return `updated_todos`. | Keeps command payload and return shape typed. |
| `src/lib/stores/todoStore.ts` | Store methods accept `RepeatEditScope`; added `replaceUpdatedTodos`; batch move explicitly uses single scope. | Updates all returned instances after a future edit. |
| `src/lib/components/TodoItem.svelte` | Added scope prompt and callback typing for edit, note, schedule, and group movement. | Lets the user choose current vs future for repeated task edits. |
| `src/lib/components/TodoPanel.svelte` | Imports `RepeatEditScope` and forwards it to store methods. | Connects item-level UI choice to backend calls. |
| `src/lib/types.ts` | Added `RepeatEditScope`. | Shared frontend type for API/store/component boundaries. |
| `src/lib/stores/todoStore.test.ts` | Updated edit mocks to return `updated_todos`; added multi-update test. | Covers frontend state replacement for repeat future edits. |
| `README.md` | Documents repeat edit scope and updates limitations. | Keeps user docs accurate. |
| `FUNCTION_OPTIMIZATION_TODO.md` | Marks repeat edit scope complete. | Keeps roadmap current. |

## Decisions Made
| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Return `updated_todos` from edit commands | Keep single `Todo`, add separate series edit commands, or return batch result | Batch result is the smallest consistent shape that supports both single and future edits without duplicating command APIs. |
| Use `"future"` scope only for active future instances | Update entire series including completed history, update only generated next item, or active future instances | Avoids rewriting history while still matching user expectation for future repeated tasks. |
| Include the edited target even if it is completed | Exclude completed tasks from future edit entirely or always include target | The user explicitly edited that item, so the visible target should change even if historical completed items are otherwise protected. |
| Use native `window.confirm` for scope choice | Custom modal, inline segmented control, or confirm dialog | It is fast and low-risk for this iteration. A custom dialog can be added later if the interaction feels too rough. |
| Leave snooze as single-instance behavior | Apply snooze to future reminders or only current item | Snooze semantically means “this reminder later,” so it should not propagate through the repeat series. |

## Immediate Next Steps
1. Review the uncommitted diff and commit it, likely as `feat: 支持重复任务编辑作用范围`.
2. Optionally run `pnpm tauri dev` and manually verify a repeating task: create repeat, complete once, edit title/date/group with both prompt choices, confirm only expected instances change.
3. Decide whether to improve the scope prompt from `window.confirm` to a small EggDone-styled popover/dialog; this is polish, not required for correctness.

## Blockers/Open Questions
- No hard blocker.
- Notification click handling remains open because the current Tauri notification path previously lacked a reliable callback design in this project.
- The current scope prompt is functional but visually plain. If user complains about the native confirm dialog, replace it with a compact in-app choice dialog.

## Deferred Items
- System notification click should open the panel and locate the task. Deferred until the notification plugin/callback path is designed.
- Notification action buttons for “稍后 10 分钟” and “今天晚些时候” remain deferred for platform support reasons.
- Archive browser/restore UI is a possible enhancement but was not part of the current functional roadmap item.
- Installer, signing, automatic update, and broader cross-platform release validation remain tracked separately in `OPTIMIZATION_TODO.md`.

## Important Context
The latest code changes are not committed. The user asked to generate a handoff immediately after the repeat edit scope implementation. The current validated behavior is: title, note, schedule/repeat rule, and group edits can pass `repeatScope` as `"single"` or `"future"`. `"future"` selects todos in the same repeat series where `deleted_at IS NULL`, `archived_at IS NULL`, `completed = 0`, and `due_date >= target.due_date`, plus the edited target itself. This means completed historical tasks are not batch-modified, but if the user edits a completed target, that target is still updated.

The frontend currently asks scope with `window.confirm`. Confirm is “后续任务”; cancel is “仅此任务”. This is intentionally minimal. If continuing development, do not add a heavy UI framework. Keep the tray app lightweight and use existing Svelte/CSS patterns.

## Assumptions Made
- Repeating tasks are date-level only; `due_at` remains incompatible with `repeat_rule`.
- Future repeat edits should not rewrite already completed historical instances.
- S3/MinIO sync and JSON import/export do not need new schema fields for this feature because only existing todo fields are edited.
- Batch task operations should continue to apply to selected concrete todos only, not implicit future repeat series.

## Potential Gotchas
- `src-tauri/src/commands.rs` now has edit functions requiring `&mut Connection` because they wrap updates in transactions. Tests and future helper calls must pass mutable connections.
- `TodoEdit` is serialized with snake_case field `updated_todos`; frontend API expects that exact key.
- Tauri command parameter naming maps Rust `repeat_scope` to frontend `repeatScope`.
- Do not grow Rust tuple comparisons in sync conflict code beyond tuple `Ord` limits; use chained `.then_with(...)` if more tie-breaks are needed.
- Running git commands may print warnings about `C:\Users\caozhipeng\.config\git\ignore` permission; this has been seen before and did not block work.
- The working copy has LF/CRLF warnings. They are existing Windows line-ending behavior and did not affect validation.

## Validation Completed
- `pnpm check`: passed
- `pnpm test -- --run`: passed, 40 tests
- `pnpm build`: passed
- `cd src-tauri && cargo fmt -- --check`: passed
- `cd src-tauri && cargo check`: passed
- `cd src-tauri && cargo test`: passed, 60 tests

Manual tray validation with `pnpm tauri dev` was not run in this session.

## Environment State
- OS shell used: PowerShell
- Project path: `D:\Develop\EggDone`
- Current branch: `main`
- Active background processes started by this session: none
- Relevant environment variable names used: `PYTHONUTF8`
- Package manager: `pnpm`
- Rust tooling: `cargo`

## Current Git State
Modified files at handoff creation:
- `FUNCTION_OPTIMIZATION_TODO.md`
- `README.md`
- `src-tauri/src/commands.rs`
- `src/lib/api/todoApi.ts`
- `src/lib/components/TodoItem.svelte`
- `src/lib/components/TodoPanel.svelte`
- `src/lib/stores/todoStore.test.ts`
- `src/lib/stores/todoStore.ts`
- `src/lib/types.ts`
- `.claude/handoffs/2026-06-12-111449-eggdone-repeat-edit-scope.md`

No files were staged or committed during this handoff creation.

## Related Resources
- [.claude/handoffs/2026-06-12-023206-eggdone-function-optimization.md](./2026-06-12-023206-eggdone-function-optimization.md)
- [FUNCTION_OPTIMIZATION_TODO.md](../../FUNCTION_OPTIMIZATION_TODO.md)
- [README.md](../../README.md)
- [src-tauri/src/commands.rs](../../src-tauri/src/commands.rs)
- [src/lib/components/TodoItem.svelte](../../src/lib/components/TodoItem.svelte)
- [src/lib/stores/todoStore.ts](../../src/lib/stores/todoStore.ts)
