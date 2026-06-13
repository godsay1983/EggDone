# Handoff: EggDone Desktop Sync Sorting And Group Scrolling

## Session Metadata
- Created: 2026-06-13 18:43:03
- Project: `D:\Develop\EggDone`
- Branch: `main`
- Session duration: about 3 hours of desktop companion work

### Recent Commits
- `a858f99 fix: 修正排序字段，使用 UUID 替代 ID 以确保一致性`
- `76d26ae feat: 新增前台自动同步轮询和ETag检查`
- `9c3d3bf fix: 统一背景遮罩层的z-index为40`
- `b6c990a docs: 添加EggDone通知点击和操作交接文档`
- `ede889e feat: 实现Windows通知点击定位和按钮操作`

## Handoff Chain
- **Continues from**: [2026-06-12-124808-eggdone-notification-actions.md](./2026-06-12-124808-eggdone-notification-actions.md)
- **Supersedes**: None

## Current State Summary

Desktop automatic sync and cross-client sorting compatibility are committed. The current uncommitted work improves the horizontal group filter when many groups exceed the panel width: mouse drag scrolling, wheel-to-horizontal scrolling, overflow-only left/right controls, and automatic reveal of the selected group. An initial implementation captured the pointer on mouse-down and broke normal group clicks; this was corrected so pointer capture starts only after movement exceeds 5 pixels. Type checking, 41 frontend tests and production build pass. The user should manually smoke test the real Tauri app with many groups before these two files are committed.

## Codebase Understanding

## Architecture Overview

The desktop client uses Svelte 5/SvelteKit for the panel and Rust/Tauri for persistence, tray behavior, reminders and synchronization. `TodoPanel.svelte` is the main UI composition layer. `todoStore.ts` maintains sorted frontend state and calls Tauri commands. Rust `commands.rs` owns authoritative SQLite reads/writes. Auto-sync is coordinated in `src/lib/sync/autoSync.ts` and Rust S3 logic. The group filter is a horizontally scrolling flex row inside the narrow tray panel.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `src/lib/components/TodoPanel.svelte` | Main panel UI and interaction state | Contains uncommitted group overflow/drag behavior |
| `src/app.css` | Global panel and dark-theme styling | Contains uncommitted group scroll controls and cursors |
| `src/lib/stores/todoStore.ts` | Frontend Todo state and stable sorting | Committed cross-client ordering tie-break |
| `src-tauri/src/commands.rs` | SQLite Todo commands and authoritative list order | Committed UUID tie-break |
| `src-tauri/src/data_exchange.rs` | Import/export ordering | Committed UUID tie-break |
| `src/lib/sync/autoSync.ts` | Foreground and interval synchronization | Committed 60-second ETag behavior |
| `README.md` | Feature and synchronization behavior | Current architecture/reference summary |

## Key Patterns Discovered

- The tray panel has limited horizontal space; overflow controls must remain compact and not create a second row.
- Group chips still serve as Todo drop targets. Horizontal browsing must not remove `data-group-drop-target`.
- Normal button clicks and drag-to-scroll share pointer events. Pointer capture must not happen until drag intent is established.
- A 5-pixel movement threshold separates click from horizontal drag.
- Overflow state derives from `scrollWidth - clientWidth`; controls should not appear when everything fits.
- The selected group should call `scrollIntoView({ inline: "nearest" })` after selection or group-list mutation.

## Work Completed

## Tasks Finished

- [x] Added desktop startup/foreground synchronization and 60-second lightweight ETag polling.
- [x] Prevented concurrent automatic sync tasks and kept local-change debounce behavior.
- [x] Unified desktop and Harmony task ordering tie-break with synchronized UUID data.
- [x] Added horizontal mouse-drag browsing for overflowing group chips.
- [x] Converted wheel input over the group row to horizontal scrolling.
- [x] Added overflow-only previous/next group controls with disabled edge states.
- [x] Added automatic reveal of the selected group.
- [x] Added dark-theme styles for the new controls.
- [x] Fixed the click regression by delaying pointer capture until actual drag movement.
- [x] Re-ran frontend checks, tests and production build after the click fix.

## Files Modified

The following files are intentionally uncommitted:

| File | Changes | Rationale |
|------|---------|-----------|
| `src/lib/components/TodoPanel.svelte` | Overflow observers, scroll state, wheel handling, pointer drag threshold, previous/next controls and selected-group reveal | Make large group sets browsable without breaking filtering |
| `src/app.css` | Scroll behavior, drag cursor, compact controls and dark-theme styles | Provide discoverable, restrained desktop interaction |

