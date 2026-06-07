# Handoff: EggDone P2 Desktop Integration

## Session Metadata
- Created: 2026-06-07 10:18:12
- Project: `D:\Develop\EggDone`
- Branch: `main`
- Session duration: approximately 12 hours across continued optimization sessions

### Recent Commits

- `33d3c8c` feat: 添加全局快捷键和开机启动配置支持
- `8ad65f3` refactor: 重构拖拽预览排序逻辑，提取公共函数
- `b898dfb` fix: 修复拖拽目标判断逻辑并改进编辑体验
- `e768470` feat: 添加 JSON 导入导出、UUID 合并和 SQLite 手动备份功能
- `0399e2b` feat: 优化拖拽排序交互，添加预览动画和样式调整

## Handoff Chain

- **Continues from**: [2026-06-06-222909-eggdone-mvp-and-optimization-roadmap.md](./2026-06-06-222909-eggdone-mvp-and-optimization-roadmap.md)
- **Supersedes**: None

The previous handoff describes the original MVP and roadmap. This document records the completed P0/P1 work and the current P2 desktop-integration state.

## Current State Summary

EggDone is now beyond the MVP. P0 data foundations and P1 Todo interaction work are complete: the database has ordered migrations and sync-ready fields, the app is single-instance, tasks can be edited and reordered, deletes are soft with undo, completed tasks can be cleared, and data can be exported, imported, and backed up. P2 now includes a configurable global shortcut and optional system autostart. The repository is clean on `main`, all current work is committed, automated checks pass, and the next planned work is the remaining tray/window section of P2: multi-monitor placement, high-DPI correctness, work-area clamping, and focus race hardening.

## Codebase Understanding

## Architecture Overview

- The frontend remains a SvelteKit static application.
- `TodoPanel.svelte` coordinates panel-level UI state, drag sessions, overlays, theme state, and Tauri events.
- `TodoItem.svelte` owns row interactions, including inline editing and pointer-based drag initiation.
- `todoStore.ts` is the business-state boundary for Todo operations.
- `todoApi.ts`, `dataApi.ts`, and `desktopSettings.ts` isolate Tauri command/plugin calls.
- `reorderPreview.ts` contains the pure drag-preview algorithm and has focused unit tests.
- Rust commands in `commands.rs` implement Todo CRUD, editing, sorting, soft delete, restore, and clear-completed.
- `db.rs` owns SQLite opening, WAL configuration, ordered migrations, and migration tests.
- `data_exchange.rs` owns versioned JSON import/export and consistent SQLite backup.
- `tray.rs` owns tray menus, click behavior, panel placement, and blur timing state.
- `lib.rs` assembles plugins, managed state, single-instance behavior, window events, and invoke handlers.
- SQLite remains the offline source of truth. Future S3/MinIO synchronization must merge records rather than upload the database file.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `AGENTS.md` | Repository development and verification rules | Read before all changes |
| `OPTIMIZATION_TODO.md` | P0-P4 roadmap and completion state | Remaining work starts at P2 tray/window tasks |
| `src/lib/components/TodoPanel.svelte` | Main panel orchestration | Integrates drag, overlays, desktop settings, and Tauri events |
| `src/lib/components/TodoItem.svelte` | Todo row and inline editing | Contains pointer/edit interaction details |
| `src/lib/components/SettingsPanel.svelte` | Global shortcut and autostart UI | New P2 settings surface |
| `src/lib/api/desktopSettings.ts` | Tauri global-shortcut/autostart plugin wrapper | Registers and rolls back shortcut changes |
| `src/lib/utils/reorderPreview.ts` | Drag preview ordering algorithm | Preserve adjacent-swap behavior when changing drag UX |
| `src/lib/stores/todoStore.ts` | Frontend Todo state and operations | Business operations should continue to flow through this store |
| `src-tauri/src/db.rs` | SQLite connection and migrations | Current schema is sync-ready |
| `src-tauri/src/commands.rs` | Todo commands and shortcut panel command | Rust frontend command boundary |
| `src-tauri/src/data_exchange.rs` | JSON exchange and database backup | Foundation for future sync serialization |
| `src-tauri/src/tray.rs` | Tray, visibility, placement, blur state | Primary target for the next P2 work |
| `src-tauri/src/lib.rs` | Tauri plugin and lifecycle assembly | Single-instance plugin must stay first |
| `src-tauri/capabilities/default.json` | Plugin permissions | Includes shortcut and autostart permissions |

