# Handoff: EggDone Sync, Windows Release, and Feature Roadmap

## Session Metadata

- Created: 2026-06-07 21:06:27
- Project: `D:\Develop\EggDone`
- Branch: `main`
- Session duration: approximately one day across continued optimization sessions

### Recent Commits

- `1e9bc22` feat: 添加Windows安装器自动验证脚本及相关文档
- `898a559` feat: 添加MIT许可证、Windows构建配置及发布文档
- `4625668` feat: 在任务操作后更新系统托盘徽章以显示待办计数
- `cb6be4f` feat: 添加同步状态显示
- `a12caf4` feat: 新增刷新方法，避免显示初始加载状态
- `3f1d461` feat: 实现自动同步功能，包括启动同步、修改后防抖同步、重试机制和状态显示
- `900e3df` feat: 实现 S3 手动同步及 ETag 冲突保护
- `4bf9232` feat: 实现S3/MinIO同步配置、凭据管理和连接测试

## Handoff Chain

- **Continues from**: [2026-06-07-101812-eggdone-p2-desktop-integration.md](./2026-06-07-101812-eggdone-p2-desktop-integration.md)
- **Supersedes**: None

The previous handoff covers P0/P1 and the start of P2. This handoff records the completed P2/P3/P4 work and the new personal-use feature roadmap.

## Current State Summary

EggDone now has a complete lightweight Todo workflow, desktop integration, record-level S3/MinIO synchronization, automatic synchronization, visible sync state, a dynamically badged tray icon, and a tested Windows NSIS installer. The engineering roadmap is complete except for deferred public-release and cross-platform tasks. The user primarily uses the app personally and explicitly chose to defer Windows code signing, automatic updates, high-DPI manual testing, and macOS/Linux verification. The current discussion has shifted to product features such as search, reminders, today view, grouping, and recurring tasks. A new `FUNCTION_OPTIMIZATION_TODO.md` has been created for that work but is not committed yet.

## Codebase Understanding

## Architecture Overview

- The frontend is a SvelteKit static app using Svelte 5 and TypeScript.
- `TodoPanel.svelte` coordinates the compact tray panel, overlays, Todo rendering, sync status, automatic sync lifecycle, theme, and desktop events.
- `TodoItem.svelte` owns row-level editing, completion, deletion, and pointer-driven reorder behavior.
- `todoStore.ts` remains the frontend business-state boundary. Mutations update the local store, persist through Tauri commands, refresh tray counts, and notify automatic sync.
- `autoSync.ts` serializes background sync attempts, performs startup sync, debounces local changes, and applies finite retry delays.
- Rust `commands.rs` exposes Todo, import/export, desktop, sync, and tray-count commands.
- `db.rs` owns SQLite initialization, forward migrations, WAL configuration, sync settings, and database tests.
- `sync.rs` owns the versioned remote document and deterministic UUID merge rules.
- `s3_sync.rs` owns S3-compatible HTTP operations, AWS SigV4 requests, ETag protection, runtime overlap protection, endpoint validation, and OS credential access.
- `tray.rs` owns tray menus, panel positioning, focus race handling, tooltip counts, and runtime tray badge rendering.
- SQLite remains the offline source of truth. Sync merges records and never uploads the SQLite file.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `AGENTS.md` | Repository rules and verification requirements | Read before any implementation |
| `OPTIMIZATION_TODO.md` | Engineering, release, and platform roadmap | Most remaining items are intentionally deferred |
| `FUNCTION_OPTIMIZATION_TODO.md` | Personal-use product feature roadmap | Current uncommitted planning artifact and likely next work source |
| `src/lib/components/TodoPanel.svelte` | Main panel orchestration | Search, filters, today view, and reminders will integrate here |
| `src/lib/components/TodoItem.svelte` | Todo row interactions | Pinning, dates, grouping, and row metadata affect this component |
| `src/lib/components/SyncSettings.svelte` | S3/MinIO settings and manual sync | Preserve HTTP warning, credential, and sync-state behavior |
| `src/lib/stores/todoStore.ts` | Todo state and mutation orchestration | New task fields and filters must preserve store behavior |
| `src/lib/sync/autoSync.ts` | Startup/debounced/retried sync | Every synced local mutation should continue to notify this layer |
| `src-tauri/src/db.rs` | SQLite schema and migrations | New fields require forward migration and fresh/legacy tests |
| `src-tauri/src/sync.rs` | Versioned sync document and merge | New synced fields need deterministic merge semantics |
| `src-tauri/src/s3_sync.rs` | S3/MinIO transport and credentials | Do not expose credentials or weaken ETag protection |
| `src-tauri/src/tray.rs` | Tray panel and dynamic badge | Badge currently represents incomplete/total counts |
| `src-tauri/tauri.windows.conf.json` | Windows NSIS configuration | Includes current-user installation and downgrade hook |
| `src-tauri/windows/installer-hooks.nsh` | Extra silent downgrade guard | Required due to Tauri 2.11.2 silent downgrade behavior |
| `scripts/verify-windows-installer.ps1` | Install/upgrade/downgrade/uninstall check | Aborts if a real EggDone installation is detected |
| `docs/RELEASING_WINDOWS.md` | Windows release procedure | Contains installer build and verification commands |

