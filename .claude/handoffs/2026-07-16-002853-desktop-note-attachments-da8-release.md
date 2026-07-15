# Handoff: EggDone Desktop Note Attachments DA8 and Release 1.0.4

## Session Metadata
- Created: 2026-07-16 00:28:53
- Project: `D:\Develop\EggDone`
- Branch: `main` (`main...origin/main` before this handoff)
- Version: `1.0.4` in the working tree
- Milestone commit: `0ff199f feat: 完成完整备份导入与原子恢复`
- Scope: complete binary backup export/import, archive validation, atomic restore, automated verification, and desktop version bump

### Recent Commits
- `0ff199f feat: 完成完整备份导入与原子恢复`
- `a98cbdd feat: 实现包含二进制的完整备份导出`
- `097db7c feat: 实现便签附件元数据的导入导出`
- `651b7db feat: 添加桌面便签附件DA7交接文档`
- `5b20c37 test: 增加存储层单元测试和样例图片 fixture`

## Handoff Chain
- Continues from: [2026-07-15-173934-desktop-note-attachments-da7.md](./2026-07-15-173934-desktop-note-attachments-da7.md)
- Supersedes: none. Read the previous handoff for DA0-DA7 persistence, S3 transfer, cache, file actions, ordering, and UI context.

## Current State Summary

Desktop note attachments have advanced through the implemented DA8 backup boundary. Ordinary JSON import/export carries attachment metadata only, while `.eggdone-backup` is a standard ZIP containing `manifest.json`, `data.json`, and validated `note-assets/` binaries. Export, secure preview, import, and rollback-capable restore are committed through `0ff199f`. The working tree now contains only the desktop version bump from `1.0.3` to `1.0.4` across the package, Tauri, Rust lock, and About UI surfaces, plus this handoff. Automated Rust and frontend verification is green. Remaining roadmap work is manual adverse-network, real S3/MinIO, cross-device delete/restore, HEIC preview, narrow/light/dark visual regression, and release documentation acceptance.

## Important Context

- Do not redesign the complete backup format independently from HarmonyOS. The shared contract is `docs/NOTES_ATTACHMENTS_BACKUP_FORMAT.md` in both repositories.
- `.eggdone-backup` is a standard ZIP with fixed safe paths. It is not JSON with embedded Base64 and it must not contain local cache paths.
- Import performs complete validation before durable writes: safe paths, no duplicates or symlinks, at most 10000 entries, at most 512 MiB uncompressed, sorted manifest paths, metadata references, byte sizes, and SHA-256 digests.
- Desktop restore stages binaries under `note-assets`, snapshots SQLite, swaps UUID directories by rename, merges data, and restores both database and files if any step fails.
- Ordinary JSON must keep `attachment_files_included: false`; a normal JSON import claiming binaries are included is rejected.
- The user-visible import entry is in the data management UI and shows task, note, attachment, verified file count, and byte totals before confirmation.
- Version `1.0.4` is not committed yet. Keep all five desktop version surfaces synchronized.
- Do not record S3 credentials, signed URLs, local attachment content, or private signing material in tests, logs, commits, or handoffs.

## Codebase Understanding

## Architecture Overview

- `src-tauri/src/data_exchange.rs` owns JSON exchange, complete backup ZIP creation/validation, import preview, SQLite snapshot/restore, and attachment directory rollback.
- `src-tauri/src/note_attachment_sync.rs` provides backup-safe attachment metadata and the existing cross-device conflict comparison used to decide which imported binaries win.
- `src-tauri/src/note_asset_store.rs` remains the canonical local attachment path and file safety boundary.
- `src/lib/api/dataApi.ts` exposes Tauri commands and typed preview/result payloads.
- `src/lib/components/DataManager.svelte` renders JSON and complete-backup import/export workflows without mixing their semantics.
- Existing note, Todo, group, and attachment merge rules are reused; DA8 did not introduce a new database schema or conflict algorithm.

## Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `docs/NOTES_ATTACHMENTS_BACKUP_FORMAT.md` | Shared backup protocol | Source of truth for both clients |
| `docs/NOTES_ATTACHMENTS_ROADMAP.md` | DA0-DA8 status | Tracks remaining manual and release work |
| `docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md` | Attachment architecture and constraints | Prevents protocol drift |
| `src-tauri/src/data_exchange.rs` | Export, validation, preview, restore, rollback | Main DA8 implementation |
| `src-tauri/src/lib.rs` | Tauri command registration | Registers full backup commands |
| `src/lib/api/dataApi.ts` | Frontend command types | Includes backup file counts and restored counts |
| `src/lib/components/DataManager.svelte` | Data management UI | Complete-backup user entry and preview |
| `package.json` | Frontend release version | Must remain `1.0.4` |
| `src-tauri/Cargo.toml` | Rust package version | Must remain `1.0.4` |
| `src-tauri/Cargo.lock` | Locked `eggdone` package version | Update only the `eggdone` entry |
| `src-tauri/tauri.conf.json` | Tauri bundle version | Must remain `1.0.4` |
| `src/lib/components/TodoPanel.svelte` | About dialog version | Displays `1.0.4` |

### Key Patterns Discovered

- Validate the entire archive and all digests before replacing any attachment directory.
- Use UUID directory renames for binary replacement and keep old directories in a rollback root until database work succeeds.
- Existing import merge functions commit their own transactions, so desktop atomic recovery uses a SQLite backup snapshot around the merge rather than wrapping all existing code in one nested transaction.
- Select restore candidates with the same attachment conflict comparator used by normal synchronization; do not overwrite a newer local attachment with older backup bytes.
- Count restored files separately from metadata rows because each image has an original plus a JPEG preview.

## Work Completed

### Tasks Finished

- [x] Added attachment metadata to ordinary JSON while explicitly excluding binaries.
- [x] Frozen the cross-client `.eggdone-backup` ZIP layout, manifest, limits, and hash rules.
- [x] Exported all active attachment originals and image previews with deterministic manifest entries.
- [x] Added secure archive parsing and rejection of unsafe, duplicate, unlisted, missing, oversized, or hash-mismatched entries.
- [x] Added full-backup import preview and confirmation commands.
- [x] Added SQLite snapshot plus attachment-directory rollback for atomic restore behavior.
- [x] Added validation tests for readable complete backups, unsafe paths, and invalid hashes.
- [x] Updated DA8 documentation and automated verification checkboxes.
- [x] Raised the desktop version from `1.0.3` to `1.0.4` in all release and About surfaces.

## Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| `docs/NOTES_ATTACHMENTS_BACKUP_FORMAT.md` | Frozen and marked complete backup boundaries | Keeps both clients compatible |
| `docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md` | Recorded complete backup implementation | Preserves architectural state |
| `docs/NOTES_ATTACHMENTS_ROADMAP.md` | Marked export/import and automation complete | Tracks DA8 accurately |
| `src-tauri/src/data_exchange.rs` | Added ZIP export, validation, preview, restore, rollback, and tests | Implements DA8 safely |
| `src-tauri/src/lib.rs` | Registered new commands | Exposes backend to UI |
| `src/lib/api/dataApi.ts` | Added full-backup payloads and commands | Keeps frontend strictly typed |
| `src/lib/components/DataManager.svelte` | Added full backup import/export and preview | Provides user workflow |
| `package.json` | Version `1.0.4` | Release metadata |
| `src-tauri/Cargo.toml` | Version `1.0.4` | Rust package metadata |
| `src-tauri/Cargo.lock` | `eggdone` version `1.0.4` | Lockfile consistency |
| `src-tauri/tauri.conf.json` | Version `1.0.4` | Installer metadata |
| `src/lib/components/TodoPanel.svelte` | About text `1.0.4` | User-visible consistency |

## Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Use standard ZIP with a custom extension | JSON Base64; opaque custom binary format | ZIP is inspectable and supported on both platforms without embedding large binaries in JSON |
| Validate before durable writes | Validate while copying into final paths | Prevents partial restore and malicious path/hash effects |
| Snapshot SQLite for rollback | Rewrite all merge services into one outer transaction | Reuses stable merge behavior while still recovering from later failures |
| Restore only winning attachment UUIDs | Replace every local attachment directory | Preserves newer local edits according to existing conflict rules |
| Keep normal JSON metadata-only | Add optional binaries to JSON | Maintains backward compatibility and manageable file size |

