# Handoff: EggDone Desktop Sync Runtime NS1

## Session Metadata

- Created: 2026-09-05 10:42:04
- Project: `D:\Develop\EggDone`
- Branch: `main`
- Session duration: current 2026-09-05 implementation session
- Current version: `1.0.7`
- Working tree at handoff creation: clean before this handoff document

### Recent Commits

- `13b50e7 feat(sync): persist runtime diagnostics`
- `048a716 docs(next): close desktop baseline stage`
- `a01ff56 docs(next): define desktop reliability and workflow roadmap`
- `c7a2531 docs(handoffs): 新增Linux构建与ksni托盘左键修复会话交接文档`
- `5e13c61 chore(release): 版本升级至 1.0.7`

## Handoff Chain

- **Continues from**: [2026-07-25-130400-desktop-linux-ksni-tray-107.md](./2026-07-25-130400-desktop-linux-ksni-tray-107.md)
- **Supersedes**: None. Older handoffs remain historical evidence; this is the current desktop checkpoint.

## Current State Summary

The desktop repository has completed DNS0 and the code/automated-test portion of DNS1 from the new reliability roadmap. Commit `13b50e7` adds persistent local sync runtime state, per-domain dirty tracking for todos, notes, and attachments, sanitized diagnostics, restart-aware UI state, and protection against clearing edits made during an upload. The full desktop release check passes. Real S3/MinIO failure recovery, forced-exit UI recovery, and narrow bilingual visual acceptance are intentionally still manual gates. The next feature stage is DNS2 smart lists, but DNS1 manual evidence should be recorded first when the required environments are available.

## Codebase Understanding

## Architecture Overview

- SvelteKit owns the panel UI and in-memory orchestration; Tauri commands are the typed boundary to Rust.
- Rust owns SQLite migrations, business persistence, synchronization, attachments, reminders, tray behavior, and diagnostics persistence.
- `sync_runtime_state` is a single-row, device-local table. It is not part of Todo/Note sync documents, JSON export, or `.eggdone-backup`.
- SQLite triggers mark dirty domains at the storage boundary so writes from UI, tray, reminders, imports, or future entry points cannot silently bypass sync status.
- A monotonically increasing revision exists per dirty domain. A sync attempt clears a domain only when its current revision still matches the revision captured at attempt start.

## Critical Files

| File | Purpose | Relevance |
| --- | --- | --- |
| `docs/NEXT_STAGE_IMPLEMENTATION_PLAN.md` | Product and architecture plan for NS0-NS7 | Defines shared semantics and non-goals |
| `docs/NEXT_STAGE_ROADMAP.md` | Executable desktop checklist | Source of truth for completed and manual items |
| `src-tauri/src/db.rs` | SQLite schema and migrations | Schema 16 and dirty-domain triggers live here |
| `src-tauri/src/sync_runtime_state.rs` | Persistent runtime snapshot and state transitions | Central implementation of attempts, results, revisions, and sanitized errors |
| `src-tauri/src/commands.rs` | Tauri commands and S3 sync orchestration | Records attempt, per-domain success, final success, and failure |
| `src/lib/api/syncApi.ts` | Strict frontend DTO and command wrappers | Exposes `SyncRuntimeSnapshot` safely to Svelte |
| `src/lib/sync/autoSync.ts` | Foreground/manual sync orchestration | Restores status from persistence and refreshes it after attempts |
| `src/lib/components/SyncSettings.svelte` | Sync settings and diagnostics UI | Shows timestamps, domain state, pending attachments, retry, and safe copy |
| `src/lib/components/TodoPanel.svelte` | Main panel composition | Hosts the compact persisted sync-status entry |

## Key Patterns Discovered

- Keep synchronized business records and local operational state separate.
- Mark dirty state in database triggers instead of relying on every Svelte event handler.
- Treat partial success as partial success: a completed Todo upload must not clear attachment dirty state.
- Persist stable error codes and sanitized summaries; localize display text in the frontend.
- Automated success is not evidence for Windows high-DPI, multi-monitor, notification, real object-storage, or cross-platform acceptance.

## Work Completed

### Tasks Finished

- [x] Created and committed the desktop next-stage implementation plan and roadmap.
- [x] Closed DNS0 baseline documentation and release-check evidence.
- [x] Migrated SQLite from schema 15 to schema 16.
- [x] Added persistent sync attempts, success timestamps, dirty domains, results, safe errors, and pending attachment count.
- [x] Added per-domain revisions to preserve edits created while a previous revision is uploading.
- [x] Integrated state recording into the existing Todo, Note, and attachment synchronization sequence.
- [x] Added a compact status pill and bilingual diagnostics view without adding another permanent toolbar row.
- [x] Added migration, reopen, partial-failure, concurrent-write, and credential-redaction tests.
- [x] Ran the full release check successfully.

## Files Modified

| File group | Changes | Rationale |
| --- | --- | --- |
| `src-tauri/src/db.rs`, `src-tauri/src/sync_runtime_state.rs` | Schema, triggers, state functions, tests | Make sync truth survive process restarts and cover all write paths |
| `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` | Sync lifecycle integration and command registration | Persist state at the actual synchronization boundary |
| `src/lib/api/syncApi.ts`, `src/lib/sync/autoSync.ts` | Typed snapshot query and restoration | Stop treating an in-memory store as the only state source |
| `src/lib/components/SyncSettings.svelte`, `src/lib/components/TodoPanel.svelte`, `src/app.css` | Diagnostics and compact status UI | Make failures understandable without exposing credentials |
| `src/lib/i18n/locales/zh-CN.ts`, `src/lib/i18n/locales/en-US.ts` | Bilingual labels and stable error text | Keep new user-visible UI internationalized |
| `docs/NEXT_STAGE_*.md` | Implementation record and checklist status | Separate completed code from pending manual acceptance |

