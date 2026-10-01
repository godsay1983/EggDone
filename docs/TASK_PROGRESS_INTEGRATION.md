# TP-1 Task Progress Integration Contract

This is a development interface supplement to TASK_PROGRESS_PROTOCOL.md. Wire entries remain unchanged.

## Local DTOs and Commands

All wire-shaped DTO fields use snake_case in both clients. Local page shape:

- ProgressCursor: { created_at: number, uuid: string }
- ProgressView: { record: protocol Entry, token: string } where token=SHA256(encode(Document with only this record)).
- ProgressPage: { task_uuid: string, title: string, read_only: boolean, total: number, entries: ProgressView[], next_cursor: ProgressCursor|null, overwritten: boolean }
- ProgressCount: { task_uuid: string, count: number }
- ProgressWrite: { operation_uuid, task_uuid, record_uuid, action: 'create'|'edit'|'delete', body, expected_record: string|null }, per fixed protocol.

Desktop Tauri commands:
- list_task_progress(task_uuid: string, cursor: ProgressCursor|null) -> ProgressPage (30 rows)
- write_task_progress(request: ProgressWrite) -> ProgressPage (first page)
- count_task_progress(task_uuids: string[]) -> ProgressCount[] (one grouped query; max 20000 ids)
- dismiss_task_progress_notice(task_uuid: string) -> void

Frontend invoke arguments use taskUuid/taskUuids for Rust snake_case parameters, matching Tauri defaults; request fields remain snake_case.

Harmony TaskProgressRepository(context, injectedStore=null):
- list(taskUuid: string, cursor: ProgressCursor|null=null): Promise<ProgressPage>
- write(request: ProgressWrite): Promise<ProgressPage>
- counts(taskUuids: string[]): Promise<ProgressCount[]>
- dismissNotice(taskUuid: string): Promise<void>
- snapshot(): Promise<TaskProgressSyncState>
- merge(document): Promise<TaskProgressSyncState>
- static readInTransaction(tx): Promise<TaskProgressSyncState>
- static mergeInTransaction(tx, document): Promise<void>
- static listInTransaction(tx, taskUuid, cursor=null): Promise<ProgressPage>
- static writeInTransaction(tx, request, now, writer): Promise<ProgressPage>

Rust task_progress_store equivalent functions snapshot, read_in_transaction, merge_in_transaction, list, write, counts, dismiss_notice. Store read/merge helpers take &Connection (Transaction coerces to it), to simplify import/purge/sync integration.

SyncState: {document,revision,synced_revision,etag,generation}. Revision safe integers. Local transaction errors use PROGRESS_DATABASE. Domain errors include PROGRESS_INVALID/LIMIT/CONFLICT/UNAVAILABLE/READ_ONLY/DELETED plus sync errors.

## Sync modules

Rust task_progress_sync mirrors task_workflow_sync Snapshot/prepare/is_current/acknowledge. Rust task_progress_session mirrors workflow Receipt/attempt/final_token; transport obtained via PreparedManualSync.progress_transport(). module registration and command registration belong to core worker; s3_sync dispatch belongs to sync integration worker.

Harmony TaskProgressSyncSnapshot and TaskProgressSyncSession mirror workflow APIs, use TaskDocumentDomain 'progress'. Repository core owns these modules; shared TaskDocumentDomain/SyncService/targets/runtime files belong to sync integration worker.

## Migration and delete

Schema 27: task_progress_entries, task_progress_operations (request digest, identifiers and committed token, local write revision; no body), task_progress_sync_state, task_progress_notices (task_uuid only, local overwrite notification). No parent FK dropping unknown parents.

Triggers must maintain revision, reject overflow, clear progress entries/operations/notices when lifecycle_terminals todo appears or a task is physically deleted, retain on completion/archive/soft deletion. Core owns migration and triggers.

For normal sync merge, if a remote edit/delete replaces a current local record referenced by an unacknowledged operation token, insert per-task notice. Do not warn for unrelated pending writes. Valid ACK can mark sent operations published via revision without acknowledging newer operations. Page overwritten indicates local notice; explicit UI dismissal invokes dismissNotice. Notices are not wire/backup data.

Purge integration worker includes progress records in task fingerprint and sanitizes related snapshots/receipts. Core triggers already erase bodies on terminal; purge integration still must ensure before-delete fingerprint detects changed progress and report progress sync pending correctly via global dirty domains.

## Ownership

Core desktop: new task_progress_* (except protocol already committed), migration27, db.rs, lib.rs, its new backend tests.
Core Harmony: models/TaskProgress.ets, TaskProgressRepository, TaskProgressMigration, DbConstants/MigrationRunner, TaskProgressSyncSnapshot/Session, TaskProgressBackup, new host DB tests/harness.
UI desktop: src/lib API/types/stores/components/i18n (and UI tests).
UI Harmony: store/TaskProgressStore, components/pages and localized resources (and UI tests).
Sync integrator: existing shared sync transport/session/runtime/targets/auto-join/space activation files in both repos; existing migration preflight/publication guards to avoid silently dropping new domain.
Backup/purge integrator: existing data exchange/backups/version gates/purge repositories and tests in both repos, plus Rust task_progress_backup.rs. Coordinate new protocol/core helpers through this contract.

No workers commit or change application versions. Parent owns docs and README; changes share a workspace. Tests must use isolated local fixtures, not real user databases/buckets. Parent sequences final Cargo/DevEco builds after integration.