## Immediate Next Steps

1. Commit the five desktop `1.0.4` version files and this handoff after reviewing the diff; do not include build output.
2. Run a real cross-client backup matrix: desktop export to Harmony import, Harmony export to desktop import, and verify text, image previews, ordinary files, ordering, tombstones, and local paths after restart.
3. Complete DA8 adverse-network and real S3/MinIO acceptance, then update README, manual regression records, release notes, and the remaining roadmap checkboxes based on actual evidence.

## Pending Work

- DA3: validate AWS S3 and at least one MinIO-compatible service.
- DA5/DA7.1/DA7.2: complete narrow window, light/dark, keyboard, and mouse visual regression.
- DA8: validate offline creation, exit during upload, network interruption, bad credentials, and digest mismatch.
- DA8: validate desktop add, Harmony delete, desktop restore and HEIC/HEIF preview behavior.
- DA8: complete README, manual regression, version/release notes, and release packaging.

### Blockers/Open Questions

- No known code blocker remains.
- Real storage acceptance requires the user's configured S3 or MinIO service.
- Phone/tablet portions of the cross-client matrix require a current Harmony build and physical-device or representative emulator access.

### Deferred Items

- Rich document attachments, arbitrary executables, and a custom document viewer remain out of scope.
- Protocol v2 changes are deferred until v1 real-device backup and recovery acceptance is complete.

## Assumptions Made

- Attachment binary objects remain immutable and UUID-addressed.
- Both clients continue using the same v1 metadata, conflict, tombstone, and object-key rules.
- A complete backup is intended for migration/recovery, not as a live two-way sync mechanism.

## Potential Gotchas

- Do not globally replace every `1.0.3` in `Cargo.lock`; only the `[[package]] name = "eggdone"` entry belongs to the application version.
- Backup staging strips the `note-assets/` prefix so installation expects `<staging>/<uuid>/original`; keep export and import path handling aligned.
- `set_restored_attachment_paths` must run only for active winning imported attachments.
- Clean both staging and rollback directories on success and failure, but never remove a rollback root before database success.
- Preview byte totals are uncompressed validated content, not the compressed archive size.
- Existing unrelated working-tree changes must not be reverted if they appear after this handoff.

## Verification Snapshot

- `cargo test --manifest-path src-tauri/Cargo.toml`: 107 tests passed.
- `pnpm test`: 10 files and 50 tests passed.
- `pnpm check`: 0 errors and 0 warnings.
- `pnpm build`: passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: passed.
- `cargo check --manifest-path src-tauri/Cargo.toml`: passed for `eggdone v1.0.4` after the version bump.
- `git diff --check`: passed before handoff generation.
- No real S3/MinIO, full cross-device restore, or final visual matrix was performed in this session.

## Environment State

### Tools/Services Used
- pnpm, SvelteKit, Vite, Vitest, Rust/Cargo, Tauri 2, and Python handoff tooling.

### Active Processes
- No dev server, watcher, Cargo task, or background helper is intentionally left running.

### Environment Variables
- `PYTHONUTF8`
- S3-related environment values were not needed and are not recorded.

## Related Resources

- [Desktop attachment roadmap](../../docs/NOTES_ATTACHMENTS_ROADMAP.md)
- [Desktop attachment implementation plan](../../docs/NOTES_ATTACHMENTS_IMPLEMENTATION_PLAN.md)
- [Shared backup format](../../docs/NOTES_ATTACHMENTS_BACKUP_FORMAT.md)
- [Previous desktop handoff](./2026-07-15-173934-desktop-note-attachments-da7.md)
- Harmony counterpart: `D:\Develop\EggDoneHarmony\.claude\handoffs\2026-07-16-002853-harmony-note-attachments-ha8-release.md`

---

**Security Note**: This document contains no credentials, access keys, tokens, signed URLs, signing material, or private attachment data.
