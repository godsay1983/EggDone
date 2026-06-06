# Handoff: EggDone MVP and Optimization Roadmap

## Session Metadata
- Created: 2026-06-06 22:29:09
- Project: `D:\Develop\EggDone`
- Branch: `main`
- Session duration: approximately 2 hours

### Recent Commits

- `72bc61b` docs: 添加优化TODO文档并更新README引用
- `993c33e` feat: 添加亮色和暗色主题切换功能
- `6362590` feat: 使用图标图片替换CSS绘制的吉祥物
- `188bc26` feat: 替换原创应用图标并更新文档
- `2af9a48` fix: 修复托盘图标被丢弃问题，更新开发环境网络配置
- `a9a4abc` feat: 初始化 EggDone 托盘 Todo 桌面应用项目

## Handoff Chain

- **Continues from**: None
- **Supersedes**: None

This is the first handoff for the project.

## Current State Summary

EggDone is a working Tauri 2, Svelte 5, TypeScript, and SQLite tray Todo MVP. It launches hidden, keeps a tray icon alive, opens a fixed 360x560 panel near the tray, hides on blur, and supports creating, completing, reopening, and deleting persistent tasks. The application uses a user-provided original mascot image for application, tray, header, empty-state, and about-dialog artwork. Light and dark themes are implemented and persisted. The repository is clean apart from this handoff file, all feature work is committed to `main`, and the next planned work starts with the P0 data migration and single-instance tasks in `OPTIMIZATION_TODO.md`.

## Codebase Understanding

## Architecture Overview

- The frontend is a SvelteKit static SPA under `src/`.
- `src/routes/+page.svelte` only mounts `TodoPanel`.
- `TodoPanel.svelte` owns panel UI state, theme switching, Tauri event listeners, and user interactions.
- `TodoItem.svelte` renders one Todo row.
- `todoStore.ts` coordinates frontend state and delegates persistence to `todoApi.ts`.
- `todoApi.ts` calls Rust commands through Tauri `invoke`.
- The Rust application is assembled in `src-tauri/src/lib.rs`.
- `db.rs` opens one bundled SQLite connection protected by a `Mutex` and creates the MVP schema.
- `commands.rs` implements CRUD commands.
- `tray.rs` owns tray menus, tray click behavior, panel placement, and the blur-click race workaround.
- The app data database is stored through Tauri `app_data_dir` as `eggdone.sqlite3`.
- The optimization roadmap intentionally puts migrations and sync-ready fields before editing, sorting, and S3/MinIO sync.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `OPTIMIZATION_TODO.md` | Prioritized P0-P4 roadmap and acceptance criteria | Start all future optimization work here |
| `AGENTS.md` | Repository development rules and architecture boundaries | Follow before editing |
| `src-tauri/src/lib.rs` | Tauri builder, managed state, window events, command registration | Tray lifetime and hide-on-blur behavior |
| `src-tauri/src/tray.rs` | Tray creation, menu actions, positioning, visibility toggling | Contains Windows-specific race workaround |
| `src-tauri/src/db.rs` | SQLite connection and initial table creation | Must be replaced or extended by migration infrastructure |
| `src-tauri/src/commands.rs` | Current Todo CRUD commands | Needs UUID, editing, soft delete, sorting, and completion-time support |
| `src/lib/components/TodoPanel.svelte` | Main UI, theme state, Tauri events, add/toggle/delete actions | Main frontend feature integration point |
| `src/lib/components/TodoItem.svelte` | Todo row | Future inline editing and drag handle belong here |
| `src/lib/stores/todoStore.ts` | Frontend state operations | Extend for editing, undo, sorting, import, and sync status |
| `src/lib/api/todoApi.ts` | Typed Tauri command calls | Keep transport logic isolated here |
| `src/app.css` | Complete light/dark visual system | Maintain both themes for all new UI |
| `src-tauri/tauri.conf.json` | Hidden fixed panel and bundle configuration | Development URL is deliberately IPv4 |
| `src-tauri/icons/app-icon.png` | Transparent processed application icon source | Regenerate platform icons from this file |
| `src-tauri/icons/eggdone-artwork-original.png` | Original user-provided image copy | Preserve as source artwork |
| `static/eggdone-icon.png` | Mascot image consumed by the frontend | Keep synchronized with processed icon source |

