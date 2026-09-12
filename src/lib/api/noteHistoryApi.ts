import { invoke } from "@tauri-apps/api/core";

export interface NoteTextVersion {
  title: string;
  content: string;
  updated_at: number;
  updated_by: string;
}
export interface HistorySummary {
  id: number;
  note_uuid: string;
  title: string;
  excerpt: string;
  updated_at: number;
  captured_at: number;
}
export interface HistoryPreview {
  entry: { id: number; note_uuid: string; text: NoteTextVersion; captured_at: number };
  current: NoteTextVersion;
}
export const noteHistoryApi = {
  list: (uuid: string) => invoke<HistorySummary[]>("list_note_history", { uuid }),
  preview: (uuid: string, id: number) => invoke<HistoryPreview>("preview_note_history", { uuid, id }),
  restore: (expected: HistoryPreview) => invoke<boolean>("restore_note_history", { expected }),
};
