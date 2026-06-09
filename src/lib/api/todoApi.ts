import { invoke } from "@tauri-apps/api/core";

import type { Todo, TodoGroup } from "$lib/types";

export interface TodoScheduleInput {
  due_date: string | null;
  due_at: number | null;
  reminder_at: number | null;
}

export const todoApi = {
  list(): Promise<Todo[]> {
    return invoke<Todo[]>("list_todos");
  },

  listGroups(): Promise<TodoGroup[]> {
    return invoke<TodoGroup[]>("list_groups");
  },

  create(title: string, groupUuid: string | null = null): Promise<Todo> {
    return invoke<Todo>("create_todo", { title, groupUuid });
  },

  createGroup(name: string): Promise<TodoGroup> {
    return invoke<TodoGroup>("create_group", { name });
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

  setSchedule(id: number, schedule: TodoScheduleInput): Promise<Todo> {
    return invoke<Todo>("set_todo_schedule", {
      id,
      dueDate: schedule.due_date,
      dueAt: schedule.due_at,
      reminderAt: schedule.reminder_at,
    });
  },

  setGroup(id: number, groupUuid: string | null): Promise<Todo> {
    return invoke<Todo>("set_todo_group", { id, groupUuid });
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
