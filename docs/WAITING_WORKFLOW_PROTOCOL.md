# Waiting Workflow (LC4)

Implementation contract, 2026-09-19. This extends the existing automatic sync-target discovery; no user-visible space selection or migration is introduced.

## Wire and Lifecycle

- Independent domain `workflow`, version 1: `{format_version:1,events:[],states:[]}`.
- `events` uses the existing daily-planning `{task_uuid,event_id}` lifecycle event set. Both domains transport the same authoritative events and union them into `daily_plan_events`. Never infer or install events from a state's basis.
- State: `{task_uuid,state,reason,review_date,clock,writer,basis}`. `state` is `ready` or `waiting`; ready requires empty reason and null review_date. Reason is at most 200 UTF-16 code units, without trimming. Review date is null or a real canonical YYYY-MM-DD local date (0001..9999).
- UUID/writer/clock/basis validation, 20,000 row / 4 MiB document limits, 4,096 event limit per task follow daily planning. Reject unknown fields and duplicate identities. No lossy truncation.
- Identity is task_uuid. LWW winner compares clock, writer (ASCII), state (`ready` wins an exact clock/writer tie), review_date (null as empty), then reason by UTF-16 code units. If all ranking fields match but basis differs, reject the ambiguous pair with WORKFLOW_CONFLICT in either merge direction. Never guess lifecycle authority from arrival order. Canonical output sorts events and states by identity; unpaired UTF-16 surrogates are invalid.
- Key: `eggdone-workflow/v1/<sha256 of configured todo Object Key as UTF-8>/states.json`, with existing key validation and collision checks.
- A waiting record is visible only for an active, nonterminal parent with exact known lifecycle basis. Completion/reopen/archive/unarchive/delete/restore generate the existing authoritative events. Stale records cannot become visible after restoration. Purge physically removes state/reason and operation receipts containing sensitive body data (receipts should store only request digests).
- Workflow has its own revision/ACK/ETag/generation. Event changes dirty both planning and workflow. Target changes, automatic join, final refresh and dirty status must include workflow. Sync applies parent tasks, then planning/workflow; lifecycle event sets are unioned, never replaced.

## Storage and API

- Database migration 26 adds workflow states, operation digests, and sync state/triggers. Existing lifecycle triggers stay authoritative.
- Backup version 8 adds optional `task_workflow` with the above document; versions through 7 remain readable. Omitted domain preserves local workflow, explicit document merges atomically using the normal terminal/lifecycle filters. Legacy fixed-domain migration must reject nonempty workflow rather than silently omit it.
- Desktop commands: `list_task_workflow(date)` and `write_task_workflow(request)`.
- Snapshot: `{date,revision,entries:[{task_uuid,reason,review_date,review_due,clock}]}`. Entries include effective waiting only. Revision is an opaque 64-lowercase-hex token covering workflow state, lifecycle/parents and target generation. Ordering: dated first, ascending review date, descending clock, task_uuid. review_due means review_date <= date.
- Write: `{operation_uuid,task_uuid,state,reason,review_date,date,remove_from_plan,expected,expected_plan}`. `expected_plan` is string or null, required to match the daily-plan snapshot revision when remove_from_plan is true. Removal writes an explicit excluded record for `date` in the same transaction. Removal is valid only for waiting. Other fields are validated even for retries.
- Duplicate operation UUID with identical request digest returns current snapshot, never replays mutation. Different payload conflicts. Optimistic mismatch rejects without writes. Error codes `WORKFLOW_INVALID`, `WORKFLOW_CONFLICT`, `WORKFLOW_UNAVAILABLE`, `WORKFLOW_LIMIT`, `WORKFLOW_DATABASE`.
- Harmony model/store/repository use the same field names and semantics; no sensitive reason in ordinary logs or receipts.

## UI

- Common task menu: set/edit waiting, resume processing. Dialog: optional reason, optional review date, optional remove from today's plan (unchecked by default, shown for a planned task). Cancel writes nothing. Failed save keeps draft and same operation identity for retry; changed draft requires a new request.
- Common task metadata: Waiting / Review due badge. Planned waiting tasks remain visible but excluded from the immediately actionable count. Existing overall incomplete count is unchanged.
- Waiting list entry in More; filters All / Review due / No review date, sorts Review date / Recently updated. Reuse common task rows and menus, no separate task content representation.
- No automatic reminders, date edits, plan creation, workflow inheritance for copies/templates/recurrences, or user-data/cloud operations during development.

