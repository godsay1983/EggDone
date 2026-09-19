# Handoff: Desktop 1.3.0 Lifecycle Release

## Session Metadata

- Created: 2026-09-19 (Asia/Shanghai).
- Project: D:/Develop/EggDone.
- Branch: main. Both repositories were already on main; no merge, push or publication is requested.
- Current version: 1.3.0; paired desktop 1.3.0 / Harmony 1.4.0 (1000031, buildVersion 1).
- Scope: version metadata, release notes, accepted regression records and handoff. No new feature or migration in this release-maintenance turn.

## Handoff Chain

- Continues from [earlier activation handoff](./2026-09-19-023440-desktop-trash-space-activation.md), for historical evidence only.
- Supersedes that handoff's migration-first workflow and pending daily-plan/waiting development steps. They are no longer current requirements.
- Companion: [other client handoff](D:/Develop/EggDoneHarmony/.claude/handoffs/2026-09-19-201949-harmony-1-4-0-lifecycle-release.md).

## Current State Summary

用户要求桌面升至 1.3.0、鸿蒙升至 1.4.0，生成 handoff 并本地提交。本次已完成版本入口、CHANGELOG、README、roadmap 与验收记录更新及构建检查，交接校验后纳入本地提交；不推送、不发布、不安装覆盖应用。当前四块任务生命周期能力已实现：归档管理、直接清空回收站、今日计划、等待处理。用户已确认最近界面修复与计划/等待/恢复处理的实际跨端往返通过，不重复这些验收或重新拆后台阶段。

Prior committed implementation:
- This repository lifecycle regression: 660f3b6; waiting UI fix: 7212bdc.
- Desktop planning/workflow: a6b8abd / 44be4a7.
- Harmony planning/workflow: 3a5eeca / fd45f53.
- Current version/handoff commit is the commit containing this file; inspect git log to obtain its hash rather than embedding a self-referential hash.

## Important Context

清空回收站不再要求用户执行同步空间迁移、备份整个空间或选择内部 Object Key。本机先移除，终态同步和附件清理后台续作；正常同步自动发现、校验并接入可信既有位置，身份不明确时停止，不盲目合并。不要根据旧交接恢复迁移前置流程。同步设置默认只显示自动管理，路径编辑留在高级设置。

今日计划不改截止日期、提醒或主列表顺序；等待与计划独立，默认保留计划，用户勾选才原子移出。完成/归档/删除/恢复后的旧计划、等待不能迟到复活。清单勾选不自动完成主任务，关联便签不能随任务清理被误删。参与同步的设备一起升级，保留升级前备份，不让旧程序直接打开升级后的数据库。本次 schema 仍为 26；有等待域的导出为 v8，旧格式按内容和既有兼容规则处理。

User acceptance scope, exactly:
1. 桌面测试任务加入今日计划，设为等待但不移出计划，正常同步。
2. 鸿蒙同步后同时看到今日计划与等待处理标签，执行恢复处理并同步。
3. 桌面同步后等待标签消失、今日计划保留。
原因、复查日期、跨日、并发、所有设备/主题矩阵并未在此次反馈中逐项确认；不声称这些全部通过。归档/清空的以前确认见 roadmap，不重复基础验收。

## Architecture Overview

Svelte components/store call Tauri commands, with SQLite repositories, lifecycle fences and independent planning/workflow sync documents. The Rust journey tests are test-only child modules of commands and call actual commands/repositories. The ordinary frontend development instance must remain separate from browser-test Vite caches to avoid Svelte runtime duplication/blank windows.

## Critical Files

| File | Purpose |
| --- | --- |
| package.json | Frontend application version 1.3.0 and check/build commands |
| src-tauri/Cargo.toml | Rust package version 1.3.0 |
| src-tauri/Cargo.lock | Only local eggdone version updated; dependency versions unchanged |
| src-tauri/tauri.conf.json | Native app version 1.3.0 |
| src-tauri/src/task_lifecycle_journey_tests.rs | Four production command/repository lifecycle journeys |
| docs/TASK_LIFECYCLE_REGRESSION.md | Automated evidence, precise user acceptance and remaining scope |
| docs/TASK_LIFECYCLE_ROADMAP.md | Current checkpoints override historical relative wording |
| docs/WAITING_WORKFLOW_PROTOCOL.md | Waiting operation contract and acceptance |
| docs/DAILY_PLANNING_PROTOCOL.md | Daily planning semantics and usage |
| docs/AUTOMATIC_SYNC_TARGET.md | Automatic target discovery and protections |
| docs/TASK_DIRECT_PURGE.md | Current direct purge behavior |