## Key Patterns Discovered

- API transport, stores, components, commands, database, sync, and tray behavior stay in their existing ownership boundaries.
- Database changes use ordered forward migrations and must cover fresh databases, legacy upgrades, and idempotent reruns.
- Synced Todo fields require updates across SQLite, Rust structs, TypeScript types, JSON exchange, remote sync document, merge tests, and tray/store refresh behavior.
- Todo conflicts resolve by newer `updated_at`; equal timestamps use deletion precedence and stable `updated_by` ordering.
- S3 updates use `If-Match`; first creation uses `If-None-Match: *`; a conflict triggers one fresh download, merge, and retry.
- Secret and access keys live in the operating-system credential store. SQLite stores only non-sensitive sync configuration.
- Automatic sync never blocks local Todo operations. It performs startup sync, a four-second mutation debounce, single-flight execution, and limited network retries.
- Store refreshes after background sync should not show the initial loading state or flash the panel.
- Tray badge rendering preserves source icon dimensions and updates after all task-changing operations.
- UI changes must support light/dark themes and `prefers-reduced-motion`.

## Work Completed

## Tasks Finished

- [x] Completed P2 panel positioning, work-area clamping, focus-race hardening, global shortcut, autostart, and tray behavior tests.
- [x] Added S3/MinIO configuration with custom endpoint, region, bucket, object key, path style, HTTP support, and explicit plaintext warning.
- [x] Stored Access Key and Secret Key in the operating-system credential store.
- [x] Added connection testing, manual synchronization, versioned `todos.json`, UUID merge, ETag conflict protection, and conflict retry.
- [x] Added startup sync, four-second debounced automatic sync, single-flight protection, and limited exponential retry.
- [x] Added panel sync status for syncing, synced, offline, conflict, and failure states without UI flashing.
- [x] Added tray tooltip counts and a dynamically rendered high-contrast incomplete/total badge.
- [x] Added product metadata, MIT license, Windows-specific NSIS configuration, release checks, manual regression documentation, and release/rollback guidance.
- [x] Added automated Windows installer verification for clean install, upgrade, downgrade rejection, version metadata, and uninstall.
- [x] Fixed Tauri 2.11.2 silent downgrade behavior with an NSIS preinstall hook.
- [x] Discussed personal-use product enhancements and created a staged feature optimization plan.

## Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| `FUNCTION_OPTIMIZATION_TODO.md` | New F1-F5 roadmap for search, filtering, pinning, dates, reminders, today view, groups, recurrence, and quick syntax | Separate personal-use features from engineering/release work |
| `README.md` | Added a link to the new feature roadmap | Make the planning document discoverable |
| `.claude/handoffs/2026-06-07-210627-eggdone-sync-release-and-feature-roadmap.md` | This handoff document | Preserve context for the next session |

