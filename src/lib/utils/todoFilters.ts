import type { Todo } from "$lib/types";

export function filterTodos(
  items: Todo[],
  query: string,
  showCompleted: boolean,
): Todo[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();

  return items.filter((todo) => {
    if (!showCompleted && todo.completed) return false;
    if (!normalizedQuery) return true;
    return todo.title.toLocaleLowerCase().includes(normalizedQuery);
  });
}
