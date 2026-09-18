# Handoff: Desktop Trash Activation Ready for Acceptance

## Session Metadata
- Created: 2026-09-19, Asia/Shanghai.
- Project: D:/Develop/EggDone; branch main; implementation commit 51c5bfe.
- Paired project: D:/Develop/EggDoneHarmony; implementation commit e6e52f7.

## Handoff Chain
- Continues from [desktop-local-purge-sync-pending](./2026-09-18-175515-desktop-local-purge-sync-pending.md).
- Supersedes its pending implementation status for terminal sync, remote cleanup and formal activation; preserve historical evidence.
- Paired [Harmony handoff](D:/Develop/EggDoneHarmony/.claude/handoffs/2026-09-19-023442-harmony-trash-space-activation.md).

## Current State Summary
LC2 clear-trash is connected end to end across both clients: explicit migration, peer enrollment, ordinary sync admission, permanent deletion, remote attachment cleanup, progress and retry. Automated regressions, bidirectional isolated S3 sessions and both native builds passed. Actual desktop native-window and Harmony phone/tablet acceptance against the user's storage remains pending. No real user space was migrated/purged; nothing was installed. Implementation is locally committed; this handoff is a separate documentation commit.

## Important Context
The user wanted the complete backend and interface together after many small rounds. Do not invent another generic migration/backend milestone. Remaining work is native/real-storage acceptance and fixes for concrete failures. The user accepted explicit migration retaining the old space. All participating devices must upgrade and gather offline changes first.

User entry: Recycle bin -> Migration preparation -> Check and prepare -> agreement -> Migrate and enable (first settled device) or Join new space (peers keep original settings). Empty trash retains this entry. Manual Object Key editing does not authorize v2 sync.

Latest request is handoff and local commits only. Do not push, publish, bump versions or start LC3/LC4. Implementation commits are desktop 51c5bfe and Harmony e6e52f7, both on main. Versions remain desktop 1.2.0 / SQLite22, Harmony 1.3.0 / code1000030 / RDB23. Desktop has a pre-existing Cargo.toml line-ending-only difference, deliberately excluded and not reverted.

## Immediate Next Steps
1. Read the paired handoff and acceptance document; recheck both Git states before continuing.
2. Obtain installation approval and connect suitable devices. Use an independent test space and synthetic records, never the only copy of user data.
3. Gather old-space edits and sync; create from a settled device, then join the other upgraded device using original source settings.
4. Verify single/selected/all purge, attachments, offline/reconnect, app restart and stale-device no-resurrection. Inspect phone/tablet touch layouts and native database transactions.
5. Record actual acceptance separately from automated evidence. Fix observed issues only, and await authorization before release or the next feature.

## Architecture Overview
- Immutable association under eggdone-space-links/v2/<SHA256 of old main UTF-8>.json; wire [1, oldMainKey, seedManifestString].
- New main is eggdone-spaces/v2/<publication UUID>/todos.json. Validate immutable seed and active parent references; write eight bounded domain envelopes, files and empty terminal ledger, then ready.json last.
- Conditional creation plus exact readback handles request/reply loss. Existing ready spaces are verified using current documents, never seed replay. Missing/corrupt domains fail closed.
- Source fingerprint includes endpoint, region, bucket, old key, path style and HTTP option, not credentials.
- Final local transaction switches key/epoch, invalidates ACKs, records target-bound proof and applies remote terminals before stale local content can merge.
- Surviving local attachment binaries are verified/copied before activation. Remote cleanup evidence is captured before metadata erasure; later deletion is target-bound and conditional.
- Metadata names: sync.space.activation.v1 (proof), migration.backup.space.v1 (pending). Pending state is excluded from local snapshot fingerprint.
- After activation, main content/settings and the cached trash list refresh immediately.

## Decisions Made
- Explicit association and proof instead of path-only admission: all peers discover one destination without bypassing safety.
- Ready-last and no seed replay: recovery cannot overwrite subsequent live edits.
- Verified local backup with current-state revalidation: concurrent edits must not disappear silently.
- Terminal-first joining: locally purged devices can join an existing destination without unsafe legacy upload.
- Preserve old spaces and recovery copies: clear-trash is not forensic deletion from every copy.