## Key Patterns Discovered

- Keep API transport, frontend state, reusable UI, database, commands, and tray behavior in their existing modules.
- Use bound SQL parameters and return understandable errors instead of panicking on business paths.
- Database changes require ordered forward migrations and both new/legacy database tests.
- Drag preview uses adjacent row-center crossings. It intentionally swaps one row at a time so reversing direction passes through intermediate rows instead of jumping.
- The visual drag highlight follows the dragged Todo UUID after reordering, not the original array index.
- Inline edits save on Enter and outside click, cancel on Escape, and preserve input on backend failure.
- Native file dialogs temporarily suppress blur hiding through `PanelState`.
- Desktop settings are device-local. Shortcut preferences use local storage; autostart state is read from the operating system.
- The global shortcut calls a Rust command so panel toggle behavior stays centralized in `tray.rs`.
- The single-instance plugin must be the first registered Tauri plugin.
- All new UI must support both light and dark themes and reduced-motion preferences.

## Work Completed

## Tasks Finished

- [x] Added ordered SQLite migrations and `schema_migrations`.
- [x] Added UUID, completion time, soft-delete time, sort order, and UTC millisecond timestamps.
- [x] Added fresh-database, legacy-upgrade, idempotence, and CRUD tests.
- [x] Added single-instance behavior that activates the existing panel.
- [x] Added inline editing with Enter, Escape, and outside-click behavior.
- [x] Added persisted drag sorting and keyboard-accessible up/down controls.
- [x] Fixed reverse drag traversal and restored dragged-row preview highlighting.
- [x] Added completion timestamp handling, clear-completed, soft delete, and five-second undo.
- [x] Added lightweight add/complete/delete/reorder animations with reduced-motion handling.
- [x] Added versioned JSON export/import preview, UUID merge, and SQLite backup.
- [x] Added configurable global shortcut, defaulting to `Ctrl + Shift + Space`.
- [x] Added shortcut conflict handling with rollback to the previous valid shortcut.
- [x] Added optional system autostart and current-state display.
- [x] Added a settings overlay with light/dark styling and mutually exclusive panel overlays.
- [x] Updated README and optimization checklist.
- [x] Verified hidden startup and shortcut-triggered panel display using Win32 window enumeration.

## Files Modified

| File or Area | Changes | Rationale |
|--------------|---------|-----------|
| `src-tauri/src/db.rs` | Migration runner, sync-ready schema, tests | Preserve old data and prepare record-level sync |
| `src-tauri/src/commands.rs` | Edit, reorder, soft delete, restore, clear, shortcut toggle | Complete P1 behavior and centralize native toggle |
| `src-tauri/src/data_exchange.rs` | Versioned JSON merge and SQLite backup | Reliable data portability before cloud sync |
| `src-tauri/src/tray.rs` | Public internal toggle result and dialog/blur state | Reuse visibility behavior safely |
| `src-tauri/src/lib.rs` | Single-instance, dialog, shortcut, and autostart plugins | Assemble native desktop capabilities |
| `src/lib/stores/todoStore.ts` | Optimistic Todo operations and error state | Keep UI components free of persistence details |
| `src/lib/components/TodoItem.svelte` | Editing and drag interactions | Complete row-level P1 UX |
| `src/lib/components/TodoPanel.svelte` | Reorder orchestration, undo, overlays, settings | Integrate user-facing workflows |
| `src/lib/components/DataManager.svelte` | Import/export/backup UI | Expose reliability features |
| `src/lib/components/SettingsPanel.svelte` | Shortcut and autostart controls | Expose P2 desktop settings |
| `src/lib/api/desktopSettings.ts` | Plugin calls and shortcut rollback | Keep desktop transport outside components |
| `src/lib/utils/reorderPreview.ts` | Pure adjacent-swap preview algorithm | Make drag behavior testable |
| `src/app.css` | P1 animations and P2 settings styles | Maintain compact light/dark UI |
| `README.md`, `OPTIMIZATION_TODO.md` | Feature and status documentation | Keep project state discoverable |

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Use pointer-driven adjacent swaps for drag sorting | HTML5 drag/drop, external library, pointer events | Native HTML drag showed prohibited cursors and poor preview control; a library would be excessive |
| Keep the dragged Todo highlighted by UUID | Highlight original DOM slot, highlight dragged record | Users track the task they grabbed, even after its index changes |
| Store shortcut choice in local storage | SQLite, config file, local storage | It is device-local UI configuration and does not belong in synced Todo data |
| Use official Tauri shortcut/autostart plugins | Custom OS APIs, plugins | Official plugins preserve cross-platform structure with a small implementation surface |
| Register shortcuts in the frontend but toggle in Rust | Fully frontend, fully Rust | Plugin settings remain near UI state while tray race logic stays centralized |
| Roll back shortcut registration on conflicts | Disable silently, keep invalid selection, rollback | The previous working shortcut must remain usable |
| Keep manual JSON exchange separate from future S3 sync | Reuse raw SQLite, combine immediately | Versioned records and merge behavior can be tested before adding network and credentials |

