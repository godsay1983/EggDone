# Handoff: Desktop 1.6.0 Work Review and Calendar Todo Release

## Session Metadata

- Created: 2026-10-02, Asia/Shanghai.
- Project: D:/Develop/EggDone; branch main.
- Current candidate: desktop 1.6.0, Harmony 1.7.0 / 1000037.
- Continues from: [2026-10-01-160113-task-progress-1-5-0-release.md](./2026-10-01-160113-task-progress-1-5-0-release.md).
- Peer handoff: [matching client](D:/Develop/EggDoneHarmony/.claude/handoffs/2026-10-02-141852-work-review-calendar-todo-1-7-0.md).
- Pre-release parent: 7548364 (CT-1); WR-1 b512301.
- This document is included in the user-requested local release commit. Resolve its final commit with git log rather than inserting a self-referential hash.

## Current State Summary

Both work review and calendar-to-todo features are implemented and the user reported core acceptance passed. Their feature commits were already made in both repositories. The following RC-1 automatic release preparation completed: missing Harmony Chinese resources fixed, combined failure/retry/backup tests added, existing regressions checked, and candidate release notes written. The latest user explicitly asked to raise both versions, generate handoffs and commit. This turn uses the user's explicitly requested versions, retaining all existing storage and wire versions. No push, publication, automatic follow-on implementation or device installation is authorized by this handoff.

## Important Context

- Two separate repositories: D:/Develop/EggDone and D:/Develop/EggDoneHarmony. Check live Git state and AGENTS.md in each before touching it.
- Desktop metadata must agree across package.json, src-tauri/Cargo.toml, src-tauri/Cargo.lock (eggdone only) and src-tauri/tauri.conf.json: 1.6.0.
- Harmony app metadata: 1.7.0, versionCode 1000037, buildVersion 1. Never recompute historical internal version codes.
- Database schema 27, portable backup inner format v9, progress/calendar-share wire v1 remain unchanged. This bump adds no migration or permission.
- Generic user approval covers core behavior only, not every device/network/fault matrix. Keep manual gaps separate from automatic proof.
- Never touch real task/calendar data, enable sharing, alter cloud settings or install/uninstall devices to fabricate acceptance evidence.
- Do not push or publish unless the user directly requests it. Handoff creation does not authorize another development phase.

## Immediate Next Steps

1. Read this handoff and the matching peer document, then confirm actual versions, branch and clean Git state. The local release commits should contain the RC-1 fixes, documents and version bumps without generated artifacts.
2. Both business features and automatic release preparation are complete. There is no next backend/UI implementation milestone. Ask for the next concrete release task rather than inventing one.
3. On an explicit packaging request, build current-version artifacts. Do not rename an older executable/package and call it the new release.
4. If device or real S3 acceptance is requested, use explicit device authorization and a disposable target, never the user's production bucket. Record actual results and retained gaps.
5. Read the acceptance document's RC-1 and version-bump sections for precise commands, counts and local evidence locations.

## Architecture Overview

Svelte 5 frontend, Tauri commands, Rust repositories, SQLite transactions. Frontend stores orchestrate mutation and normal autosync; components do not access SQL.

Work review is a readonly projection over existing progress/parent data, not a new sync domain. Calendar-to-todo produces an ordinary independent task, not a persistent calendar binding. Both reuse existing task lifecycle, backup and sync orchestration.

## Critical Files

| File | Purpose |
| --- | --- |
| src-tauri/src/calendar_todo.rs | Atomic task create/resolve and lifecycle guards |
| src-tauri/src/work_review.rs | Readonly review query, paging and snapshot |
| src/lib/components/CalendarTodoDialog.svelte | Editable confirmed task draft |
| src/lib/stores/calendarTodoStore.ts | Stable UUID, uncertain-result recovery |
| src/lib/components/WorkReviewDialog.svelte | Review filters, list and full copy |
| docs/WORK_REVIEW_CALENDAR_TODO_ROADMAP.md | Completed phases and retained acceptance boundaries |
| docs/WORK_REVIEW_CALENDAR_TODO_ACCEPTANCE.md | Commands, counts, errors and evidence |
| docs/WORK_REVIEW_CALENDAR_TODO_RELEASE_NOTES.md | Candidate user-facing update description |
| CHANGELOG.md | Reverse-chronological release history |

## Files Modified

Version-only changes in package.json, Cargo.toml, the eggdone package entry in Cargo.lock and tauri.conf.json. README, CHANGELOG, roadmap, acceptance, candidate release notes and this handoff capture the finished release preparation. No production logic change in this turn; the Harmony peer carries the resource/test fixes.