## Validation
Implementation-turn evidence:
- Rust: 400 passed / 33 ignored; ignored storage tests were not counted as passing. Relevant storage suite ran separately.
- Frontend: 415 tests, type check 0 errors/0 warnings, 964 aligned translation keys.
- Desktop Playwright: 24 locale/theme/window/zoom combinations plus 4 migration/purge/failure scenarios, including postactivation trash cache refresh.
- Harmony initializer: 68 request/reply fault boundaries, no replay, missing-object refusal, invalid seed cannot claim, dependency checks.
- Harmony purge/v6 backup; 29 remote-cleanup groups; terminal protocol/transactions; 40 preflight cases; native-backup host adapters; 45 session/gate scenarios; recurrence workflow and 9 upload ACK scenarios passed.
- Real isolated S3 forward: desktop creates -> Harmony joins/purges -> lost DELETE reply retry -> stale desktop joins without resurrection.
- Reverse: Harmony creates -> lost ready PUT reply/new instance resumes -> desktop local-only terminal joins/purges -> stale Harmony joins without resurrection.
- Production admission is not bypassed. Desktop injects test credentials into actual admission/core/success handling; Harmony uses production services with host file/RDB/credentials/HTTP adapters. Not native ArkData proof.
- Evidence: %TEMP%/eggdone-sync-core-dad389d1db2c4e61945e6aec91667c2f.
- UI screenshots: %TEMP%/eggdone-trash-ui-1789756352082.
- Desktop Debug EXE and Harmony Debug HAP built successfully. Existing compiler warnings remain.
Commit-turn checks:
- Desktop cargo test space_activation --lib: all 4 passed.
- Harmony 68 initializer boundaries plus migration/purge panels passed.
- Both staged diffs reviewed and git diff --cached --check passed.

## Assumptions Made
Storage implements required conditional operations; disposable SeaweedFS is not evidence for every provider. Users accept explicit migration and retain source settings until joining. First creator has completed old-space sync and gathered offline edits.

## Potential Gotchas
- A device with local terminal deletions can join an existing space, but cannot create first while legacy settled checks fail. Use a settled peer; never erase terminals to bypass this.
- Global sync failure/dirty domains conservatively keep purge pending, even when unrelated to that operation. No per-operation completion receipts were added.
- Old spaces, staging seeds, recovery snapshots, independent backups, provider versions and failed-copy orphan assets are outside purge scope.
- Seed reference checks do not imply live multi-object snapshot atomicity.
- Do not run desktop build/check concurrently with Playwright Vite tests: generated-file reloads invalidate fixture state.
- Preserve desktop Cargo.toml pre-existing line-ending-only dirt.
- Native builds/host tests do not prove device layout, keyboard, permission or actual crash/restart behavior.

## Environment State
- Windows PowerShell; pnpm, Cargo, Node, Docker, DevEco CLI.
- No task-owned build/test processes remain. Disposable S3 resources were removed; do not stop unrelated services.
- Only detected device was phone emulator Mate 80 Pro Max at 127.0.0.1:5555. Installation permission question was unanswered; no install performed.
- Desktop artifact: [eggdone.exe](D:/Develop/EggDone/src-tauri/target/debug/eggdone.exe).
- Harmony artifact: [entry-default-signed.hap](D:/Develop/EggDoneHarmony/EggDone/entry/build/default/outputs/default/entry-default-signed.hap).
- Logs: %TEMP%/eggdone-space-desktop-build.log and %TEMP%/eggdone-space-build.log.
- Variable names: PYTHONUTF8, PLAYWRIGHT_PATH, EGGDONE_NS7_S3_RUN, EGGDONE_NS7_S3_PORT. Do not record credential values.

## Critical Files
| File | Purpose |
| --- | --- |
| src-tauri/src/space_activation.rs | Wire, initializer, proof, atomic transaction and fault tests |
| src-tauri/src/space_activation_commands.rs | Native status/prepare/activate orchestration |
| src-tauri/src/s3_sync.rs | Bounded S3 transport and production admission |
| src-tauri/src/purge.rs | Purge safety, fixed targets and progress |
| src-tauri/src/purge_remote.rs | Capture remote evidence before erasure |
| src/lib/components/MigrationPreparation.svelte | Guided explicit migration |
| src/lib/components/TrashDialog.svelte | Purge states/retry/cache refresh |
| src-tauri/src/sync_core_activation_s3_tests.rs | Bidirectional ordinary-path sessions |
| docs/TASK_PURGE_ACTIVATION_ACCEPTANCE.md | Steps, boundaries and pending acceptance |

## Files Modified
Commit 51c5bfe contains 26 files. Supporting edits register commands, expose verified backup assets, extract transaction helpers, add activation API, update bilingual messages, tests and lifecycle docs. Inspect git show --stat 51c5bfe and git show 51c5bfe for exact scope. No dependencies, version or database schema changes committed.

## Reproduction Commands
Run in D:/Develop/EggDone:
~~~powershell
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml --lib
./scripts/run-sync-core-s3.ps1 -ActivationSessions -HarmonyRoot D:/Develop/EggDoneHarmony -Port 18480
$env:PLAYWRIGHT_PATH='C:/Users/caozhipeng/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
node scripts/test-trash-ui.mjs
pnpm tauri build --debug --no-bundle
~~~
Run build/check separately from browser tests. Ask for normal exit if the executable is locked; do not force-kill the app. S3 runner requires the preinstalled pinned image, uses loopback/tmpfs/synthetic data and removes its own container.

## Related Resources
- [Acceptance](D:/Develop/EggDone/docs/TASK_PURGE_ACTIVATION_ACCEPTANCE.md)
- [Roadmap](D:/Develop/EggDone/docs/TASK_LIFECYCLE_ROADMAP.md)
- [Implementation plan](D:/Develop/EggDone/docs/TASK_LIFECYCLE_IMPLEMENTATION_PLAN.md)
