# Handoff: EggDone Desktop Note Attachment On-Demand Cache

## Session Metadata
- Created: 2026-07-14 22:44:22
- Project: `D:\Develop\EggDone`
- Branch: `main` (`main...origin/main`)
- Version: `1.0.3`
- Milestone commit: `d51584a feat: 实现附件按需下载与缓存管理`
- Scope: desktop DA6 preview/original on-demand download and local cache states

### Recent Commits
- `d51584a feat: 实现附件按需下载与缓存管理`
- `f69029c feat: 实现附件手动排序及元数据同步`
- `35258b5 feat: 实现附件删除与6秒撤销功能`
- `2b12e99 docs: 添加笔记附件UI交接文档`
- `154a355 feat: 实现笔记附件（图片）支持`

## Handoff Chain
- Continues from: [2026-07-14-175741-desktop-note-attachments-ui.md](./2026-07-14-175741-desktop-note-attachments-ui.md)
- Supersedes: none. Read the previous handoff for DA0-DA5 protocol, storage, synchronization, and UI context.

## Current State Summary

Desktop DA6 now supports remote image assets without downloading every binary at application startup. The notes list downloads only the first visible preview per note, the editor loads the selected note previews, and the viewer, save, or share path downloads the original only when requested. Repository state now distinguishes remote-only, downloading, cached, and failed assets; remote download failures are not requeued as uploads. On-demand transfers use the same global synchronization lock as manual and automatic synchronization. The implementation is committed in `d51584a`; the working tree is clean except for this newly generated handoff file.

## Architecture Overview