All sync, tray badge, and Windows installer implementation changes are already committed through `1e9bc22`.

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Use S3-compatible object sync | Direct SQLite upload, Git sync, custom backend, S3 object | Record merge supports offline edits and MinIO without operating a custom service |
| Permit HTTP endpoints with explicit warning | Force HTTPS, silently permit HTTP | Personal LAN MinIO may use HTTP, but users must acknowledge plaintext credential and data transport |
| Keep credentials out of SQLite | SQLite encryption, local storage, OS credential store | OS credential storage limits accidental export, logs, and database exposure |
| Use ETag conditional writes | Last-write-wins upload, object locks, custom server | Conditional requests prevent silently overwriting a newer remote document |
| Add automatic sync only after manual sync stabilized | Immediate automatic sync, manual-only forever | Existing merge and ETag protection made background sync acceptable |
| Render tray badge at runtime | Static icon, tooltip only, text badge | Runtime rendering communicates counts without platform-specific overlay APIs |
| Use NSIS current-user installation | MSI/per-machine, portable-only, NSIS current-user | Personal Windows use should not require administrator rights |
| Add a custom NSIS downgrade hook | Trust Tauri flag, permit downgrade, custom hook | Tauri 2.11.2 generated the flag but silent downgrade still replaced the newer build |
| Defer release-hardening work | Implement signing/updater/platform validation now, defer | User primarily uses EggDone personally and does not need public distribution safeguards yet |
| Prioritize search before reminders and groups | Groups first, reminders first, search first | Search and completed-task filtering provide immediate value with low schema and interaction risk |
| Keep pure dates distinct from timestamps | Store all dates as midnight UTC, separate date semantics | Midnight UTC changes calendar day across time zones |
| Keep reminder-trigger state device-local | Sync trigger records, device-local trigger records | One device firing a notification must not suppress notifications on another device |

## Pending Work

## Immediate Next Steps

1. Review and commit the current `FUNCTION_OPTIMIZATION_TODO.md`, README link, and this handoff if the user wants the planning state preserved in Git.
2. When the user says to continue the feature plan, start with F1 search and completed-task visibility. Inspect `TodoPanel.svelte`, `todoStore.ts`, and current settings persistence before editing.
3. Implement F1 incrementally: pure display filtering first, persisted “show completed” preference second, then pinning with a migration and full sync/import/export support.
4. Run `pnpm release:check` after implementation and manually verify the tray panel because browser preview cannot exercise Tauri commands.

## Blockers/Open Questions

- [ ] Pinning semantics: decide whether completed pinned tasks remain above incomplete unpinned tasks or whether completion grouping takes precedence. Suggested default: incomplete pinned, incomplete normal, completed pinned, completed normal.
- [ ] Search placement: determine whether it is always visible or opened from a compact icon. The roadmap currently proposes a collapsible search field.
- [ ] Completed-task visibility persistence: local storage is probably sufficient because it is device-local UI preference; confirm during implementation.
- [ ] Reminder notification actions vary by platform. Windows is primary, but unsupported action buttons need an in-panel fallback.
- [ ] Group names: decide whether duplicate names are permitted before F3 implementation.

## Deferred Items

- Windows code signing: deferred because the app is for personal use and unsigned SmartScreen warnings are acceptable.
- Automatic application updates and update signing: deferred until distribution to other users becomes important.
- Windows 100%, 125%, 150%, and 200% manual DPI matrix: deferred; current positioning code and unit tests remain in place.
- macOS and Linux real-device validation: deferred because Windows is the active platform.
- Complex project management, multilevel projects, subtasks, tags plus groups, boards, attachments, collaboration, and AI scheduling: explicitly out of scope.

## Context for Resuming Agent

## Important Context

- Read `AGENTS.md`, this handoff, `OPTIMIZATION_TODO.md`, and `FUNCTION_OPTIMIZATION_TODO.md` before changing code.
- The current branch is `main`. At handoff creation, the only product change not committed is the new feature roadmap and README link.
- The user explicitly asked to pause remaining release/platform work and discuss product features for personal use.
- Do not restart P3 synchronization work. Manual and automatic S3/MinIO sync, ETag protection, credentials, status UI, and tests are complete and committed.
- Do not restart Windows installer work. Install, upgrade, downgrade rejection, and uninstall automation are complete and committed.
- `FUNCTION_OPTIMIZATION_TODO.md` is intentionally separate from `OPTIMIZATION_TODO.md`: the former is product functionality; the latter is engineering/release work.
- The recommended next feature sequence is search and completed-task folding, pinning, due dates/today view, notifications, single-level groups, recurring tasks, then quick-add syntax.
- Search should initially remain a local view filter and must not alter persisted ordering.
- “Show completed” is a device-local presentation preference and should not be synchronized.
- Pinning changes Todo data and therefore must be included in SQLite migrations, JSON import/export, remote sync, merge tests, and TypeScript/Rust types.
- Reminder dates need two meanings: a date-only local calendar value and an exact UTC timestamp. Do not encode date-only values as midnight UTC.
- Notification-fired state is local to each device. Do not sync it.
- Existing drag reorder behavior was refined through multiple user reports. Any filtering or pinning implementation must preserve UUID identity and adjacent preview traversal.
- Never expose S3 credentials in source, logs, tests, JSON exports, handoffs, or final responses.