The ordering and auto-sync changes are already committed in `a858f99` and `76d26ae`.

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Keep one horizontal chip row | Wrap to multiple rows; dropdown-only; horizontal browsing | Preserves compact tray-panel height and existing direct filtering |
| Support drag, wheel and buttons | Only scrollbar; only buttons; all three | Mouse users need discoverability and fast navigation |
| Delay pointer capture until 5px movement | Capture on mouse-down; no capture; threshold capture | Preserves normal chip clicks while keeping drag stable outside the row |
| Show arrows only during overflow | Always show; hidden scrollbar only; overflow-only | Avoids wasting space for small group sets |
| Keep current group reorder controls | Add chip drag-reorder in this change; retain manage up/down | This work addresses browsing, not group order editing |

## Pending Work

## Immediate Next Steps

1. Run `pnpm tauri dev`, open the real panel with enough groups to overflow, and verify `全部`, `未分组`, and every group chip still filter and show Todo content.
2. Verify horizontal browsing by mouse drag, vertical wheel, touchpad horizontal gesture and both arrow controls.
3. Verify dragging a Todo onto a group chip still moves the Todo and does not accidentally scroll the group row.
4. Verify light/dark themes and narrow panel layout, then commit `src/lib/components/TodoPanel.svelte` and `src/app.css`.
5. If clicking still fails in Tauri only, inspect event order before changing the Todo filtering/store code; the likely area is pointer/click suppression.

## Blockers/Open Questions

- [ ] Real Tauri manual validation with many actual groups has not been completed after the pointer-capture fix.
- [ ] Browser-only preview cannot load Tauri data because `invoke` is unavailable, so it cannot fully validate the Todo list response to real group data.
- [ ] Decide later whether group reordering itself should gain direct chip drag-and-drop; current manage-panel up/down controls remain functional.

## Deferred Items

- Direct drag-reordering of group chips is deferred to a separate interaction change.
- Persisting group-row scroll position across application restarts is unnecessary for now.
- Cross-platform notification action buttons remain Windows-first as recorded in the previous handoff.

## Context for Resuming Agent

## Important Context

Start with `git status --short`. `src/lib/components/TodoPanel.svelte` and `src/app.css` are modified and must not be discarded. The user reported that group chips stopped responding and Todo content disappeared after the first drag implementation. The root cause was immediate `setPointerCapture` in `pointerdown`; the current code captures only in `pointermove` after crossing the 5-pixel threshold. Do not reintroduce capture on mouse-down. The latest automated checks pass, but the correct next action is real Tauri manual testing, then a focused commit.

## Assumptions Made

- The group filter should remain a single compact row.
- Mouse vertical-wheel input over the row may be consumed for horizontal navigation when overflow exists.
- Existing manage-panel up/down group ordering is sufficient for now.
- The user's missing Todo content was a filtering interaction regression, not data loss.

## Potential Gotchas

- `suppressGroupClick` is reset asynchronously after a real drag so the synthetic click following pointer-up is ignored.
- `pointerleave` clears a pending non-drag pointer operation; do not clear an active captured drag there.
- `ResizeObserver` and `MutationObserver` are disconnected in `onMount` cleanup.
- `data-group-filter` is for reveal lookup; `data-group-drop-target` must remain for Todo drag-to-group behavior.
- Browser preview displays `Cannot read properties of undefined (reading 'invoke')` because it lacks the Tauri runtime. This is expected and not caused by the group work.
- The temporary local Vite server used for browser verification was stopped.
- `create_handoff.py` needs `$env:PYTHONUTF8='1'` on this machine for Chinese Git subjects.

## Environment State

## Tools/Services Used

- pnpm for Svelte check, Vitest and production build
- Cargo/Rust tests for committed sorting/sync changes
- In-app browser for a limited non-Tauri click smoke test
- PowerShell in `D:\Develop\EggDone`

## Validation Completed

- `pnpm check`: 0 errors, 0 warnings
- `pnpm test`: 8 files and 41 tests passed
- `pnpm build`: passed
- Earlier cross-client ordering validation: 62 Rust tests passed
- Browser smoke test before the final pointer fix confirmed normal group selection logic; real Tauri regression remains required

## Active Processes

- The temporary Vite dev server on port `4173` was stopped.
- No known long-running process is required.

## Environment Variables

- `PYTHONUTF8` for handoff script execution only
- No secret values were read or recorded

## Related Resources

- [Project README](../../README.md)
- [Manual regression checklist](../../docs/MANUAL_REGRESSION.md)
- [Windows release guide](../../docs/RELEASING_WINDOWS.md)
- Previous handoff: [2026-06-12-124808-eggdone-notification-actions.md](./2026-06-12-124808-eggdone-notification-actions.md)
- Harmony companion project: `D:\Develop\EggDoneHarmony`
