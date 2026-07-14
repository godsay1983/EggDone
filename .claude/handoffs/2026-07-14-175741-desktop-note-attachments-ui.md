# Handoff: EggDone Desktop Note Image Attachments

## Session Metadata
- Created: 2026-07-14 17:57:41
- Project: `D:\Develop\EggDone`
- Branch: `main` (`main...origin/main`)
- Version: `1.0.3`
- Milestone: note image attachment protocol, persistence, S3 transfer, metadata sync, and DA5 UI MVP

### Recent Commits
- `154a355 feat: 实现笔记附件（图片）支持`
- `2e2f846 feat: 实现附件二进制上传与元数据同步，并整合到手动同步流程`
- `3cd6913 feat: 实现笔记附件的S3上传、下载和删除功能`
- `86d9908 feat: 实现笔记附件本地资产存储与图片处理`
- `e690d16 feat: 实现便签附件数据库与同步协议`

## Handoff Chain
- Continues from: [2026-07-14-120440-desktop-notes-ui-v1-0-3.md](./2026-07-14-120440-desktop-notes-ui-v1-0-3.md)
- Supersedes: none; read the previous handoff for the completed text-note and release context.

## Current State Summary

The desktop client now supports image attachments on notes end to end: local image import and preview generation, attachment metadata persistence, binary S3 transfer, versioned metadata synchronization, editor and card presentation, delete/retry, drag and drop, and clipboard paste. All implementation is committed and the working tree was clean before this handoff file was created. DA5 is functional but not fully closed because precise upload progress, delete undo, sorting, keyboard acceptance, and final visual regression remain unchecked. DA6 remote-only on-demand download and cache management is the next major implementation stage and must stay protocol-compatible with the Harmony client.

## Architecture Overview