## Key Patterns Discovered

- Keep Rust modules separated: commands, database, and tray behavior do not belong in one file.
- Tauri command errors are returned as user-readable Chinese strings.
- SQL uses bound parameters.
- Frontend state changes go through `todoStore`; components do not call SQLite directly.
- `TodoPanel` checks `isTauri()` before registering Tauri event listeners.
- Theme choice uses `localStorage` key `eggdone-theme`; initial page script applies it before rendering to prevent a light-theme flash.
- The yellow mascot color remains the accent in both themes.
- Tauri tray icon handles are reference counted. The built `TrayIcon` must remain managed in application state.
- Development server and `devUrl` are fixed to `127.0.0.1:1420`; using `localhost` caused Tauri CLI to wait indefinitely on this machine.

## Work Completed

## Tasks Finished

- [x] Initialized Tauri 2 + SvelteKit + TypeScript with pnpm.
- [x] Implemented hidden startup and tray-resident application behavior.
- [x] Implemented tray left-click toggle and right-click menu.
- [x] Implemented fixed, borderless, always-on-top panel that skips the taskbar.
- [x] Implemented panel placement near the tray with a screen-corner fallback.
- [x] Implemented hide-on-close and hide-on-blur behavior.
- [x] Implemented SQLite Todo persistence with bundled `rusqlite`.
- [x] Implemented Todo add, list, complete/uncomplete, delete, and remaining count.
- [x] Implemented custom mascot artwork and generated Windows, macOS, Linux, iOS, and Android icon assets.
- [x] Replaced header, empty-state, and about-dialog placeholders with the mascot image.
- [x] Implemented persistent light/dark themes with system-theme initialization.
- [x] Fixed tray icon disappearance by retaining the `TrayIcon` handle.
- [x] Fixed `tauri dev` startup by aligning Vite and Tauri on `127.0.0.1`.
- [x] Added README, AGENTS, and detailed optimization roadmap.
- [x] Verified Svelte type checking, Rust checking, frontend production build, and Tauri debug build during development.

## Files Modified

| File or Area | Changes | Rationale |
|--------------|---------|-----------|
| `src/` | Added Todo UI, store, API layer, themes, mascot use, and styling | Deliver the panel MVP while keeping frontend responsibilities separated |
| `src-tauri/src/` | Added SQLite, commands, tray integration, lifecycle handling, and positioning | Implement native desktop behavior and persistence |
| `src-tauri/icons/` | Added original artwork, processed transparent source, and generated platform icons | Use the user's selected original mascot across platforms |
| `static/` | Added frontend mascot and favicon | Keep panel visuals consistent with application icon |
| `README.md` | Added setup, build, architecture, feature, and roadmap links | Replace GitLab placeholder documentation |
| `AGENTS.md` | Added coding, architecture, IP, platform, and verification rules | Guide future agents and contributors |
| `OPTIMIZATION_TODO.md` | Added P0-P4 plan through S3/MinIO sync and release | Prevent dependency-order mistakes and scope creep |
| `vite.config.js` and `tauri.conf.json` | Fixed dev host to `127.0.0.1` and configured hidden panel | Make development startup reliable on the current Windows machine |

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Keep SQLite as the local source of truth | SQLite, direct cloud-only storage | Offline Todo behavior must remain fast and reliable |
| Use `rusqlite` with bundled SQLite | Tauri SQL plugin, external SQLite, `rusqlite` | Small dependency surface and no system SQLite requirement |
| Store the tray handle in Tauri managed state | Ignore returned handle, global static, managed state | Tauri removes the icon when the last handle is dropped |
| Use S3-compatible object sync later | S3/MinIO, application-managed Git, custom backend | MinIO and S3 provide a small API surface without Git authentication and merge complexity |
| Sync versioned JSON, not the SQLite file | SQLite file upload, per-record API, one JSON object | JSON supports record-level merging and avoids SQLite corruption |
| Allow HTTP MinIO endpoints with a warning | HTTPS-only, unrestricted HTTP | Trusted local networks may need HTTP, but users must see the credential/data exposure risk |
| Implement manual sync before automatic sync | Automatic first, manual first | Conflict handling and credential behavior must be proven before background writes |
| Put migrations before editing and sync | Build UI features first, migrate first | UUID, soft delete, sort order, and UTC timestamps are shared prerequisites |
| Persist theme locally | System-only, local preference, database setting | Theme is device-local UI state and does not need SQLite or cloud sync |

