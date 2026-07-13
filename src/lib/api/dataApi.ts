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
}

export interface ImportResult {
  added: number;
  updated: number;
  unchanged: number;
  note_added: number;
  note_updated: number;
  note_unchanged: number;
}

export const dataApi = {
  exportTodos(): Promise<string | null> {
    return invoke<string | null>("export_todos");
  },

  previewImport(): Promise<ImportPreview | null> {
    return invoke<ImportPreview | null>("preview_todo_import");
  },

  confirmImport(path: string): Promise<ImportResult> {
    return invoke<ImportResult>("confirm_todo_import", { path });
  },

  backupDatabase(): Promise<string | null> {
    return invoke<string | null>("backup_database");
  },
};
