import { invoke } from "@tauri-apps/api/core";

import type { Todo } from "$lib/types";

export const todoApi = {
  list(): Promise<Todo[]> {
    return invoke<Todo[]>("list_todos");
  },

  create(title: string): Promise<Todo> {
    return invoke<Todo>("create_todo", { title });
  },

  setCompleted(id: number, completed: boolean): Promise<Todo> {
    return invoke<Todo>("set_todo_completed", { id, completed });
  },

  updateTitle(id: number, title: string): Promise<Todo> {
    return invoke<Todo>("update_todo_title", { id, title });
  },

  setPinned(id: number, pinned: boolean): Promise<Todo> {
    return invoke<Todo>("set_todo_pinned", { id, pinned });
  },

  reorder(orderedIds: number[]): Promise<Todo[]> {
    return invoke<Todo[]>("reorder_todos", { orderedIds });
  },

  delete(id: number): Promise<Todo> {
    return invoke<Todo>("delete_todo", { id });
  },

  restore(id: number): Promise<Todo> {
    return invoke<Todo>("restore_todo", { id });
  },

  clearCompleted(): Promise<number> {
    return invoke<number>("clear_completed_todos");
  },

  hidePanel(): Promise<void> {
    return invoke<void>("hide_panel");
  },

  markPanelInteraction(): Promise<void> {
    return invoke<void>("mark_panel_interaction");
  },
};