## Pending Work

## Immediate Next Steps

1. Implement P0 database migrations: add `schema_migrations`, UUID, `completed_at`, `deleted_at`, `sort_order`, and UTC millisecond timestamps while preserving existing rows.
2. Add migration integration tests for both an empty database and the current v0.1 schema before changing Todo commands.
3. Add single-instance behavior so repeated launches activate the existing panel instead of creating another tray process.
4. After P0 is stable, implement inline task editing, soft delete with undo, completion timestamps, clear-completed, and persisted ordering.
5. Only then begin the manual S3/MinIO sync provider described in `OPTIMIZATION_TODO.md`.

## Blockers/Open Questions

- [ ] UUID library choice: prefer a small Rust UUID dependency, but select the exact crate/version when implementing migrations.
- [ ] Timestamp tie-breaker for sync: roadmap proposes `updated_at` plus a stable deterministic rule; define whether the tie-breaker is `device_id`, change ID, or UUID ordering.
- [ ] System credential storage implementation: choose a cross-platform keyring approach before S3 credentials are added.
- [ ] S3 Rust client size: evaluate AWS SDK versus a smaller S3-compatible client before adding dependencies.
- [ ] Drag-and-drop library: prefer native pointer handling or a small dependency; do not add a large UI framework.

## Deferred Items

- Automatic synchronization is deferred until manual S3/MinIO sync and ETag conflict handling are tested.
- Account systems, custom backend services, real-time WebSockets, and team sharing are outside current scope.
- Complex categories, projects, labels, search, and reminders are deferred to protect the lightweight product scope.
- Application-managed Git pull/commit/push is explicitly not planned.
- Code signing and automatic updates are deferred until the Windows installer flow is stable.

## Context for Resuming Agent

## Important Context

- Read `AGENTS.md` and `OPTIMIZATION_TODO.md` before implementation.
- The current repository state is a committed, working MVP on `main`; this handoff is the only expected untracked addition.
- Do not implement sync before the data model migration. Current rows only have integer `id`, title, completed, and SQLite text timestamps.
- Existing delete is a hard `DELETE`; P0/P1 must convert this to soft deletion before cloud sync.
- Existing listing sorts incomplete first, then newest creation and ID. Future `sort_order` semantics need to replace or explicitly combine with this ordering.
- The tray icon will disappear if `tray::create_tray` stops returning a handle or `lib.rs` stops managing it.
- The 350 ms `PanelState` blur marker in `tray.rs` prevents a Windows tray click from hiding and immediately reopening the panel. Preserve or replace it only after real tray interaction testing.
- The application starts hidden. A successful `pnpm tauri dev` must eventually print `Running target\debug\eggdone.exe`; Vite readiness alone does not mean the tray app has started.
- The icon may appear under the Windows hidden-icons caret. The tray image is the yellow mascot holding a check board.
- Normal browser preview lacks the Tauri invoke bridge. The UI may show a database/invoke error in a regular browser even though desktop execution works. Event listeners are guarded with `isTauri()`, but `todos.load()` still invokes Tauri.
- The user-provided original image path outside the repository is not needed anymore. Both original and processed copies are tracked under `src-tauri/icons/`.
- S3/MinIO requirements agreed with the user: custom endpoint, region, bucket, object key, path style, HTTP or HTTPS, HTTP warning, system credential storage, UUID merge, soft deletes, ETag/`If-Match`, and manual sync first.
- Never store S3 Secret Key in SQLite, JSON, localStorage, logs, or this handoff.

