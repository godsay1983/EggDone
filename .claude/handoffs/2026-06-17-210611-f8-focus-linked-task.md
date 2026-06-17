# Handoff: EggDone Desktop F8 Focus Linked Task

## Session Metadata
- Created: 2026-06-17 21:06:11
- Project: D:\Develop\EggDone
- Branch: main
- Session duration: ~2 hours across the final focus iterations

## Recent Commits
- 2b4abfd feat: 支持专注模式关联并显示Todo标题
- 705f525 feat: 添加专注阶段结束时的系统通知功能
- 1fd6164 feat: 实现可配置的专注与休息时长
- 36ecb4a feat: 新增独立专注窗口，支持番茄钟计时与悬浮窗交互
- ca76e37 docs: 添加桌面视图与专注当前状态交接文档

## Handoff Chain

- Continues from: [2026-06-17-175156-desktop-views-focus-current.md](./2026-06-17-175156-desktop-views-focus-current.md)
- Supersedes: 2026-06-17-175156-desktop-views-focus-current.md for F8 focus implementation status.

## Current State Summary

Desktop F8 focus work has advanced from a basic independent Tauri focus window to a usable linked-task Pomodoro flow. The app now supports configurable focus/rest durations, phase-end system notifications, and starting focus from an individual Todo. The latest implementation is committed on `main`; the working tree is clean except for this handoff document.

## Codebase Understanding

## Architecture Overview

The desktop app is a Svelte + Tauri project. The main tray panel lives in `src/lib/components/TodoPanel.svelte`; the independent focus window lives in `src/lib/components/FocusWindow.svelte` and is configured as the hidden `focus` window in Tauri. Focus state remains intentionally local and temporary: it does not write to SQLite, does not enter JSON export, and does not participate in S3/MinIO sync. Cross-window focus preferences and temporary focus target state are shared through `localStorage` helpers in `src/lib/utils/focusSettings.ts`.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `VIEWS_AND_FOCUS_TODO.md` | Desktop F6-F8 plan | Tracks completed and remaining F8 scope. |
| `src/lib/components/TodoPanel.svelte` | Main tray Todo panel | Owns fallback focus overlay, header focus button, and task menu entry for “专注做这件事”. |
| `src/lib/components/FocusWindow.svelte` | Independent focus window UI | Displays phase, timer, controls, and linked Todo title. |
| `src/lib/components/TodoItem.svelte` | Task row and action menu | Adds the task-level “专注做这件事” menu item. |
| `src/lib/utils/focusSettings.ts` | Focus preferences and temporary target helpers | Stores duration settings and temporary focus target in localStorage. |
| `src-tauri/src/reminders.rs` | Reminder and notification backend | Sends phase-end system notifications for focus. |
| `src-tauri/src/commands.rs` | Tauri command layer | Exposes `publish_focus_notification`, `open_focus_window`, and `hide_focus_window`. |
| `src-tauri/src/lib.rs` | Tauri app setup | Registers commands and configures focus window close behavior. |
| `src-tauri/tauri.conf.json` | Tauri window config | Defines hidden always-on-top `focus` window. |
| `src-tauri/capabilities/default.json` | Tauri permissions | Includes `core:window:allow-start-dragging`, required for dragging the focus window. |

## Key Patterns Discovered

- Focus is deliberately not persisted as business data. Keep it out of `todos`, migrations, sync documents, and JSON export.
- Desktop independent focus window communicates with the main window through shared browser storage and Tauri commands, not through backend session state.
- Timer correctness is based on `Date.now()` plus `focusEndsAt`, not interval accumulation.
- Header/global focus opens an unlinked Pomodoro; task menu focus writes a temporary `FocusTarget` before opening the focus surface.
- The fallback overlay still exists for non-Tauri/browser mode and should stay behaviorally aligned with `FocusWindow.svelte`.

## Work Completed

## Tasks Finished

- [x] Added configurable focus/rest durations in settings.
- [x] Added phase-end system notification through Tauri backend.
- [x] Added task-level “专注做这件事” action.
- [x] Displayed linked Todo title in both independent focus window and fallback overlay.
- [x] Updated `VIEWS_AND_FOCUS_TODO.md` to mark completed F8 items.

## Files Modified By Recent Focus Work

