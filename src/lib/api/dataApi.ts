import { invoke } from "@tauri-apps/api/core";

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
    return invoke<string | null>("export_todos");
  },

  exportFullBackup(): Promise<FullBackupExportResult | null> {
    return invoke<FullBackupExportResult | null>("export_full_backup");
  },

  previewImport(): Promise<ImportPreview | null> {
    return invoke<ImportPreview | null>("preview_todo_import");
  },

  confirmImport(path: string): Promise<ImportResult> {
    return invoke<ImportResult>("confirm_todo_import", { path });
  },

  previewFullBackupImport(): Promise<ImportPreview | null> {
    return invoke<ImportPreview | null>("preview_full_backup_import");
  },

  confirmFullBackupImport(path: string): Promise<ImportResult> {
    return invoke<ImportResult>("confirm_full_backup_import", { path });
  },

  backupDatabase(): Promise<string | null> {
    return invoke<string | null>("backup_database");
  },
};
