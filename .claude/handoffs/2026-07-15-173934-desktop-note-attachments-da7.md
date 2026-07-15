# Handoff: EggDone Desktop Note Attachments DA7

## Session Metadata
- Created: 2026-07-15 17:39:34
- Project: `D:\Develop\EggDone`
- Branch: `main` (`main...origin/main`, ahead 0, behind 0 before this handoff)
- Version: `1.0.3`
- Milestone commit: `5b20c37 test: 增加存储层单元测试和样例图片 fixture`
- Scope: desktop attachment cache cleanup, generic files, attachment manager, kind-scoped ordering, card summary, and storage tests

### Recent Commits
- `5b20c37 test: 增加存储层单元测试和样例图片 fixture`
- `f49d89b style: 便签卡片操作按钮复用编辑器亮暗主题和危险操作配色`
- `483d4f8 feat: 实现附件按类型分组排序并更新卡片预览`
- `f8a35d6 feat: 重构附件管理，提升卡片和编辑器的附件展示与操作体验`
- `e847c53 feat: 实现普通附件上传、打开和文件管理`

## Handoff Chain
- Continues from: [2026-07-14-224422-desktop-note-attachments-on-demand-cache.md](./2026-07-14-224422-desktop-note-attachments-on-demand-cache.md)
- Supersedes: none. Read the previous handoff for DA0-DA5 protocol, persistence, S3 transfer, and on-demand cache context.

## Current State Summary

The desktop attachment implementation has advanced through DA6 and the main DA7 feature work. Local cache statistics and safe cleanup, note cascade deletion, 30-day remote cleanup, generic file attachments, a dedicated attachment manager, kind-scoped image/file ordering, compact note-card summaries, and storage fixtures are implemented and committed. The editor now preserves writing space even when many attachments exist. The repository was clean and synchronized with `origin/main` before this handoff was generated; this handoff file is the only expected untracked artifact.

## Codebase Understanding

## Architecture Overview

