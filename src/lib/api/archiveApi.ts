import { invoke } from "@tauri-apps/api/core";
export type ArchiveAction = "unarchive" | "reopen" | "delete";
export interface ArchiveExpected { uuid: string; scope: string; fingerprint: string; }
export interface ArchivePreview {
  expected: ArchiveExpected; title: string; content: string; completed: boolean;
  archived_at: number; updated_at: number; completed_at: number | null;
  due_date: string | null; due_at: number | null; group_name: string | null;
  checklist_json: string; links_json: string;
}
export interface ArchiveCursor { archived_at: number; uuid: string; query: string; }
export interface ArchivePage { items: ArchivePreview[]; next: ArchiveCursor | null; }
export interface ArchiveOutcome {
  uuid: string; action: ArchiveAction; outcome: string; result_version: number; warnings: string[];
}
export interface ArchiveRequest { operation: string; action: ArchiveAction; expected: ArchiveExpected; }
export type ArchiveBatchAction = Exclude<ArchiveAction, "reopen">;
export interface ArchiveBatchRequest { operation: string; action: ArchiveBatchAction; targets: ArchiveExpected[]; }
export interface ArchiveItemResult { uuid: string; result: ArchiveOutcome | null; error: string | null; }
export interface ArchiveJob { operation_uuid: string; action: ArchiveBatchAction; scope: string; targets: ArchiveExpected[]; results: ArchiveItemResult[]; }
export interface ArchiveBatchExecution { job: ArchiveJob | null; error: string | null; refreshNeeded: boolean; }
export const archiveApi = {
  list: (query: string, cursor: ArchiveCursor | null) => invoke<ArchivePage>("list_archived", { query, cursor }),
  preview: (uuid: string) => invoke<ArchivePreview>("preview_archived", { uuid }),
  apply: (request: ArchiveRequest) => invoke<ArchiveOutcome>("apply_archive_action", { ...request }),
};
export const archiveBatchApi = {
  prepare: (request: ArchiveBatchRequest) => invoke<ArchiveJob>("prepare_archive_batch", { ...request }),
  run: (operation: string) => invoke<ArchiveJob>("run_archive_batch", { operation }),
  pending: () => invoke<ArchiveJob[]>("pending_archive_batches"),
  dismiss: (operation: string) => invoke<void>("dismiss_archive_batch", { operation }),
};