## Files Modified

Updated package.json, Cargo.toml, Cargo.lock and tauri.conf.json from 1.2.0 to 1.3.0.
Both repositories update README current-version/latest-handoff entries, CHANGELOG dated release section, lifecycle roadmap, regression acceptance and this handoff. Existing historical release entries remain historical. There are no new business code, dependency, signing, database migration, cloud or user-data changes. Build artifacts remain ignored and are not staged.

## Validation

This version-maintenance turn:
- Desktop: pnpm check -> 0 errors / 0 warnings; pnpm test -> 486 passed across 58 files; pnpm build passed.
- Desktop: cargo fmt --all -- --check and cargo check --locked passed. cargo metadata plus JSON checks confirmed version 1.3.0 consistently.
- Harmony: devecocli build --product default --build-mode debug passed in 42.946 seconds (42 tasks, 32 executed, 10 up-to-date).
- Harmony: generated signed HAP module metadata and pack.info report 1.4.0 / 1000031, build 1. No install or native re-acceptance.
- Prior lifecycle regression: desktop Rust 481 passed, 40 intentionally ignored; Harmony 22 relevant scripts passed. Both repositories have four new full journeys, using new and reconstructed schema-25 upgrade databases. These suites were not all rerun after the version-only change.
- Handoff completeness/security validation must pass before commit; final score is reported in the user response.

Warnings remain: frontend chunk >500 kB; Rust unused functions/field; Harmony ArkTS throwing/deprecated API, permission guidance and duplicate component ID warnings. No permissions or signing changes were made to silence them. Build success is not formal signing, market submission or native-system acceptance.

## Decisions Made

- Raise versions exactly as requested; Harmony code increases by one to keep installation upgrade ordering, without resetting historic numbering.
- Keep buildVersion, SDK, app identity, credentials and schema unchanged.
- Local commits only; no new branch, merge, push, release tag, Linux/Windows installer or market publication.
- Record accepted cross-device core behavior without expanding it to untested combinations.
- Finish release preparation rather than add more backend milestones or redo accepted UI work.

## Assumptions Made

The user's “验收通过” confirms the three-step cross-device procedure above, not every historical matrix. The user's current request authorizes version metadata, handoff and commits, not applying cloud changes, deleting real data or publishing. Earlier generated installers are older artifacts unless explicitly rebuilt and inspected.

## Potential Gotchas

- README contains many historical snapshots. Its new top version status and this handoff override old “未提交”, “下一步” and schema/version claims.
- Do not revive the old manual migration path as a prerequisite for purge or daily planning.
- Do not force-close the user's desktop development app. Stop/restart/install only with clear user intent when required.
- Both repositories share document copies, not one Git worktree; commit/review separately.
- Host tests use isolated SQLite and platform adapters, not real user storage.
- Xiaoyi registration/platform review and real voice entry remain independent of build success.
- Formal package signing and platform-specific runtime acceptance remain release tasks, not inferred from Debug artifacts.

## Immediate Next Steps

1. Verify cwd, git status and latest commits in both repositories; this handoff is a completed versioning checkpoint, not authorization to start new work.
2. Ask for or follow the user's next requested delivery platform (Windows, Linux, Harmony package or store release). Do not rebuild/publish all platforms by default.
3. For authorized release, preserve backups and coordinated-client upgrade guidance, then verify the exact generated package/version/signing and outstanding relevant acceptance only.
4. Do not repeat the accepted plan/wait/resume round trip; track additional date/concurrency/device combinations honestly as residual release scope.

## Environment State

- Windows PowerShell; desktop root D:/Develop/EggDone; Harmony root D:/Develop/EggDoneHarmony, nested project EggDone.
- Tools: pnpm, Cargo, DevEco CLI using installed DevEco Studio, Python 3.12 handoff scripts.
- Python environment name PYTHONUTF8 is used for Chinese-safe handoff generation/validation.
- Debug build output is not a formal release artifact. No credentials, user storage endpoints, signing values or databases are included here.
- Earlier device discovery showed only a phone emulator, not a connected real phone; recheck before any device work.
- Development services may already be running from user work; inspect rather than restart or terminate blindly.

## Related Resources

- [Current regression report](../../docs/TASK_LIFECYCLE_REGRESSION.md)
- [Current roadmap](../../docs/TASK_LIFECYCLE_ROADMAP.md)
- [Release notes](../../CHANGELOG.md)