## Pending Work

## Immediate Next Steps

1. Complete P2 tray placement: select the monitor containing the tray anchor, use its work area, and clamp the panel within visible bounds.
2. Test and correct coordinate conversion at Windows 100%, 125%, 150%, and 200% scaling, including mixed-DPI multi-monitor setups.
3. Extract tray visibility/placement decisions into testable pure helpers and add tray toggle/placement unit tests.
4. Manually verify tray click, shortcut toggle, panel blur, native dialogs, settings overlay, and single-instance activation together.
5. After P2 is stable, begin P3 with a versioned sync document and pure UUID merge tests before adding any S3 client or credentials.

## Blockers/Open Questions

- [ ] Tauri tray event rectangles and monitor APIs may expose coordinates in different logical/physical spaces on mixed-DPI Windows setups; verify against real monitors before finalizing conversion.
- [ ] Decide whether the panel should use monitor work area rather than full monitor size on each supported platform; Windows should avoid the taskbar.
- [ ] macOS and Linux require real-device checks for tray coordinates, shortcut registration, and autostart behavior.
- [ ] Future S3 client and system keyring crates remain undecided; evaluate dependency size and MinIO path-style support during P3 design.

## Deferred Items

- S3/MinIO synchronization remains deferred until P2 and the pure merge model are complete.
- Automatic synchronization remains deferred until manual sync, ETag conflict handling, and credential storage are proven.
- Windows installer metadata, signing, and automatic updates remain P4 work.
- Complex categories, reminders, accounts, collaboration, and direct SQLite cloud upload remain out of scope.

## Context for Resuming Agent

## Important Context

- Start by reading `AGENTS.md` and `OPTIMIZATION_TODO.md`.
- The repository was clean at handoff creation. The new handoff file is the only expected untracked file until committed.
- P0 and P1 are complete and committed. Do not redo migrations, editing, drag sorting, undo, or data exchange.
- The current drag algorithm was refined through several user-reported bugs. Preserve adjacent traversal in both directions and the UUID-bound highlighter.
- `tray.rs` currently uses `window.current_monitor()` after computing a tray anchor. This can choose the wrong monitor while the hidden panel still belongs to another display. The next change should resolve the monitor from the tray point.
- Current clamping uses full monitor size, not an explicit work area. This may overlap taskbars or docks.
- The 350 ms recent-blur marker prevents a tray click from hiding and immediately reopening the panel on Windows. Do not remove it without manual tray testing.
- `PanelState` also protects native file dialogs from blur hiding. Settings is an in-webview overlay and should not need the dialog guard.
- `toggle_panel` returns whether the panel was shown. The shortcut command emits `focus-new-todo` only when it actually opens the panel.
- At startup, Win32 process APIs may report a visible `com.eggdone.desktop-siw` window from the single-instance plugin. This is not the Todo panel. Enumerate windows and check the `蛋定 Todo` title when validating hidden startup.
- The latest runtime check confirmed `蛋定 Todo` was hidden at startup and visible after simulating `Ctrl + Shift + Space`.
- The global shortcut is registered after the frontend mounts. Registration conflicts are shown in settings and do not crash the app.
- Shortcut settings use local storage keys `eggdone-global-shortcut` and `eggdone-global-shortcut-enabled`.
- Autostart passes `--autostart`, but startup behavior is already hidden for all launches. The argument is reserved for future differentiation.
- The README “当前限制” line still says the app does not contain autostart even though autostart is now implemented. Correct this during the next documentation update.
- Never put S3 secrets in SQLite, local storage, JSON exports, logs, tests, or handoff documents.