| File | Changes | Rationale |
|------|---------|-----------|
| `src/lib/utils/focusSettings.ts` | Added duration options, storage helpers, focus target storage, and change events. | Keeps focus preferences and target temporary and frontend-local. |
| `src/lib/components/FocusWindow.svelte` | Reads duration/target state, shows linked Todo title, clears target on end. | Makes the independent window reflect task-level focus. |
| `src/lib/components/TodoPanel.svelte` | Adds unlinked vs linked focus entry paths, fallback target display, task-level handler. | Keeps global focus and task focus behavior distinct. |
| `src/lib/components/TodoItem.svelte` | Adds `onFocus` prop and menu item. | Allows users to start focus directly from a task. |
| `src/app.css` | Adds compact focus target styling for light/dark themes. | Prevents linked task display from overwhelming the timer UI. |
| `src-tauri/src/reminders.rs` | Adds focus phase notification delivery. | Reuses existing system notification path without changing Todo reminders. |
| `src-tauri/src/commands.rs` and `src-tauri/src/lib.rs` | Adds and registers `publish_focus_notification`. | Allows frontend focus timer to request a native notification. |
| `src/lib/api/todoApi.ts` and `src/lib/stores/todoStore.test.ts` | Adds API wrapper and test mock for focus notification. | Keeps TypeScript contract complete. |

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Keep focus target temporary and localStorage-backed | DB column, store-only variable, localStorage | DB/sync would overcomplicate v1; store-only would not reach the independent Tauri window. |
| Clear focus target when ending a session | Keep last target, clear immediately, ask user | Clearing avoids stale “正在专注” labels after the session is done. |
| Send phase-end notification from Rust command | Tauri JS notification plugin directly, Rust command | Existing native reminder path and Windows toast dependency are already in Rust. |
| Do not implement “mark Todo complete on focus end” yet | Auto-complete, prompt, leave pending | Completion is a data write and needs explicit UX; current plan marks it as separate deferred item. |

## Pending Work

## Immediate Next Steps

1. Decide whether to implement `专注结束 -> 标记关联 Todo 完成` and design the confirmation UX.
2. Add tray integration: tooltip `专注 mm:ss · 任务名` and tray menu controls for start/pause/end.
3. Add collapsed/capsule mode for the independent focus window if the user still wants a smaller always-on-top presence.

## Blockers/Open Questions

- [ ] The plan item “将前台 MVP 迁移为独立 Tauri 悬浮窗” is partially done but still listed open because collapsed mode and full tray lifecycle controls are not complete.
- [ ] Notification buttons like “开始休息 / 再来一颗” are not implemented yet; current notification is informational only.

## Deferred Items

- Auto-completing the linked Todo after focus completion, deferred because it changes task data and needs user confirmation behavior.
- Focus statistics/history, deferred by plan as out of scope.
- Cross-device focus session sync, deferred by product principle: focus v1 is local-only.

## Context for Resuming Agent

## Important Context

The current desktop code already includes the latest focus work in committed HEAD `2b4abfd`. If continuing work, do not re-add the just-completed features. Start from the remaining unchecked F8 items in `VIEWS_AND_FOCUS_TODO.md`: completion action, tray tooltip/menu controls, and collapsed focus window mode. The focus target is stored under `eggdone-focus-target-uuid` and `eggdone-focus-target-title`; if you change this storage scheme, update both `TodoPanel.svelte` and `FocusWindow.svelte`.

## Assumptions Made

- Linked focus target is a temporary pointer only and should not survive an ended session.
- The independent focus window should remain separate from the main tray panel and should not hide when the main panel loses focus.
- Phase-end notification does not need action buttons for the current milestone.

## Potential Gotchas

- `FocusWindow.svelte` is a separate Tauri webview; Svelte component state from `TodoPanel.svelte` is not shared.
- Dragging the focus window depends on `core:window:allow-start-dragging`; removing that permission breaks drag even if cursor/UI suggests draggable behavior.
- `pnpm check` catches missing `todoApi` mock methods in tests because `todoStore.test.ts` types its mock as `typeof todoApi`.
- Do not use `git reset` or revert unrelated changes; user may have work in both repos.

## Environment State

## Tools/Services Used

- `pnpm check` passed.
- `pnpm test -- --run` passed: 8 test files, 42 tests.
- `cargo check` passed in `src-tauri`.
- `cargo fmt` was used in the previous notification step.

## Active Processes

- No dev server or long-running process was intentionally left running.

## Environment Variables

- None required for desktop checks in this session.

## Related Resources

- Desktop plan: `VIEWS_AND_FOCUS_TODO.md`
- Previous handoff: `.claude/handoffs/2026-06-17-175156-desktop-views-focus-current.md`
- Companion Harmony handoff: `D:\Develop\EggDoneHarmony\.claude\handoffs\2026-06-17-210611-h13-focus-linked-task.md`

---

Security check note: no credentials or secret values are intentionally included.