## UI Refinement (2026-09-19)

- Desktop review date uses a fixed `yyyy/mm/dd` placeholder and slash display, retaining the native calendar picker and canonical ISO storage.
- Both clients use matching Waiting / Review due wording and light/dark badge colors, ordered after the daily-plan badge.
- Harmony waiting actions use the neighboring 30vp capsule-button style and equal-width columns.
- Refinement verification: desktop 486 frontend tests, zero check errors/warnings, build and 1,044-key i18n check passed. Browser coverage passed 12 waiting, 12 planning, 24 preview and 12 settings cases using mocked IPC.
- Harmony waiting UI 23 host scenarios and related plan/action checks passed; final Debug HAP build succeeded (53 seconds incremental). No native-device or user-cloud acceptance is implied.

## Acceptance Boundary

Automated tests and builds do not substitute for desktop native, Harmony phone/tablet, or user-cloud cross-device acceptance. Record actual results in the roadmap after verification.

## 操作与验收

1. 活动任务的“更多”选择“设为等待”，原因和查看日期都可不填；保存后任务出现等待标签。
2. 已安排的任务默认仍在今日计划。只有勾选“同时移出今日计划”后保存，才同时移出。
3. 主页面“更多 -> 等待处理”查看列表，支持全部、待复查、无查看日期筛选，以及查看日期、最近更新排序。
4. 等待任务的“更多”可编辑等待或恢复处理。恢复处理不改变原截止日期、提醒，不自动加入今日计划。
5. 分别在两端设置、修改、恢复等待，然后按原有方式同步；确认另一端的原因、查看日期和计划归属一致，无需修改 Object Key。
6. 验证完成后再取消完成、归档后再取消归档、删除后再恢复，都不恢复旧等待。复制、模板新建、重复后继也不继承等待。
7. 查看日期到达后只显示“待复查”，不会自动解除等待、安排今天或触发新通知。原有截止/逾期提醒仍保持。
8. 等待期间保存备份并导入，确认状态保留；旧版本备份不含等待领域时，不清除现有等待。清空回收站后，等待原因不能通过旧同步内容恢复。

## 自动化记录

- Desktop frontend: `pnpm check` (0 errors, 0 warnings), `pnpm test` (484 tests), `pnpm build`, i18n catalog (1,044 aligned keys).
- `scripts/test-daily-plan-ui.mjs`: 12 waiting UI cases across zh/en, light/dark, 320/480/1000 widths, plus uncertain-result exact retry; existing 12 planning, 24 import-preview and 12 sync-settings cases passed. Mocked IPC only.
- Both repositories contain identical `docs/fixtures/task-workflow-v1.json` for protocol validation, UTF-16 ordering, explicit ready tie-breaking and authoritative event union.
- Desktop backend: `cargo fmt --all -- --check`, `cargo check`, workflow-focused 26 tests; final `cargo test --lib -j 2`: 477 passed, 0 failed, 40 ignored. Ignored isolated-service/manual harnesses were not claimed as passed.
- Harmony: `node scripts/test-task-workflow.cjs --desktop=D:/Develop/EggDone` passed 10 repository/migration scenarios plus shared fixtures (3 valid, 5 invalid, 1 invalid raw, 3 bidirectional merges). `test-task-workflow-store.cjs` passed 5 scenarios; `test-task-workflow-integration.cjs` and `test-task-workflow-http.cjs` passed backup, final-refresh/ACK, local HTTP/CAS/network/permission/malformed-response coverage. These use isolated local data, not a user's bucket.
- Harmony UI: `node scripts/test-waiting-ui.cjs` passed 22 host scenarios, including compact resume confirmation, native date-picker draft callbacks, frozen uncertain writes and day rollover. Related planning/navigation/link/history/preferences regressions passed.
- Final native Harmony Debug HAP: DevEco Hvigor `assembleHap --no-daemon`, BUILD SUCCESSFUL (38 seconds incremental). Output: `EggDone/entry/build/default/outputs/default/entry-default-signed.hap`. Existing deprecated-API and duplicate `todo-search-input` warnings remain outside LC4 scope.
- Final desktop browser screenshots checked: no horizontal clipping at 320px; native client runtime, real phone/tablet keyboard/layout and user-cloud acceptance remain open. No installation, real-data writes, publishing or version bump was performed. Implementation and UI refinement are included in the requested local commits.