The peer includes scripts/test-work-review-calendar-todo-release.cjs: production task creation, progress write, complete sync, review and v9 backup restore connected in one isolated host suite. Two stale tests were corrected without changing production behavior: current backup is v9, with explicit v7/v8 compatibility; page disposal is executed rather than required to clean calendar on its first line.

## Decisions Made

- Work review filters by progress created_at and current task title/group; edits never move original dates. Completed/archived tasks remain visible, trashed/purged/missing parents do not.
- Stable 30-row keyset pagination, query-bound tokens, fresh complete copy snapshot; 100000 UTF-16 limit fails explicitly without truncation.
- Calendar draft freezes plain title/time/location/calendar summary. Editable title/note/group only. No due/reminder/repeat/today-plan defaults, no shortcut parsing, no system calendar writes.
- Each draft owns one UUID; create/resolve is atomic and retry-safe. Lost response recovers current task, does not overwrite later edits or revive deleted/archived/purged UUIDs.
- Opening a new draft intentionally permits another independent follow-up from the same calendar event.
- Missing Chinese keys reuse the existing base Chinese values; checks were not weakened.
- The user explicitly chose desktop 1.6.0 and Harmony 1.7.0; these override the initial patch proposal. No dependency version churn or speculative refactor.

## Validation Results

Before this bump, desktop release:check passed 617 Vitest tests, 558 Rust tests with 45 dedicated integration tests ignored, zero Svelte errors/warnings, 1178 locale keys, 89 view fixtures, 12 capture fixtures, production build, cargo fmt/check/test. Harmony's final 26 host scripts passed; this includes six new combined failure/retry/backup groups and 18 complete progress-sync integration groups.

Explicit peer exchange passed using the actual Rust test binary and production Harmony repositories: task document roundtrip; progress create/edit/delete/conflict/terminal/pagination; v9 Rust-Harmony-Rust-Harmony backup. Nine backup groups passed. Dedicated bridge tests were actually executed instead of being counted as ignored successes.

After the user's specified version bump: desktop 1.6.0 full release:check passed (617 Vitest, 558 Rust, 45 dedicated integrations ignored), and Harmony 1.7.0 main Debug build passed. The HAP manifest was verified as 1.7.0 / 1000037. Complete Harmony i18n release check and six combined regression groups also passed. Final evidence and retained warnings are recorded in the acceptance document.

The final 26-script log is eggdone-rc1-harmony-final-20261002.log under the local temporary directory. Cross-client evidence is eggdone-rc1-cross-client-20261002.log. Version-specific logs are eggdone-1-6-0-release-check.log and eggdone-1-7-0-build.log. Logs, databases, screenshots and generated packages are not committed.

## Assumptions Made

User approval is retained as reported, not expanded into a complete test matrix. Host SQLite and memory HTTP are valid regression tools for production orchestration but not native RDB, real cloud service or device visual proof. Old v1-v8 backups retain their documented compatibility; older apps may reject v9 and must be upgraded before importing it.

## Potential Gotchas

- Existing large frontend chunk, seven Rust dead-code warnings and many existing ArkTS exception/deprecated API warnings remain. Build success is not zero-warning proof.
- Forty-five dedicated Rust tests require special harnesses; only the explicitly executed bridge subset is claimed beyond the ordinary suite.
- The existing Windows Debug executable predates this version bump. Frontend production build and Rust check/test do not rebuild a distributable executable. No Windows installer or Linux package was generated in this turn; rebuild explicitly before distribution.
- Actual S3 permission/network policy, Windows native WebView, phone/tablet/full font/theme/language matrix and native clipboard remain partially unverified. Prior phone instrument evidence is recorded, but there was no device operation this turn.
- Do not silently convert a copy length rejection into truncated successful output, or a postcommit refresh failure into a failed task creation.
- Keep terminal checks authoritative so retries and stale peers cannot resurrect deleted records.

## Environment State

Windows / PowerShell. Use login:false for command execution; set PYTHONUTF8=1 when invoking the handoff validator. DevEco CLI drives the native build in the nested EggDone project. Node host tests use isolated databases. Desktop uses pnpm and Cargo. No new dev server, emulator or application was started in this turn; build/check sessions must finish before final response. DevEco may retain its normal build daemon.

## Related Resources

Read AGENTS.md, the current CHANGELOG and the linked roadmap, acceptance and candidate release notes. Previous handoffs are historical context, not the current authorization or proof of publication.
