import { invoke } from "@tauri-apps/api/core";

import type { Note, NoteColor } from "$lib/types";

export const noteApi = {
  list(): Promise<Note[]> {
    return invoke<Note[]>("list_notes");
  },

  create(
    title: string,
    content: string,
    color: NoteColor = "default",
  ): Promise<Note> {
    return invoke<Note>("create_note", { title, content, color });
  },

  update(uuid: string, title: string, content: string): Promise<Note> {
    return invoke<Note>("update_note", { uuid, title, content });
  },

  setPinned(uuid: string, pinned: boolean): Promise<Note> {
    return invoke<Note>("set_note_pinned", { uuid, pinned });
  },

  setColor(uuid: string, color: NoteColor): Promise<Note> {
    return invoke<Note>("set_note_color", { uuid, color });
  },

  delete(uuid: string): Promise<Note> {
    return invoke<Note>("delete_note", { uuid });
  },

  restore(uuid: string): Promise<Note> {
    return invoke<Note>("restore_note", { uuid });
  },
};
