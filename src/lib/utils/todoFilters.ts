import type { Todo } from "$lib/types";
import { isDueTodayOrOverdue } from "./todoDates";
import { matchesSmartView, type SmartViewId } from "./smartViews";

export type TodoListView = "all" | "today" | "quadrants" | "calendar";

export interface TodoFilterOptions {
  view?: TodoListView;
  groupUuid?: string | null;
  now?: Date;
  smartView?: SmartViewId | null;
}

export function filterTodos(
  items: Todo[],
  query: string,
  showCompleted: boolean,
  options: TodoFilterOptions = {},
): Todo[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const view = options.view ?? "all";
  const groupUuid = options.groupUuid;
  const now = options.now ?? new Date();

  return items.filter((todo) => {
    if (todo.deleted_at !== null || todo.archived_at !== null) return false;
    if (groupUuid === null && todo.group_uuid !== null) return false;
    if (typeof groupUuid === "string" && todo.group_uuid !== groupUuid) {
      return false;
    }
    if (options.smartView) {
      if (!matchesSmartView({
        completed: todo.completed, completedAt: todo.completed_at,
        deletedAt: todo.deleted_at, archivedAt: todo.archived_at,
        dueAt: todo.due_at, dueDate: todo.due_date, priority: todo.priority,
      }, options.smartView, now.getTime())) return false;
    } else {
      if (view === "today" && !isDueTodayOrOverdue(todo, now)) return false;
      if (!showCompleted && todo.completed) return false;
    }
    if (!normalizedQuery) return true;
    return (options.smartView ? `${todo.title}\n${todo.note ?? ""}` : todo.title)
      .toLocaleLowerCase().includes(normalizedQuery);
  });
}
