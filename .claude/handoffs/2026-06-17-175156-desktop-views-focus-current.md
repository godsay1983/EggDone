# Handoff: EggDone Desktop Views And Focus Current

## Session Metadata
- Created: 2026-06-17 17:51:56
- Project: D:\Develop\EggDone
- Branch: main
- Session duration: Multi-session F6-F8 continuation; this handoff captures the current clean state after the latest views/focus work.

### Recent Commits (for context)
  - 04985d5 feat: 实现前台专注MVP，支持番茄钟基本操作
  - e367def docs: 更新桌面端月历决策为已完成
  - 8d107f3 feat: 实现日历视图周条导航与日期选择功能
  - 973df4b feat: 添加日历视图与日程分桶功能
  - 81d91ec feat: 快捷新增支持开头 `!` 标记重要

## Handoff Chain

- **Continues from**: [2026-06-16-223719-views-and-focus-planning.md](./2026-06-16-223719-views-and-focus-planning.md)
  - Previous title: EggDone Desktop Views And Focus Planning (F6-F8)
- **Supersedes**: None

Review the previous handoff for the original F6-F8 planning context before resuming feature work.

## Current State Summary

EggDone desktop is on `main` with a clean working tree before this handoff file was generated. F6 priority/four-quadrant, F7 agenda/calendar with week strip and date navigation, and the F8 foreground focus MVP are implemented. The desktop focus timer was recently fixed so the countdown updates live without closing/reopening the panel. The next significant work is no longer main-panel polish; it is the planned independent Tauri floating focus window with tray integration and system notification behavior.

## Codebase Understanding

### Architecture Overview

The desktop app is a Tauri + Svelte app. Rust commands, migrations, JSON exchange, and sync live under `src-tauri/src/`; Svelte UI and stores live under `src/`. Todo remains the only data model for views: quadrants, today, and calendar are projections over Todo fields. `priority` is stored and synced as a Todo field compatible with the Harmony client. The current focus MVP is UI-local and intentionally does not persist or sync focus sessions.

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `VIEWS_AND_FOCUS_TODO.md` | Tracks F6-F8 requirements and completion state | Source of truth for desktop views/focus plan |
| `FUNCTION_OPTIMIZATION_TODO.md` | Earlier F1-F5 completed feature plan | Confirms base features already completed before F6-F8 |
| `src/lib/components/TodoPanel.svelte` | Main tray panel UI and current focus overlay | Contains current view switcher, quadrant/calendar/focus UI |
| `src/lib/stores/todoStore.ts` | Frontend Todo state and derived view data | Calendar/quadrant filtering and priority defaults belong here |
| `src/lib/types.ts` | Shared frontend types | Contains Todo `priority` type |
| `src-tauri/src/db.rs` | SQLite migrations | Contains `priority` migration and schema compatibility |
| `src-tauri/src/commands.rs` | Tauri command surface | Reads/writes Todo fields including priority |
| `src-tauri/src/data_exchange.rs` | JSON import/export | Must include priority for compatibility |
| `src-tauri/src/sync.rs` | S3/MinIO sync document and merge | Must stay compatible with Harmony sync semantics |

### Key Patterns Discovered

- Keep views as derived UI state over the same Todo list; do not duplicate task data for quadrants or calendar.
- `priority` uses `0 = 普通`, `1 = 重要`, matching Harmony.
- The desktop main panel should stay lightweight and keep its tray/blur-hide model. Focus should move to an independent Tauri window for persistent visibility.
- For live countdown UI in Svelte, use explicit reactive display values derived from focus state/time so the template updates every tick.

## Work Completed

### Tasks Finished

- [x] Added `priority` to SQLite, Rust types/commands, JSON import/export, sync, frontend types, and store defaults.
- [x] Added important marker/actions and quick-add parsing for leading `!` / `！`.
- [x] Added four-quadrant view with colored quadrant presentation and empty states.
- [x] Added agenda/calendar view with buckets, week strip, selected-date behavior, previous/next week controls, and today jump.
- [x] Added foreground focus MVP with start/pause, +5 minutes, skip, and end actions.
- [x] Fixed desktop focus countdown so it updates live in the panel.
- [x] Verified desktop checks/tests in the recent work stream.

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| `VIEWS_AND_FOCUS_TODO.md` | F6/F7 completed; F8 foreground MVP completed and floating-window work still pending | Keeps the next-session plan aligned with implemented behavior |
| `src/lib/components/TodoPanel.svelte` | Main UI changes for quadrants, calendar, and focus MVP; latest timer display refresh fix | Central desktop UI surface |
| `src/lib/stores/todoStore.ts` | Priority defaults and derived view support | Keeps filtering/view logic in frontend store |
| `src/lib/types.ts` | Todo priority typing | Frontend compatibility |
| `src-tauri/src/db.rs` | SQLite migration for priority | Existing user databases upgrade safely |
| `src-tauri/src/commands.rs` | Todo priority command handling | UI can set/cancel important |
| `src-tauri/src/data_exchange.rs` | JSON import/export priority compatibility | Cross-version data exchange |
| `src-tauri/src/sync.rs` | Sync serialization/merge compatibility | Desktop/mobile sync consistency |

