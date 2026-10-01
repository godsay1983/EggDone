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
  recurrence_total: number;
  link_total: number;
  checklist_total: number;
  checklist_deleted: number;
  checklist_definition_total: number;
  checklist_missing_parent_total: number;
  checklist_metadata_included: boolean;
  template_total: number;
  template_deleted: number;
  template_metadata_included: boolean;
  planning_metadata_included: boolean;
  planning_relations: number;
  planning_completions: number;
  workflow_metadata_included?: boolean;
  workflow_states?: number;
  progressMetadataIncluded?: boolean;
  progressTotal?: number;
  progressAdded?: number;
  progressUpdated?: number;
  progressDeleted?: number;
  progressUnchanged?: number;
  link_deleted: number;
  link_metadata_included: boolean;
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

interface SnakeProgressPreview {
  progress_metadata_included?: boolean;
  progress_total?: number;
  progress_added?: number;
  progress_updated?: number;
  progress_deleted?: number;
  progress_unchanged?: number;
}
function progressPreview(preview: (ImportPreview & SnakeProgressPreview) | null): ImportPreview | null {
  if (!preview || preview.progressMetadataIncluded !== undefined || preview.progress_metadata_included === undefined) return preview;
  // Existing desktop preview fields are snake_case; normalize only the new domain at the IPC boundary.
  return { ...preview, progressMetadataIncluded: preview.progress_metadata_included,
    progressTotal: preview.progress_total, progressAdded: preview.progress_added, progressUpdated: preview.progress_updated,
    progressDeleted: preview.progress_deleted, progressUnchanged: preview.progress_unchanged };
}

export const dataApi = {
  exportTodos(): Promise<string | null> {
    return codedInvoke(invoke<string | null>("export_todos"), "DATA_EXCHANGE_FAILED");
  },

  exportFullBackup(): Promise<FullBackupExportResult | null> {
    return codedInvoke(invoke<FullBackupExportResult | null>("export_full_backup"), "DATA_EXCHANGE_FAILED");
  },

  previewImport(): Promise<ImportPreview | null> {
    return codedInvoke(invoke<(ImportPreview & SnakeProgressPreview) | null>("preview_todo_import"), "DATA_EXCHANGE_FAILED").then(progressPreview);
  },

  confirmImport(path: string): Promise<ImportResult> {
    return codedInvoke(invoke<ImportResult>("confirm_todo_import", { path }), "DATA_EXCHANGE_FAILED");
  },

  previewFullBackupImport(): Promise<ImportPreview | null> {
    return codedInvoke(invoke<(ImportPreview & SnakeProgressPreview) | null>("preview_full_backup_import"), "DATA_EXCHANGE_FAILED").then(progressPreview);
  },

  confirmFullBackupImport(path: string): Promise<ImportResult> {
    return codedInvoke(invoke<ImportResult>("confirm_full_backup_import", { path }), "DATA_EXCHANGE_FAILED");
  },

  backupDatabase(): Promise<string | null> {
    return codedInvoke(invoke<string | null>("backup_database"), "DATA_EXCHANGE_FAILED");
  },
};