## Assumptions Made

- Windows 10/11 remains the primary runtime.
- EggDone remains a lightweight tray utility, not a full task-management platform.
- SQLite remains authoritative and usable without network access.
- Existing S3/MinIO synchronization must continue to work as new fields are added.
- Personal-use convenience matters more than public release polish for the next phase.
- Simple single-level grouping is acceptable; projects, nested groups, and simultaneous tag systems are not.

## Potential Gotchas

- `pnpm tauri dev` intentionally starts with the panel hidden; inspect the tray rather than waiting for a normal window.
- Keep `127.0.0.1:1420` aligned between Vite and Tauri. `localhost` previously caused startup waiting on this machine.
- Browser preview does not have the Tauri invoke bridge and cannot validate SQLite, tray, credentials, notifications, or sync.
- Do not hold the SQLite mutex while awaiting S3 network requests.
- Updating only frontend Todo types will silently break import/export or sync compatibility; trace every new field across all layers.
- Background sync refresh must continue to use the non-loading refresh path to avoid panel flashing.
- Dynamic tray icons require the tray handle to remain managed and image dimensions to remain valid.
- Installer verification intentionally refuses to run when EggDone is already installed or running. It uses the real current-user installation path and registry.
- Tauri 2.11.2's built-in `allowDowngrades: false` was insufficient for silent installation on this machine. Preserve `installer-hooks.nsh`.
- The handoff scripts need `PYTHONUTF8=1` on this Windows machine because Chinese Git commit messages can fail under GBK decoding.

## Environment State

## Tools/Services Used

- Windows PowerShell
- Node.js 24.16.0
- pnpm 11.3.0
- Rust 1.94.0 stable, MSVC target
- Tauri 2.11.2
- Svelte 5, SvelteKit, Vite 6, Vitest 4
- SQLite through bundled `rusqlite`
- S3-compatible storage support suitable for AWS S3 and MinIO
- Windows NSIS current-user installer

## Active Processes

- No EggDone installation, EggDone process, or installer residue remained after the latest installer verification.
- No project dev server or Cargo build was intentionally left running by this session.

## Environment Variables

- `TAURI_DEV_HOST` is supported by the Vite configuration.
- `CARGO_TARGET_DIR` can be set for isolated build output.
- `PYTHONUTF8=1` is required for reliable handoff script execution with Chinese Git history.
- No credential, S3, MinIO, signing, or updater secret values are recorded here.

## Verification Status

The latest full check passed:

```text
pnpm release:check
```

Results:

- Svelte check: 0 errors, 0 warnings.
- Frontend tests: 10 passed.
- Frontend production build: passed.
- `cargo fmt -- --check`: passed.
- `cargo check`: passed.
- Rust tests: 37 passed.

Windows installer automation also passed:

```text
基础版本安装通过
覆盖升级通过：0.1.1
降级阻止通过：安装器退出码 2，当前版本 0.1.1
卸载通过
```

The current uncommitted changes are documentation only, so code checks were not rerun after creating `FUNCTION_OPTIMIZATION_TODO.md`.

## Related Resources

- [Project README](../../README.md)
- [Engineering optimization roadmap](../../OPTIMIZATION_TODO.md)
- [Feature optimization roadmap](../../FUNCTION_OPTIMIZATION_TODO.md)
- [Windows release guide](../../docs/RELEASING_WINDOWS.md)
- [Manual regression checklist](../../docs/MANUAL_REGRESSION.md)
- [Previous P2 handoff](./2026-06-07-101812-eggdone-p2-desktop-integration.md)

---

**Security check required**: validate this document before using it as the next-session source of truth.