## Assumptions Made

- Windows remains the primary development and verification platform.
- macOS and Linux structure must stay portable, but real-device validation has not occurred.
- The mascot artwork supplied by the user is approved for use in the project.
- Local SQLite remains authoritative and fully usable while offline.
- A single `todos.json` object is sufficient for the first sync version.
- HTTP S3 endpoints are allowed only by explicit user configuration and with a visible warning.

## Potential Gotchas

- On Windows, `localhost` resolved incompatibly with the Vite listener during `tauri dev`; retain `127.0.0.1` in both config files.
- The Tauri panel intentionally has `visible: false`; absence of a normal window at startup is expected.
- `skipTaskbar`, `alwaysOnTop`, no decorations, fixed width, and hide-on-blur are deliberate product behavior.
- Clicking panel-owned native menus or future dialogs may trigger blur and hide the panel. Revisit window event handling when settings or file dialogs are added.
- The current database connection is one `Mutex<Connection>`. Do not hold the lock across network calls when sync is implemented.
- SQLite uses WAL. Backups should use an SQLite-aware backup method or checkpoint strategy, not blindly copy an active database.
- Theme storage is local-only and should not be added to synced Todo JSON.
- Generated icon assets are numerous. Update `app-icon.png`, then run `pnpm tauri icon src-tauri/icons/app-icon.png`; do not hand-edit each generated size.
- `pnpm tauri dev` may take roughly one minute on a cold Rust build.
- Existing frontend browser preview error text is expected outside Tauri until an explicit web/mock API adapter is added.

## Environment State

## Tools/Services Used

- Windows PowerShell
- Node.js `v24.16.0`
- pnpm `11.3.0`
- Rust/Cargo `1.94.0`
- Tauri CLI `2.11.x`
- Svelte 5 / SvelteKit 2 / Vite 6
- SQLite through `rusqlite 0.37` with bundled SQLite
- Codex in-app browser for 360x560 visual checks
- Pillow was used locally to process the user image; no runtime Python dependency was added

## Active Processes

At handoff creation, a user-started development session is running:

- `eggdone.exe` from `src-tauri\target\debug`
- Tauri CLI, Vite, and Cargo child processes

Do not terminate these processes unless the next task requires a clean restart. Use the tray “退出” action or stop the owning `pnpm tauri dev` terminal when appropriate.

## Environment Variables

- `TAURI_DEV_HOST` is supported by `vite.config.js` for remote/mobile development.
- No S3, MinIO, credential, or signing environment variables are configured.
- No secrets are required for the current MVP.

## Verification Status

The following checks passed during implementation:

```text
pnpm check
pnpm build
cargo fmt -- --check
cargo check
pnpm tauri build --debug --no-bundle
```

Additional manual verification completed:

- Tray icon remained visible after fixing handle lifetime.
- Tray icon click opened the panel.
- SQLite database was created in the Windows application data directory.
- 360x560 light and dark themes rendered without overflow.
- Theme selection persisted after reload.
- Generated Windows executable contained the new mascot icon.

## Related Resources

- `README.md`
- `AGENTS.md`
- `OPTIMIZATION_TODO.md`
- Tauri configuration: `src-tauri/tauri.conf.json`
- Current database schema: `src-tauri/src/db.rs`
- Current tray implementation: `src-tauri/src/tray.rs`
- Official Tauri documentation: <https://v2.tauri.app/>
- AWS S3 conditional writes: <https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html>

---

**Recommended first action for the next session:** create the migration design and tests for the current v0.1 SQLite schema before changing any Todo behavior.
