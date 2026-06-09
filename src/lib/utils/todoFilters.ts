import type { Todo } from "$lib/types";
import { isDueTodayOrOverdue } from "./todoDates";

export type TodoListView = "all" | "today";

export interface TodoFilterOptions {
  view?: TodoListView;
  now?: Date;
}

export function filterTodos(
  items: Todo[],
  query: string,
  showCompleted: boolean,
  options: TodoFilterOptions = {},
): Todo[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const view = options.view ?? "all";
  const now = options.now ?? new Date();

  return items.filter((todo) => {
    if (view === "today" && !isDueTodayOrOverdue(todo, now)) return false;
    if (!showCompleted && todo.completed) return false;
    if (!normalizedQuery) return true;
    return todo.title.toLocaleLowerCase().includes(normalizedQuery);
  });
}
