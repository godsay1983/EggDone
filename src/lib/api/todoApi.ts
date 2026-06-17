import { invoke } from "@tauri-apps/api/core";

import type {
  RepeatDeleteScope,
  RepeatEditScope,
  RepeatRule,
  Todo,
  TodoGroup,
} from "$lib/types";

export interface TodoScheduleInput {
  due_date: string | null;
  due_at: number | null;
  reminder_at: number | null;
  repeat_rule: RepeatRule | null;
}

export interface TodoCompletionResult {
  updated_todo: Todo;
  created_todo: Todo | null;
}

export interface TodoDeletionResult {
  deleted_todos: Todo[];
}

export interface TodoEditResult {
  updated_todos: Todo[];
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

  updateGroupName(uuid: string, name: string): Promise<TodoGroup> {
    return invoke<TodoGroup>("update_group_name", { uuid, name });
  },

  updateGroupColor(uuid: string, color: string): Promise<TodoGroup> {
    return invoke<TodoGroup>("update_group_color", { uuid, color });
  },

  deleteGroup(uuid: string): Promise<TodoGroup> {
    return invoke<TodoGroup>("delete_group", { uuid });
  },

  reorderGroups(orderedUuids: string[]): Promise<TodoGroup[]> {
    return invoke<TodoGroup[]>("reorder_groups", { orderedUuids });
  },

  setCompleted(id: number, completed: boolean): Promise<TodoCompletionResult> {
    return invoke<TodoCompletionResult>("set_todo_completed", { id, completed });
  },

  setCompletedByUuid(
    uuid: string,
    completed: boolean,
  ): Promise<TodoCompletionResult> {
    return invoke<TodoCompletionResult>("set_todo_completed_by_uuid", {
      uuid,
      completed,
    });
  },

  updateTitle(
    id: number,
    title: string,
    repeatScope: RepeatEditScope = "single",
  ): Promise<TodoEditResult> {
    return invoke<TodoEditResult>("update_todo_title", {
      id,
      title,
      repeatScope,
    });
  },

  updateNote(
    id: number,
    note: string | null,
    repeatScope: RepeatEditScope = "single",
  ): Promise<TodoEditResult> {
    return invoke<TodoEditResult>("update_todo_note", { id, note, repeatScope });
  },

  setPinned(id: number, pinned: boolean): Promise<Todo> {
    return invoke<Todo>("set_todo_pinned", { id, pinned });
  },

  setPriority(id: number, priority: number): Promise<Todo> {
    return invoke<Todo>("set_todo_priority", { id, priority });
  },

  setSchedule(
    id: number,
    schedule: TodoScheduleInput,
    repeatScope: RepeatEditScope = "single",
  ): Promise<TodoEditResult> {
    return invoke<TodoEditResult>("set_todo_schedule", {
      id,
      dueDate: schedule.due_date,
      dueAt: schedule.due_at,
      reminderAt: schedule.reminder_at,
      repeatRule: schedule.repeat_rule,
      repeatScope,
    });
  },

  setGroup(
    id: number,
    groupUuid: string | null,
    repeatScope: RepeatEditScope = "single",
  ): Promise<TodoEditResult> {
    return invoke<TodoEditResult>("set_todo_group", {
      id,
      groupUuid,
      repeatScope,
    });
  },

  reorder(orderedIds: number[]): Promise<Todo[]> {
    return invoke<Todo[]>("reorder_todos", { orderedIds });
  },

  delete(
    id: number,
    repeatScope: RepeatDeleteScope = "single",
  ): Promise<TodoDeletionResult> {
    return invoke<TodoDeletionResult>("delete_todo", { id, repeatScope });
  },

  restore(id: number): Promise<Todo> {
    return invoke<Todo>("restore_todo", { id });
  },

  clearCompleted(): Promise<number> {
    return invoke<number>("clear_completed_todos");
  },

  archiveCompleted(): Promise<number> {
    return invoke<number>("archive_completed_todos");
  },

  hidePanel(): Promise<void> {
    return invoke<void>("hide_panel");
  },

  openFocusWindow(): Promise<void> {
    return invoke<void>("open_focus_window");
  },

  publishFocusNotification(completedPhase: "focus" | "break"): Promise<void> {
    return invoke<void>("publish_focus_notification", { completedPhase });
  },

  markPanelInteraction(): Promise<void> {
    return invoke<void>("mark_panel_interaction");
  },
};
