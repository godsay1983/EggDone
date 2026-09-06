# Handoff: EggDone Desktop Preferences Persistence and Linux 1.0.10

## Session Metadata

- Created: 2026-09-06 23:49 (Asia/Shanghai).
- Repository: D:/Develop/EggDone, branch main, version 1.0.10.
- Source commit: 8325313 fix(settings): 持久化窗口与快捷键设置并修复重启恢复.
- This handoff is validated and committed separately; resolve its hash with git log.
- User asked for desktop handoff and local commit, not push or another version bump.

## Handoff Chain

Continues from: [Percentage control alignment](./2026-09-06-222747-desktop-zoom-control-alignment.md).
Earlier window feature context: [Desktop 1.0.10](./2026-09-06-220306-desktop-1-0-10-window-readability.md).

## Current State Summary

用户报告重启后窗口大小、缩放及快速便签快捷键未保持。本轮将这些偏好写入已有本机数据库，迁移旧 localStorage 设置，补充退出保存与失败保护。源码、测试、README 和同版本 CHANGELOG 已作为 8325313 提交。随后按用户要求完成 Linux 1.0.10 amd64 DEB/AppImage 打包，准确来说打包发生在该提交之前，来源为 49b4b79 加当时工作区修复的 ZIP 快照。当前用户要求交接并提交；本篇单独提交，不推送、不发布。鸿蒙端未修改。

## Important Context

- 原窗口与快捷键仅使用 WebView localStorage；窗口尺寸保存有250ms防抖。没有从真实进程复现证明用户两次丢失都由 localStorage 刷盘导致，不应把可能原因写成已证实根因。
- 已确认快捷键启动注册失败时，旧代码将返回的 enabled 状态改为 false，导致“设置丢失”的视觉误导；现在保留用户选择并返回独立错误。
- 新偏好保存在 app_metadata：main_window_preferences_v1、panel_shortcut_preferences_v1、note_shortcut_preferences_v1。数据库结构仍为 v17，无迁移、无新增同步字段。
- native 读取成功且无记录时才从旧 localStorage 迁移。native 读取失败不使用默认值覆盖已保存记录。没有清除用户数据或卸载应用。
- 主面板关闭/失焦仍隐藏到托盘；只有托盘退出结束进程。独立专注窗口不采用主窗口尺寸偏好。
- 真正原生重启、系统快捷键冲突、多显示器、Linux桌面验收仍未由用户确认；自动测试和成功打包不能关闭这些验收项。
- 本版保持1.0.10，关于页仍从包元数据读取；没有修改版本号、依赖锁文件、S3协议、任务数据或鸿蒙代码。
- 本机 package/ 已被Git忽略。不要强制把安装包、日志、源码ZIP、签名或本地数据库纳入Git。
- Linux构建容器 eggdone-linux-110-20260906 已停止并删除；没有遗留本轮编译会话或开发服务器。

## Architecture Overview

- Frontend window store handles UI state, serial native apply/save queue, display fitting and resize events.
- Window preference API performs native persistence and legacy migration; browser previews retain localStorage fallback.
- Native window_preferences module validates and stores logical dimensions/zoom using the existing Database mutex and parameterized SQL.
- lib.rs registers preference commands/state and calls native size flush on main CloseRequested and RunEvent::ExitRequested. Readiness prevents saving default startup geometry before successful restoration.
- desktopSettings manages shortcut registration and saved intent. Native shortcut_preferences stores panel/note records independently and validates the available combinations.
- Registration is attempted before saving an edited choice. Persistence/registration failures attempt to restore the previous actual registration; saved enabled intent is distinct from current registration success.
- Shortcut error strings use two new bilingual translation keys; catalog now has549 aligned keys.

## Critical Files

| File | Responsibility |
| --- | --- |
| src/lib/stores/windowPreferences.ts | Restore, apply/save serialization, resize persistence |
| src/lib/api/windowPreferencesApi.ts | Native window storage and legacy migration |
| src-tauri/src/window_preferences.rs | Window metadata, native exit size flush, reopen test |
| src/lib/api/desktopSettings.ts | Shortcut load/save, registration, rollback and error separation |
| src-tauri/src/shortcut_preferences.rs | Shortcut metadata, validation and reopen test |
| src/lib/api/desktopSettings.test.ts | Eight new shortcut persistence/registration regressions |
| src/lib/components/SettingsPanel.svelte | Preserve note shortcut error in parent settings |
| src-tauri/src/lib.rs | Native lifecycle and command registration |
| scripts/test-window-ui.mjs | Real Svelte controller with mocked native window/database transport |
| docs/WINDOW_READABILITY.md | Window behavior and manual acceptance checklist |
| CHANGELOG.md | Same-version fixes and Linux candidate evidence |

## Work Completed

- Window native storage, legacy migration, read/write failure protection and native close/exit size flush.
- Serialized resize writes against zoom/preset updates to avoid stale preference overwrites.
- Persisted both shortcut combinations and enable flags; startup failures no longer silently uncheck enable.
- Shortcut rollback on registration/save failures and idempotent repeated registration of the same active shortcut.
- Bilingual error messages, README and same-version changelog.
- Linux source snapshot, frozen-lockfile build, both bundles, metadata/content verification and checksum comparison after copying to Windows.

## Files Modified

Source commit 8325313 contains14 files: CHANGELOG, README, window documentation, window browser test, lib.rs, two native preference modules, two frontend preference APIs, shortcut tests, SettingsPanel, two locale catalogs and the window store. Four source/test files are new. This handoff is a separate documentation commit.

## Decisions Made