- `TodoPanel.svelte` decides which previews are currently needed and owns object URL creation and revocation.
- `NoteEditor.svelte` remains presentation-focused and requests an original through its callback only when the viewer opens.
- Async Tauri read commands first verify the app-private cache and otherwise download the immutable S3 object.
- `NoteAssetStore` performs verified atomic writes; expected size and SHA-256 come from attachment metadata.
- `note_attachments.rs` persists local paths and transfer states while keeping download failures out of the upload queue.
- `SyncRuntime` provides one global lock for synchronization and on-demand downloads plus per-attachment UUID transfer deduplication.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md` | Cross-device attachment protocol | Do not change object keys or metadata independently |
| `docs/NOTES_ATTACHMENTS_ROADMAP.md` | DA0-DA8 progress | DA6 cache cleanup is the next implementation stage |
| `src-tauri/src/commands.rs` | Async preview/original commands and download orchestration | Main backend entry for on-demand reads |
| `src-tauri/src/note_attachments.rs` | Cache-state persistence and pending-transfer query | Prevents remote download failures from becoming uploads |
| `src-tauri/src/note_asset_store.rs` | Verified local asset reads and atomic downloaded writes | Protects cache integrity |
| `src-tauri/src/s3_sync.rs` | Global sync lock, per-UUID guard, checked binary download | Concurrency and S3 boundary |
| `src/lib/components/TodoPanel.svelte` | Visible-preview selection, attachment event refresh, retry | Main frontend orchestration |
| `src/lib/components/NoteEditor.svelte` | Transfer-state labels and original viewer callback | User-facing cache state |

## Work Completed

- [x] Converted preview and original read commands to async cache-or-download operations.
- [x] Downloaded assets are validated by size and SHA-256 before atomic cache writes.
- [x] Added repository transitions for cached files and failed downloads.
- [x] Excluded `failed + remote_uploaded` rows from the pending upload query.
- [x] Made remote download retry local-only; upload failures still return to `pending_upload` and schedule synchronization.
- [x] Limited list-view loading to the first image preview for each visible note.
- [x] Kept all-preview loading for the selected note editor.
- [x] Delayed original download until viewer, save, or share actions.
- [x] Prevented attachment state events from downloading all previews for a closed note.
- [x] Acquired the global `SyncRuntime` guard before an on-demand network transfer.
- [x] Added a Rust test covering preview cache, original failure, pending-queue exclusion, and final cached state.
- [x] Updated DA6 roadmap status.

## Files Modified

| File | Changes |
|------|---------|
| `docs/NOTES_ATTACHMENTS_ROADMAP.md` | Marked on-demand preview/original loading and cache states complete |
| `src-tauri/src/commands.rs` | Added async cache-or-download commands, event refresh, retry semantics, and global sync locking |
| `src-tauri/src/note_attachments.rs` | Added cache/failure persistence, upload-queue exclusion, and repository tests |
| `src/lib/components/NoteEditor.svelte` | Added downloading/cached/processing state labels |
| `src/lib/components/TodoPanel.svelte` | Added visible first-preview selection, metadata-only event refresh, and remote retry behavior |

## Decisions Made

| Decision | Alternatives Considered | Rationale |
|----------|-------------------------|-----------|
| Download only the first preview for visible cards | Download every attachment preview at startup | Reduces startup traffic and memory for large note collections |
| Download originals only for explicit actions | Cache originals together with previews | Preserves bandwidth and storage while keeping viewing predictable |
| Keep failed remote downloads out of upload work | Reuse the generic failed transfer queue | Remote immutable content must be fetched, not overwritten from missing local bytes |
| Share the global synchronization lock | Allow S3 reads alongside metadata sync | Avoids races between metadata merge, binary upload, and cache download |
| Emit metadata refresh events during state changes | Mutate frontend state optimistically only | Keeps multiple windows and asynchronous command results consistent |

## Immediate Next Steps

1. Implement desktop local attachment cache statistics and cleanup, preserving every original whose binary has not been uploaded.
2. Add transactional attachment soft deletion when deleting a parent note, then verify tombstone merge and restore behavior across both clients.
3. Add the settings readouts and cleanup entry, then validate a real desktop-to-Harmony S3/MinIO round trip including cache deletion and re-download.

## Pending Work

- DA5 manual acceptance remains for light/dark themes, narrow tray width, and keyboard-only operation.
- DA6 still needs cache size calculation, safe local cleanup, note cascade soft deletion, 30-day remote tombstone cleanup, missing `DeleteObject` degradation, and settings readouts.
- DA7 generic safe file attachments has not started.
- DA8 complete backup, adverse-network regression, cross-device acceptance, documentation, and release versioning remain.

## Blockers/Open Questions

- No code blocker is known.
- Real download, retry, and cache-clear acceptance requires the user's configured S3 or MinIO service and a Harmony build using the same keys.
- The cache size limit and cleanup policy are not yet frozen; preserve pending local originals regardless of the selected policy.

## Important Context

- Commit `d51584a` already contains the desktop on-demand implementation. Do not repeat the DA6 download work.
- Keep desktop and Harmony metadata, object-key derivation, transfer-state meaning, and retry behavior synchronized.
- Preview cache success may remain `remote_only` when the original is not local. That state is intentional: the preview is cached while the original remains remote.
- Local download state must not mark synchronization dirty. Only local metadata changes that need publication should schedule the existing auto-sync path.
- The `note-attachments-changed` listener refreshes all previews only for the currently selected note; closed-note events refresh metadata and let the visible-card selector request only the first preview.
- The backend first verifies an existing local file. Network and S3 configuration are consulted only after local verification fails and `remote_uploaded` is true.
- The global `SyncRuntime` guard must remain held across S3 download and atomic cache persistence.
- The repository is otherwise clean and aligned with `origin/main`; only this handoff artifact is untracked after creation.

## Assumptions Made

- Attachment original and preview objects are immutable after upload and continue using UUID-derived keys.
- Remote metadata supplies trusted expected size and SHA-256 only after strict document validation.
- A failed remote fetch is recoverable through retry and does not imply that local metadata should be uploaded.

## Potential Gotchas

- Revoke replaced or unused browser object URLs or repeated note navigation will leak memory.
- Do not load all previews from the global attachment-change listener; that silently defeats on-demand behavior.
- Acquire the global synchronization guard before changing state to `downloading`, so a rejected concurrent request does not leave a stale status.
- Cache cleanup must never delete a local original with `remote_uploaded = false`.
- Preserve attachment tombstones long enough for offline devices; do not immediately delete remote binary objects on UI deletion.
- Never place S3 credentials, binary content, or signed URLs in logs, fixtures, or handoff files.

## Verification Snapshot

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: passed.
- Focused attachment tests: 5 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml`: 97 passed.
- `pnpm check`: passed with 0 errors and 0 warnings.
- `pnpm test`: 9 files and 46 tests passed.
- `pnpm build`: passed with static adapter output generated.
- `git diff --check`: passed before commit; only normal Windows line-ending notices appeared.
- No real S3/MinIO or manual Windows visual acceptance was performed in this session.

## Environment State

### Tools/Services Used
- pnpm and SvelteKit for frontend checks and build.
- Rust/Cargo and Tauri 2 for backend tests.
- Session handoff scripts run with Python UTF-8 mode.

### Active Processes
- No dev server, watcher, Cargo test, or background helper is intentionally left running.

### Environment Variables
- `PYTHONUTF8` was used for handoff tooling.
- S3 credential variable values are not recorded and were not needed for automated verification.

## Related Resources

- [Desktop attachment roadmap](../../docs/NOTES_ATTACHMENTS_ROADMAP.md)
- [Desktop implementation plan](../../docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md)
- [Previous desktop attachment handoff](./2026-07-14-175741-desktop-note-attachments-ui.md)
- Harmony counterpart roadmap: `D:\Develop\EggDoneHarmony\docs\HARMONY_NOTES_ATTACHMENTS_ROADMAP.md`

---

**Security Note**: This document contains no credentials, access keys, tokens, signed URLs, or signing passwords.
