# Handoff: EggDone Desktop Views And Focus Planning (F6-F8)

## Session Metadata
- Created: 2026-06-16 22:37:19
- Project: D:\Develop\EggDone
- Branch: main
- Session duration: about 1 hour of design and planning; no production source changes

## Recent Commits (for context)
  - a25a932 chore: 升级版本至0.1.1并添加交接文档
  - b6d8d5e feat: 支持任务具体到期时间（分钟级时刻）
  - 11aa6ad docs(README): 添加应用截图并精简吉祥物说明
  - 6333c94 feat: 实现分组横向滚动浏览功能，支持鼠标拖拽、滚轮和箭头按钮
  - a858f99 fix: 修正排序字段，使用 UUID 替代 ID 以确保一致性

## Handoff Chain

- **Continues from**: [2026-06-13-184303-eggdone-desktop-sync-sorting-groups-scroll.md](./2026-06-13-184303-eggdone-desktop-sync-sorting-groups-scroll.md)
  - Previous title: EggDone Desktop Sync Sorting And Group Scrolling
- **Supersedes**: None

> Review the previous handoff for full context before filling this one.

## Current State Summary

This session was design and planning only; no production source files were changed. We defined three desktop feature areas as a continuation of F1-F5: F6 task priority plus an Eisenhower four-quadrant view, F7 a lightweight calendar/agenda view, and F8 a Pomodoro focus timer in a dedicated always-on-top floating window separate from the auto-hiding tray panel. The core product framing was settled: the existing Todo is the single data source; 全部/今天/四象限/日历 are views over the same Todos, while the Pomodoro timer is a tool that optionally points at a Todo and persists nothing in v1. A high-fidelity HTML mockup covering desktop, Harmony phone/tablet, the HarmonyOS Live View (灵动岛) and the focus screen was produced. A standalone plan `VIEWS_AND_FOCUS_TODO.md` was written. Only new untracked artifacts exist and are ready to commit.

## Codebase Understanding

## Architecture Overview

Desktop client is a Svelte 5/SvelteKit panel + Rust/Tauri backend with SQLite. The only planned data-model change is adding a `priority` integer column to `todos` (0=normal, 1=important); quadrant urgency is derived from due dates, not stored. Views are pure-frontend groupings in `todoStore.ts`/`TodoPanel.svelte`. The focus timer must NOT reuse the auto-hiding main panel; it will be a separate Tauri window (always-on-top, skip-taskbar) plus tray integration. Sync merge stays whole-record by `updated_at`, so `priority` carries along with no new merge rules.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `VIEWS_AND_FOCUS_TODO.md` | New standalone F6-F8 plan | Authoritative scope for this work |
| `mockups/eggdone-features-mockup.html` | High-fidelity UI mockup (both platforms) | Visual reference approved by user |
| `src-tauri/src/db.rs` | SQLite schema and migrations | Where the `priority` migration goes |
| `src-tauri/src/commands.rs` | Todo CRUD commands | Read/write `priority`, bump `updated_at`/`updated_by` |
| `src-tauri/src/sync.rs` | Sync document and UUID merge | Serialize `priority`, no new merge rule |
| `src-tauri/src/data_exchange.rs` | JSON import/export | Include `priority` |
| `src/lib/stores/todoStore.ts` | Frontend Todo state and sorting | Quadrant/agenda grouping, `priority` field |
| `src/lib/components/TodoPanel.svelte` | Main panel UI | View switcher, 2x2 overview, agenda buckets |

## Key Patterns Discovered

- Single source of truth: 全部/今天/四象限/日历 are lenses over one Todo set, not separate lists.
- Pomodoro is a tool, not a view; it lives in its own window/lifecycle.
- New fields must be forward-compatible: unknown ignored, missing defaults to 0.
- The tray panel is narrow; the quadrant view uses a 2x2 count overview plus stacked sections, not a full grid.
- Do not break existing manual drag order; `priority` is a view dimension and optional sort only.

## Work Completed

## Tasks Finished

- [x] Read both projects' latest handoffs to establish context.
- [x] Settled feature scope; rejected full calendar and time-tracking scope creep.
- [x] Settled IA: one main page with a view switcher plus an independent Pomodoro (entry option A).
- [x] Defined data-model impact: only `priority` added; calendar reuses `due_date`; Pomodoro not persisted.
- [x] Produced a high-fidelity HTML mockup for both platforms including the Live View.
- [x] Wrote the standalone desktop plan `VIEWS_AND_FOCUS_TODO.md` (F6-F8).
- [x] Created this handoff.

## Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| `VIEWS_AND_FOCUS_TODO.md` | New file (untracked) | Standalone F6-F8 plan |
| `mockups/eggdone-features-mockup.html` | New file (untracked) | UI mockup preview |

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Add only a `priority` flag (0/1), derive urgency from due date | per-task urgent+important fields; multi-level priority | Lightest model change; quadrant needs only the importance axis |
| Pomodoro in a separate floating window | inside tray panel; bottom-nav tab | Tray panel auto-hides on blur; the timer must persist independently |
| Fuse views into one page with a switcher | four parallel pages | Same data; avoids duplicate lists and keeps filters shared |
| Pomodoro v1 not persisted | persist sessions/stats | Avoids excluded time-tracking scope; stays lightweight |
| Calendar = agenda + week strip, no full month grid | full desktop month view | Tray panel too narrow; full calendar is already out of scope |

## Pending Work

## Immediate Next Steps

1. Commit the new artifacts: `VIEWS_AND_FOCUS_TODO.md`, `mockups/`, and this handoff.
2. When implementing, start with F6 data model: add the `priority` migration in `db.rs`, then `commands.rs`, `sync.rs`, `data_exchange.rs`, `types.ts`, `todoStore.ts`; verify old DB upgrade.
3. Then F6 UI (important marker + 四象限 view), then F7 agenda/week strip, then F8 floating timer + tray integration.
4. Keep `priority` semantics identical to the Harmony client for cross-device sync.

## Blockers/Open Questions

- [ ] None blocking; implementation has not started, this is planning only.
- [ ] Decide whether `priority` stays binary or expands to multi-level before UI ships widely.

## Deferred Items

- Cross-cell drag inside the quadrant view (use a menu toggle first).
- Desktop full month calendar (out of scope).
- Pomodoro stats/history (explicitly excluded).

## Context for Resuming Agent

## Important Context

This session changed NO production source code. The repo is clean except for new untracked files: `VIEWS_AND_FOCUS_TODO.md` and `mockups/`. The earlier group-scroll work referenced by the previous handoff is already committed (`6333c94`). The single most important design constraint: 全部/今天/四象限/日历 are views over one Todo dataset, while the Pomodoro timer is a separate tool/window and must not be built into the auto-hiding panel. The only schema change for all three features is a `priority` column, which must stay identical to the Harmony client. A static preview server for the mockup was started on port 8123.

## Assumptions Made

- Binary `priority` (an important flag) is sufficient for v1.
- Urgent = overdue or due today, matching the existing 今天 view.
- A separate Pomodoro window is acceptable on desktop despite the earlier tray-conflict concern.

## Potential Gotchas

- Do not place the Pomodoro timer in the main panel; it hides on blur.
- `create_handoff.py` needs `$env:PYTHONUTF8='1'` on this machine for Chinese git subjects.
- A static HTTP server may still be running on port 8123 (started this session for the mockup).
- `.claude/handoffs/` is tracked and committed in this repo.

## Environment State

## Tools/Services Used

- pnpm/Tauri toolchain (not run this session; planning only).
- Python static HTTP server on port 8123 for the mockup preview.
- session-handoff skill scripts under `C:\Users\caozhipeng\.agents\skills\session-handoff`.
- PowerShell in `D:\Develop\EggDone`.

## Active Processes

- Static HTTP server on port 8123 serving `mockups/` (may still be running; stop it when done).

## Environment Variables

- `PYTHONUTF8` (set temporarily for the handoff script only).
- No secret values were read or recorded.

## Related Resources

- `VIEWS_AND_FOCUS_TODO.md`
- `mockups/eggdone-features-mockup.html`
- `FUNCTION_OPTIMIZATION_TODO.md` (F1-F5 predecessor)
- Harmony companion plan: `D:\Develop\EggDoneHarmony\HARMONY_VIEWS_AND_FOCUS_TODO.md`
- Previous handoff: `2026-06-13-184303-eggdone-desktop-sync-sorting-groups-scroll.md`

---

**Security Reminder**: Before finalizing, run `validate_handoff.py` to check for accidental secret exposure.
