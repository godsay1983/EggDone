import { invoke } from "@tauri-apps/api/core";

export type TrashKind = "todo" | "note";
export interface TrashItem {
  kind: TrashKind;
  uuid: string;
  title: string;
  content: string;
  deleted_at: number;
  updated_at: number;
  updated_by: string;
  completed: boolean;
  repeating: boolean;
  attachments: { uuid: string; name: string; updated_at: number; updated_by: string; deleted_at: number | null }[];
}
export const trashApi = {
  list: (offset = 0, limit = 50) => invoke<TrashItem[]>("list_trash", { offset, limit }),
  preview: (kind: TrashKind, uuid: string) => invoke<TrashItem>("preview_trash", { kind, uuid }),
  restore: (expected: TrashItem) => invoke<void>("restore_trash", { expected }),
};
