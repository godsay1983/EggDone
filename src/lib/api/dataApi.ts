import { invoke } from "@tauri-apps/api/core";
import { codedInvoke } from "$lib/i18n/errors";

export interface ImportPreview {
  path: string;
  file_name: string;
  total: number;
  added: number;
  updated: number;
  unchanged: number;
  note_total: number;
  note_added: number;
  note_updated: number;
  note_unchanged: number;
  attachment_total: number;
  attachment_added: number;
  attachment_updated: number;
  attachment_unchanged: number;
  attachment_files_included: boolean;
  backup_file_count: number;
  backup_total_bytes: number;
}

export interface ImportResult {
  added: number;
  updated: number;
  unchanged: number;
  note_added: number;
  note_updated: number;
  note_unchanged: number;
  attachment_added: number;
  attachment_updated: number;
  attachment_unchanged: number;
  restored_file_count: number;
}

export interface FullBackupExportResult {
  path: string;
  attachment_count: number;
  file_count: number;
  total_bytes: number;
}

export const dataApi = {
  exportTodos(): Promise<string | null> {
    return codedInvoke(invoke<string | null>("export_todos"), "DATA_EXCHANGE_FAILED");
  },

  exportFullBackup(): Promise<FullBackupExportResult | null> {
    return codedInvoke(invoke<FullBackupExportResult | null>("export_full_backup"), "DATA_EXCHANGE_FAILED");
  },

  previewImport(): Promise<ImportPreview | null> {
    return codedInvoke(invoke<ImportPreview | null>("preview_todo_import"), "DATA_EXCHANGE_FAILED");
  },

  confirmImport(path: string): Promise<ImportResult> {
    return codedInvoke(invoke<ImportResult>("confirm_todo_import", { path }), "DATA_EXCHANGE_FAILED");
  },

  previewFullBackupImport(): Promise<ImportPreview | null> {
    return codedInvoke(invoke<ImportPreview | null>("preview_full_backup_import"), "DATA_EXCHANGE_FAILED");
  },

  confirmFullBackupImport(path: string): Promise<ImportResult> {
    return codedInvoke(invoke<ImportResult>("confirm_full_backup_import", { path }), "DATA_EXCHANGE_FAILED");
  },

  backupDatabase(): Promise<string | null> {
    return codedInvoke(invoke<string | null>("backup_database"), "DATA_EXCHANGE_FAILED");
  },
};
