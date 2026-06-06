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

  delete(id: number): Promise<void> {
    return invoke<void>("delete_todo", { id });
  },

  hidePanel(): Promise<void> {
    return invoke<void>("hide_panel");
  },
};
