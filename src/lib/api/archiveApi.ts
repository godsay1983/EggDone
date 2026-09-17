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
export const archiveApi = {
  list: (query: string, cursor: ArchiveCursor | null) => invoke<ArchivePage>("list_archived", { query, cursor }),
  preview: (uuid: string) => invoke<ArchivePreview>("preview_archived", { uuid }),
  apply: (request: ArchiveRequest) => invoke<ArchiveOutcome>("apply_archive_action", { ...request }),
};