## Decisions Made

| Decision | Options Considered | Rationale |
| --- | --- | --- |
| Local-only runtime table | Upload status to S3 vs store locally | Runtime state is device-specific and must not overwrite another device's status |
| Database triggers for dirty marking | UI-only calls vs command wrappers vs triggers | Triggers cover every persistence path and reduce future omissions |
| Per-domain revision counters | Clear dirty after any successful upload vs compare revisions | Revision checks prevent an old upload from clearing a newer edit |
| Stable error codes plus sanitized message | Persist raw exceptions vs generic boolean vs sanitized diagnostics | Gives support value while excluding credentials, request signing, and file content |
| Preserve dirty when sync is disabled | Clear UI state vs retain local truth | Disabled sync does not mean data reached remote storage |

## Pending Work

### Blockers/Open Questions

- [ ] No code blocker. Real S3/MinIO endpoints and platform UI/device environments are needed for the remaining acceptance evidence.
- [ ] Decide whether to close DNS1 manual checks before starting DNS2, or run them alongside DNS2 development. Do not mark them complete without evidence.

### Deferred Items

- Windows bilingual light/dark and minimum-width visual verification.
- Forced process exit followed by restart with unsynchronized local edits.
- Real offline, invalid-credential, ETag-conflict, attachment-failure, and retry-success flows.
- Existing `TraySnapshot.locale` dead-code warning; it is non-blocking and should be handled only with related tray work.
- Windows signing/auto-update, Linux ksni packaging, and macOS menu-bar behavior remain DNS7 release gates.

## Important Context

- `main` contains all current work; no push was performed during this session.
- Version remains `1.0.7`; this stage intentionally did not create a release version bump.
- The implementation commit is `13b50e7`. The handoff documentation commit will follow it.
- Do not add sync-runtime fields to S3 documents, JSON export, or full backup. They describe only this installation.
- `dirty_domains` and internal revision columns have different purposes: domains are user-visible state, revisions are concurrency guards.
- Attachment triggers deliberately ignore local cache/transfer-only updates that should not create remote metadata churn.
- A global `success` result can coexist briefly with a still-dirty domain when a newer edit occurred during the attempt; UI logic must prioritize dirty domains over the last result.
- Never copy Access Key, Secret Key, Authorization headers, signatures, or file contents into diagnostics, logs, tests, or handoffs.
- The previous Linux handoff is useful for platform release context, but its build/cache paths should be revalidated before reuse.

## Assumptions Made

- The existing S3/MinIO Todo, Note, and attachment object formats remain unchanged.
- One SQLite connection protected by the application database lock serializes local state updates used by the revision check.
- Smart-list selection in DNS2 will remain local preference data and will not alter Todo DTOs or synchronization.

## Potential Gotchas

- Do not clear all dirty domains in a final success handler; each domain is cleared only after its own remote step succeeds.
- Do not replace database trigger coverage with frontend calls when refactoring UI.
- Keep `sync_runtime_state` migration idempotent for both new and upgraded databases.
- The diagnostics clipboard intentionally contains endpoint, bucket, and object-key metadata. These are allowed by the current whitelist, but credentials are not.
- `pnpm build` success does not validate the native tray, notifications, high DPI, or a packaged installer.

## Immediate Next Steps

1. Run and record DNS1 manual acceptance: create local edits, force-exit/restart, then verify the persisted pill and diagnostics before any successful sync clears them.
2. Exercise real S3/MinIO offline, invalid-credential, ETag-conflict, partial attachment failure, and retry-success cases; update only the matching boxes in `docs/NEXT_STAGE_ROADMAP.md`.
3. Freeze the shared DNS2/HNS2 smart-list rules and date fixtures with the Harmony repository before implementing desktop `SmartViewId`, pure filters, counts, and the compact More-menu entry.

## Environment State

### Tools/Services Used

- Node.js and pnpm from the local development environment.
- Rust/Cargo and Tauri 2 toolchain.
- SQLite through `rusqlite`.
- Validation command: `pnpm release:check`.

### Verification Evidence

- i18n catalogs: 460 aligned keys; user-visible Svelte hardcode check passed.
- `svelte-check`: 0 errors and 0 warnings.
- Vitest: 13 files, 75 tests passed.
- Production SvelteKit build: passed.
- Cargo format and check: passed with the existing `TraySnapshot.locale` warning.
- Rust tests: 115 passed, 0 failed.
- Manual UI, packaged application, real S3/MinIO, and cross-device checks were not performed in this session.

### Active Processes

- None required for continuation.

### Environment Variables

- No project secret values were recorded in this handoff.
- Use only the existing local credential-storage flow for S3/MinIO credentials.

## Related Resources

- [Desktop next-stage roadmap](../../docs/NEXT_STAGE_ROADMAP.md)
- [Desktop next-stage implementation plan](../../docs/NEXT_STAGE_IMPLEMENTATION_PLAN.md)
- [Previous desktop handoff](./2026-07-25-130400-desktop-linux-ksni-tray-107.md)
- Harmony counterpart: `D:\Develop\EggDoneHarmony\docs\HARMONY_NEXT_STAGE_ROADMAP.md`

---

**Security Reminder**: This document intentionally contains no credentials or signing secrets. Re-run `validate_handoff.py` after future edits.