Note: `git status --short --branch` was clean before this handoff file was created.

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Implement focus as foreground MVP first | MVP in panel, full independent window immediately | Lets the user validate core timing/actions before adding Tauri window lifecycle complexity |
| Move future focus to independent Tauri window | Keep expanding main panel, separate focus window | Main panel auto-hide behavior conflicts with long-running visible focus timing |
| Use derived urgency | Store urgency, derive from due date | Avoids extra sync state and matches Harmony semantics |
| Do not sync/persist focus sessions in v1 | Store sessions, keep local transient state | Focus is a tool, not a Todo data feature in this phase |

## Immediate Next Steps

1. Create the independent Tauri focus window for F8: borderless, always-on-top, skip taskbar, and separate from the main tray panel lifecycle.
2. Move or share the current focus state/actions so the floating window and tray menu can control the same session.
3. Add collapsed/expanded floating window states and draggable positioning.
4. Add tray tooltip/menu integration for focus status and actions.
5. Add phase-end system notification behavior and validate sleep/resume timestamp correction.

## Pending Work

### Blockers/Open Questions

- [ ] Need final UX decision on the floating window collapsed size/position and whether it should remember its last position.
- [ ] Need decide whether "focus complete -> mark linked Todo complete" belongs in the first floating-window release or a later option.
- [ ] Need confirm target platforms beyond Windows before implementing platform-specific notification action buttons.

### Deferred Items

- Focus history, reports, statistics, and points are out of scope.
- Full desktop month calendar remains intentionally out of scope; current plan uses agenda + week strip + jump date.
- Complex quadrant drag-and-drop remains deferred; existing important toggle and due date controls cover the primary workflows.

## Important Context

The next desktop focus step should not continue packing more controls into `TodoPanel.svelte`. The planned direction is an independent Tauri floating focus window because the main panel is meant to hide on blur and stay lightweight. Keep desktop and Harmony data semantics aligned: `priority` must remain a Todo field in SQLite/JSON/sync, and focus sessions should remain local transient state unless the user explicitly changes that product decision. The desktop timer bug has already been addressed by explicit reactive display values; avoid regressing to a static display derived only during open/close.

## Context for Resuming Agent

### Assumptions Made

- The user wants desktop and mobile feature parity at the data/semantic level, with platform-specific UI where appropriate.
- Windows is the primary desktop target for the next focus-window work.
- Current repository commits through `04985d5` represent the implemented F6/F7/F8 foreground MVP baseline.

### Potential Gotchas

- The desktop panel has a tray/blur-hide interaction model; focus must not depend on that panel staying visible.
- Tauri multi-window work will likely require changes outside `TodoPanel.svelte`, including Rust/window setup and possibly app lifecycle wiring.
- `git status` was clean before generating this handoff, but the handoff file itself is now a new uncommitted file.
- Keep four version surfaces in sync only when doing release bumps: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and the `eggdone` entry in `src-tauri/Cargo.lock`.

## Environment State

### Tools/Services Used

- Desktop verification in the recent work stream used `pnpm check` and `pnpm test -- --run`.
- Session handoff generated with `C:\Users\caozhipeng\.agents\skills\session-handoff\scripts\create_handoff.py`.

### Active Processes

- No dev server or watcher is known to be running from this handoff.

### Environment Variables

- `PYTHONUTF8`

## Related Resources

- `VIEWS_AND_FOCUS_TODO.md`
- `FUNCTION_OPTIMIZATION_TODO.md`
- `.claude/handoffs/2026-06-16-223719-views-and-focus-planning.md`
- `src/lib/components/TodoPanel.svelte`
- `src/lib/stores/todoStore.ts`
- `src-tauri/src/sync.rs`

---

**Security Reminder**: This handoff intentionally omits credentials and secret values.