## Assumptions Made

- Windows 10/11 remains the primary target and manual validation platform.
- SQLite remains authoritative and fully usable offline.
- Shortcut and autostart preferences are device-local and must not be synchronized.
- The existing original mascot artwork is approved and should not be replaced with commercial IP.
- A single versioned remote `todos.json` object remains the intended first S3/MinIO sync design.

## Potential Gotchas

- Keep `127.0.0.1:1420` aligned between Vite and Tauri; `localhost` previously caused indefinite dev-server waiting on this machine.
- The panel intentionally starts with `visible: false`; no normal window at launch is correct.
- The tray handle must stay in managed application state or the icon can disappear.
- Do not use array indices as stable drag identity.
- Do not hold the SQLite mutex across future network requests.
- Backups must use the SQLite backup API, not copy a live WAL database file.
- Browser preview has no Tauri invoke bridge and cannot fully validate settings or persistence.
- A normal `target` directory may be locked by a running dev process. Use a separate `CARGO_TARGET_DIR` for isolated desktop builds.
- The handoff scaffold script needs `PYTHONUTF8=1` on this Windows environment because Chinese Git commit messages can fail under GBK decoding.

## Environment State

## Tools/Services Used

- Windows PowerShell
- pnpm
- Rust/Cargo stable
- Tauri 2.11.x
- Svelte 5 / SvelteKit 2 / Vite 6 / Vitest
- SQLite through bundled `rusqlite`
- Official `tauri-plugin-global-shortcut` and `tauri-plugin-autostart`
- Win32 `EnumWindows` and `IsWindowVisible` for desktop runtime verification

## Active Processes

- No `eggdone.exe` or Cargo process was running at handoff creation.
- Two unrelated Node processes were present; do not terminate them based on this handoff.

## Environment Variables

- `TAURI_DEV_HOST` is supported by Vite configuration.
- `CARGO_TARGET_DIR` can be set to an isolated directory for build verification.
- `PYTHONUTF8=1` is needed when running the handoff scripts with Chinese Git history.
- No S3, MinIO, signing, or credential environment variables are configured.

## Verification Status

The following checks passed after the latest P2 work:

```text
pnpm check
pnpm test
pnpm build
cd src-tauri
cargo fmt -- --check
cargo check
cargo test
```

Results:

- Svelte check: 0 errors and 0 warnings.
- Frontend tests: 6 passed.
- Rust tests: 10 passed.
- Tauri debug desktop build succeeded using:
  `CARGO_TARGET_DIR=D:\Develop\EggDone\src-tauri\target\p2-desktop-check`
- Built executable:
  `D:\Develop\EggDone\src-tauri\target\p2-desktop-check\debug\eggdone.exe`
- Runtime validation confirmed hidden Todo panel on startup.
- Simulated default shortcut confirmed the Todo panel became visible.

## Related Resources

- `README.md`
- `AGENTS.md`
- `OPTIMIZATION_TODO.md`
- `src-tauri/src/tray.rs`
- `src/lib/api/desktopSettings.ts`
- `src/lib/utils/reorderPreview.ts`
- Previous handoff: `2026-06-06-222909-eggdone-mvp-and-optimization-roadmap.md`
- Tauri documentation: <https://v2.tauri.app/>

---

**Recommended first action for the next session:** inspect Tauri's monitor/work-area APIs and refactor `tray.rs` placement into pure coordinate helpers with tests before changing runtime behavior.