- Use existing app_metadata rather than a new schema or shared S3 preference object: sizes and OS shortcuts are device-specific.
- Keep actual native registration separate from saved enable intent; report conflicts without rewriting the user's selection.
- Keep old localStorage solely for migration/browser previews, not as the durable native source.
- Combine related local preference fixes in one source commit; separate the handoff.
- Do not retroactively label the Linux bundles as built from a clean commit. Source ZIP is the build authority, and this handoff/updated changelog came later.

## Validation Evidence

- Immediately before this handoff commit: pnpm test rerun, 163 tests across18 files passed.
- Preceding repair turn: pnpm check (0 errors/0 warnings), pnpm build, pnpm i18n:check (549 aligned keys), cargo fmt -- --check and cargo check passed.
- Windows Rust focused tests: cargo test window_preferences --lib and cargo test shortcut_preferences --lib each passed the SQLite close/reopen test. These are focused tests, not a full Rust suite run.
- Window browser test:16 language/theme/width combinations plus legacy migration, native storage restoration without old localStorage, error protection, display fitting, keyboard, resize and cleanup passed. Native transport is mocked.
- Eight added shortcut tests cover migration/fresh runtime, disabled restore, occupied startup/retry, read failure preservation, occupied update rollback, write failure including disable rollback, panel isolation, key-press dispatch and duplicate registration.
- Linux snapshot: pnpm check0/0, pnpm test163/18, frontend build and Tauri release bundling passed.
- Windows keeps existing TraySnapshot.locale unused warning; Linux release had11 existing unused-code warnings.
- git diff --cached --check passed. No new dependency or version changes.

## Linux Artifacts

Output directory: D:/Develop/EggDone/package/linux-1.0.10/.

- EggDone_1.0.10_amd64.deb: 12,747,774 bytes.
- EggDone_1.0.10_amd64.AppImage: 89,700,856 bytes.
- BUILD_INFO.md, SHA256SUMS.txt, build.log, verify.log, toolchain.log.
- SOURCE_STATUS.txt and SOURCE_SNAPSHOT.zip: preserve exact precommit inputs.
- Snapshot SHA256: 5c55b990d90f18df4080007a8d7dcda3e737c879af85ed31e03f212182995fd4.
- Base commit:49b4b79821ef0931b32af656996423dfaffb8884 plus the precommit fixes.
- Ubuntu22.04/glibc2.35, WebKitGTK4.1 2.50.4, GTK3.24.33; Node22.14.0, pnpm10.17.1, Rust1.98.0; CARGO_BUILD_JOBS=4, APPIMAGE_EXTRACT_AND_RUN=1.
- Commands: pnpm install --frozen-lockfile; pnpm check; pnpm test; pnpm tauri build --bundles deb,appimage -- --locked.
- Verified DEB version/amd64 metadata, payload MD5, desktop files, ELF architecture, shared-library resolution, AppImage extraction/AppRun, SHA256 before/after copying.
- Package checks do not establish Linux graphical/tray/Wayland compatibility. No signing or publication.
- Prior package/linux-1.0.9/ remains unchanged and does not contain these fixes.

## Immediate Next Steps

1. Confirm D:/Develop/EggDone, main, git status and latest source/handoff commits; do not accidentally work in the default Harmony cwd.
2. Ask the user to test actual client: choose Comfortable/125%, manually resize, tray Quit, relaunch and verify size/zoom; repeat quick hide/show and immediate exit after resizing.
3. Test quick-note and panel shortcut: enabled, changed combination, disabled, restart, actual trigger. With a conflicting program, verify preference remains enabled with an error, then retry after freeing the shortcut.
4. Test Linux packages on the intended distribution and desktop. First sha256sum -c SHA256SUMS.txt; use apt install for DEB or chmod +x for AppImage. App starts hidden: open through system tray.
5. Only mark native acceptance passed when the user confirms. Do not bump, rebuild, push or publish without a new request.

## Pending Work

No additional coding is planned for this handoff request. Outstanding work is real native acceptance and any subsequent issue reports. A future clean-commit release build should use its exact approved commit instead of reusing the precommit bundle provenance.

## Assumptions Made

- Latest user request is desktop handoff plus local commit only.
- No permission to modify Harmony, delete user preferences, push, or upload packages.
- Package version stays1.0.10 and historical acceptance records remain historical.

## Potential Gotchas

- Frontend reload/native mocks are not proof of OS process restart persistence.
- SQLite preference protection does not imply other localStorage-only application settings have all been migrated.
- Changing shortcut options later requires updating both frontend options and native allowlists.
- Normal exit flush is not a promise to handle forced termination or power loss mid-operation.
- Native read failures deliberately show errors rather than silently reset stored preferences.
- Native rollback is best-effort if unregister/re-register itself fails; user should not assume every OS conflict can be repaired automatically.
- Artifact SHA256 values are checksums, not credentials. No S3 credentials or user data are recorded here.

## Environment State

Windows/PowerShell, Docker Desktop Linux engine. Temporary build/toolchain/verify shell scripts and source ZIP remain under the user's Temp folder; the disposable Ubuntu container is removed. Existing unrelated Docker services were not changed. package/ remains ignored. No active tool sessions remain.

## Related Documents

- [Window operations](../../docs/WINDOW_READABILITY.md)
- [Changelog](../../CHANGELOG.md)
- [Quick capture contract](../../docs/QUICK_CAPTURE_CONTRACT.md)
- Local package build record: D:/Develop/EggDone/package/linux-1.0.10/BUILD_INFO.md (ignored, may not exist on another clone).