- `TodoPanel.svelte` owns attachment orchestration, object URL lifetime, note selection, and calls into Tauri attachment commands.
- `NoteEditor.svelte` renders the writing-first editor, compact attachment summary, and independent scrollable attachment manager.
- `NoteCard.svelte` renders only a compact first-image or first-file summary so attachments do not consume the note body area.
- `noteAttachmentOrder.ts` computes moves within an attachment kind. Images and ordinary files do not consume each other's previous/next boundaries.
- Rust command and repository layers remain authoritative for local paths, transfer state, cache cleanup, safe file validation, and S3 work.
- Attachment metadata and immutable binary objects still use the frozen cross-device v1 protocol shared with HarmonyOS.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md` | Cross-device protocol and constraints | Do not change metadata or object keys independently |
| `docs/NOTES_ATTACHMENTS_ROADMAP.md` | DA0-DA8 status | Source of remaining acceptance and release work |
| `src/lib/components/NoteEditor.svelte` | Editor and attachment manager UI | Main desktop attachment workflow |
| `src/lib/components/NoteCard.svelte` | Compact list preview | Keeps note cards readable with attachments |
| `src/lib/components/TodoPanel.svelte` | Frontend orchestration | Manages events, object URLs, and backend calls |
| `src/lib/utils/noteAttachmentOrder.ts` | Kind-scoped move calculation | Must stay compatible with Harmony ordering semantics |
| `src-tauri/src/note_asset_store.rs` | Image/file import, validation, fixtures, and cache | Security and storage boundary |
| `src-tauri/src/note_attachments.rs` | Attachment persistence and transfer state | Soft deletion, reorder, and cache path state |
| `src-tauri/src/commands.rs` | Tauri attachment commands | Save, open, share, cleanup, and on-demand operations |
| `src-tauri/src/note_attachment_sync.rs` | Metadata merge and cleanup eligibility | Cross-device tombstones and retention |

## Key Patterns Discovered

- Keep image and file ordering separate in UI while preserving one existing `sort_order` field and the v1 sync document.
- Keep local cache paths out of synchronized metadata.
- A remote download failure is fetch work, not pending upload work.
- Preserve pending local originals during cache cleanup whenever `remote_uploaded` is false.
- Revoke browser object URLs when cards, editors, or viewers replace content.
- Validate ordinary files by whitelist, extension, MIME, signature, and size before persistence or opening.

## Work Completed

### Tasks Finished

- [x] Added cache size statistics and safe cleanup for uploaded attachment binaries.
- [x] Soft-deleted active attachments when deleting a parent note.
- [x] Added 30-day tombstone-based remote cleanup with safe degradation when remote delete is unavailable.
- [x] Added settings readouts for attachment keys, pending counts, and cache cleanup.
- [x] Added ordinary file import, upload, download, save, open, and attachment management.
- [x] Enforced the safe file whitelist and 20 MiB maximum.
- [x] Reworked the editor into a writing-first layout with a separate attachment manager.
- [x] Grouped image and ordinary-file sorting without changing the protocol schema.
- [x] Reworked note cards to show a compact full-ratio preview and summary.
- [x] Unified note-card action colors with editor light/dark and destructive-action styling.
- [x] Added mixed-kind ordering tests and image/file storage fixture tests.

## Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| `docs/NOTES_ATTACHMENTS_ROADMAP.md` | Marked DA6, DA7, DA7.1, and DA7.2 work | Keeps implementation state explicit |
| `src-tauri/src/commands.rs` | Added cleanup and ordinary-file commands | Exposes safe backend operations |
| `src-tauri/src/note_asset_store.rs` | Added file validation, cache cleanup, and fixtures | Centralizes storage safety |
| `src-tauri/src/note_attachments.rs` | Added cascade and cache state operations | Keeps database behavior transactional |
| `src-tauri/src/note_attachment_sync.rs` | Added retention cleanup rules | Preserves offline tombstone safety |
| `src/lib/components/NoteEditor.svelte` | Added compact summary and attachment manager | Prevents attachments from shrinking the editor |
| `src/lib/components/NoteCard.svelte` | Added compact preview/summary | Improves list density and image framing |
| `src/lib/components/TodoPanel.svelte` | Updated attachment orchestration | Supports new manager and ordering workflow |
| `src/lib/utils/noteAttachmentOrder.ts` | Added kind-scoped moves | Separates image and file sorting |
| `src/app.css` | Added responsive attachment UI styling | Supports dark/light and narrow layouts |

## Decisions Made

| Decision | Alternatives Considered | Rationale |
|----------|-------------------------|-----------|
| Put detailed attachment operations in a manager | Keep every thumbnail and button in the editor | Preserves text space and avoids clipped controls |
| Sort within each attachment kind | Use one mixed visual sequence | Matches user expectations without a protocol migration |
| Keep cards to one preview summary | Render an attachment gallery in each card | Retains scan density and readable note content |
| Use strict file whitelist and signature validation | Accept arbitrary files by extension | Prevents unsafe or misleading attachments |
| Protect unsynced originals during cache cleanup | Clear all local attachment files | Prevents irreversible local data loss |

## Immediate Next Steps

1. Run real desktop-to-Harmony S3/MinIO acceptance for JPEG/PNG/WebP plus PDF, text, Office, and ZIP files, including cache clear and re-download.
2. Complete DA5/DA7.1/DA7.2 manual visual regression in light/dark themes, narrow tray width, mouse, and keyboard operation; fix only observed regressions.
3. Start DA8 by freezing `.eggdone-backup` requirements and defining how metadata plus binary objects are exported and restored across both clients.

## Pending Work

- DA3 still needs AWS S3 plus one MinIO-compatible real service validation.
- DA5 still has unchecked upload-state/failed-state visual acceptance and complete narrow/keyboard regression.
- DA7 still needs cross-device validation for PDF, text, Office, and ZIP.
- DA8 backup, adverse-network regression, cross-device delete/restore, HEIC preview acceptance, documentation, versioning, and release notes remain.

### Blockers/Open Questions

- No code blocker is known.
- Real cross-device acceptance requires the user's configured S3 or MinIO service and a current Harmony build using matching object keys.
- The complete backup container format is not yet frozen and must be designed jointly with HarmonyOS.

### Deferred Items

- Full binary backup is deferred to DA8 because its compatibility and recovery contract must be defined before implementation.
- Release version bump is deferred until real storage and visual acceptance pass.

## Important Context

- Do not repeat DA6 cache work or DA7 generic-file implementation; both are committed through `5b20c37`.
- Keep desktop and Harmony metadata schema, UUID object paths, conflict rules, transfer states, sort semantics, and tombstone retention synchronized.
- Images and files are visually grouped and moved independently, but the persisted schema remains unchanged.
- The editor attachment manager is intentional. Do not move all controls back into the main editor and reduce writing space.
- Cache cleanup may remove only recoverable uploaded binaries. Never delete a local original that has not reached remote storage.
- Ordinary file opening must remain behind type/signature validation; do not add executable or script types for convenience.
- Before this handoff, `main` matched `origin/main` at `5b20c37`; only the generated handoff is expected to make the worktree dirty.

## Assumptions Made

- Attachment binary objects remain immutable and UUID-addressed.
- The cross-device attachment v1 document remains backward compatible with clients that only synchronize notes.
- System-default file opening is acceptable only for validated supported types.

## Potential Gotchas

- A mixed image/file list must not use raw array neighbors for move enablement; use `noteAttachmentOrder.ts`.
- Do not let a global attachment-change listener eagerly download every preview.
- Keep object URL revocation in every replacement and teardown path.
- Remote cleanup must honor the full tombstone retention period and must not fail the main sync when delete permission is absent.
- Do not record credentials, signed URLs, binary contents, or local private data in logs or handoffs.

## Verification Snapshot

- `pnpm check`: passed with 0 errors and 0 warnings.
- `pnpm test`: 10 test files and 50 tests passed.
- `pnpm build`: passed; static adapter output generated.
- `cargo test --manifest-path src-tauri/Cargo.toml`: 104 tests passed.
- `git diff --check`: clean before handoff generation.
- No real S3/MinIO round trip or final Windows visual regression was performed during this handoff session.

## Environment State

### Tools/Services Used
- pnpm, SvelteKit, Vite, and Vitest for frontend verification.
- Rust/Cargo and Tauri 2 for backend verification.
- Session handoff scripts with Python UTF-8 mode.

### Active Processes
- No dev server, watcher, Cargo task, or background helper is intentionally left running.

### Environment Variables
- `PYTHONUTF8` was used for handoff tooling.
- S3-related variable names and values were not needed for automated verification and are not recorded.

## Related Resources

- [Desktop attachment roadmap](../../docs/NOTES_ATTACHMENTS_ROADMAP.md)
- [Desktop attachment implementation plan](../../docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md)
- [Previous desktop handoff](./2026-07-14-224422-desktop-note-attachments-on-demand-cache.md)
- Harmony counterpart: `D:\Develop\EggDoneHarmony\docs\HARMONY_NOTES_ATTACHMENTS_ROADMAP.md`

---

**Security Note**: This document contains no credentials, access keys, tokens, signed URLs, or signing material.