- Rust owns attachment validation, local asset storage, database writes, S3 binary transfer, metadata merge, and Tauri commands.
- Svelte owns file selection, drag/drop/paste, object URL lifecycle, editor/card presentation, and orchestration through `TodoPanel.svelte`.
- `note-attachments.json` is synchronized separately from `notes.json`; immutable original/preview objects use UUID-derived keys.
- Todo, note, and attachment synchronization share the existing mutual-exclusion and dirty-state flow.
- Empty notes remain visually unchanged; attachment UI is rendered only when a note has attachments.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md` | Cross-device design and protocol decisions | Read before changing schema or object keys |
| `docs/NOTES_ATTACHMENTS_ROADMAP.md` | DA0-DA8 progress | Authoritative remaining-work checklist |
| `src-tauri/src/note_attachments.rs` | Attachment model, validation, and repository behavior | Local and synchronized metadata semantics |
| `src-tauri/src/note_asset_store.rs` | Safe import, preview generation, file validation, and cleanup | Local image pipeline |
| `src-tauri/src/note_attachment_sync.rs` | Attachment document merge and synchronization | Must remain compatible with ArkTS |
| `src-tauri/src/s3_sync.rs` | Binary S3 operations | Immutable upload, checked download, and delete behavior |
| `src-tauri/src/commands.rs` | Tauri attachment and synchronization commands | Backend-to-frontend boundary |
| `src/lib/api/noteAttachmentApi.ts` | Typed attachment invoke wrapper | Frontend API entry point |
| `src/lib/components/NoteEditor.svelte` | Picker, drop/paste, grid, viewer, retry, and delete UI | DA5 main UI |
| `src/lib/components/TodoPanel.svelte` | Attachment state, note persistence, preview URLs, and auto-sync | Main orchestration point |
| `src/lib/components/NoteCard.svelte` | First preview and image-count summary | Note list presentation |

## Work Completed

- [x] Frozen the cross-device attachment v1 protocol and shared fixtures.
- [x] Added attachment database migration, repository, strict model, tombstones, and local transfer state.
- [x] Added safe local image import for JPEG, PNG, and WebP plus 512px JPEG previews.
- [x] Added binary S3 upload/download/HEAD/delete and attachment metadata synchronization.
- [x] Integrated attachment work into the existing synchronization mutex and status reporting.
- [x] Added image selection, drag/drop, clipboard paste, two-column previews, viewer, save-original link, retry, and delete.
- [x] Added note-card first preview and image count without changing cards that have no attachments.
- [x] Added blank-draft auto-title from the first file name, falling back to `图片便签`.
- [x] Limited each note to nine images and trigger existing auto-sync after attachment changes.

## Decisions Made

| Decision | Alternatives | Rationale |
|----------|--------------|-----------|
| Keep metadata separate from `notes.json` | Embed attachments in notes | Preserves old-client compatibility and independent binary lifecycle |
| Copy/import bytes into app data immediately | Keep external file paths | Survives source moves, restarts, and permission changes |
| Publish metadata only after binaries succeed | Publish pending metadata first | Prevents another device from seeing references to missing objects |
| Hide attachment regions when empty | Always show an empty attachment card | Keeps the original compact note workflow unchanged |
| Reuse existing auto-sync orchestration | Add a second background sync loop | Prevents concurrent synchronization and inconsistent status |

## Immediate Next Steps

1. Perform Windows acceptance in light/dark themes and narrow tray width: select, drop, paste, view, save, delete, retry, restart, and sync an image note.
2. Close remaining DA5 items: precise upload/progress feedback, short delete undo, attachment sorting, keyboard focus/operation, and storage-layer image fixtures.
3. Run a real desktop-to-Harmony S3/MinIO round trip, then implement DA6 preview-on-demand, original-on-view, deduplicated download, and cache cleanup.

## Pending Work

- DA2: storage-layer unit tests and sample image fixtures.
- DA3: real AWS S3 and at least one MinIO-compatible service validation.
- DA5: progress, undo, sorting, final theme/narrow/keyboard acceptance.
- DA6: remote-only previews, original on demand, cache statistics/cleanup, tombstone cleanup, and settings readouts.
- DA7: safe generic file attachments.
- DA8: complete backup, cross-device regression, release documentation, and next version bump.

## Blockers and Open Questions

- No code blocker is known.
- Real object-storage acceptance requires the user's configured S3 or MinIO account and a Harmony device/build using the same object keys.
- UX for manual image ordering and the duration of delete undo is not yet frozen; follow existing compact note controls.

## Important Context

- Do not redesign the attachment protocol independently on desktop. Every metadata or object-key change must be mirrored in `D:\Develop\EggDoneHarmony`.
- The UI code in commit `154a355` is implemented and committed; do not repeat the DA5 picker/grid work.
- `TodoPanel.svelte` owns attachment orchestration and object URL cleanup. Keep reusable note components free of direct Tauri calls.
- Attachment create/delete/retry marks synchronization dirty through the existing flow. Do not create a parallel attachment timer.
- Current verification covered compilation and automated tests, but not a new manual Windows visual/device pass after the final UI commit.
- The repository was clean and aligned with `origin/main` before this handoff artifact was generated.

## Potential Gotchas

- Revoke preview/original object URLs when replacing or unmounting them or repeated editing will leak browser memory.
- A blank draft must be persisted before its first attachment because attachment rows require the parent note UUID.
- Never publish attachment metadata before both original and preview uploads are confirmed.
- Preserve tombstones; immediate remote deletion would break offline conflict recovery.
- Keep binary bytes out of logs and never put synchronization credentials in source, fixtures, or handoff files.

## Verification Snapshot

- `pnpm check`: passed with 0 errors and 0 warnings.
- `pnpm test`: 9 test files and 46 tests passed.
- `pnpm build`: passed.
- `cargo fmt`: run successfully.
- `cargo check`: passed; only the existing dead-code warnings for `set_sort_order` and `restore` remained.
- `cargo test`: 95 tests passed.
- No dev server or watcher is intentionally left running.

## Environment Notes

- Package manager: pnpm.
- Desktop runtime: Tauri 2 plus Rust.
- Relevant configuration is stored through the application's existing settings and secure credential path; no credentials are recorded here.
- Useful release command: `pnpm release:check`.

## Related Resources

- [Desktop attachment roadmap](../../docs/NOTES_ATTACHMENTS_ROADMAP.md)
- [Desktop implementation plan](../../docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md)
- Harmony counterpart: `D:\Develop\EggDoneHarmony\docs\HARMONY_NOTES_ATTACHMENTS_ROADMAP.md`
- Shared fixture locations are documented in the implementation plan and synchronization test files.
