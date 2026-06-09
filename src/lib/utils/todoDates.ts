import type { Todo } from "$lib/types";

export type DueTone = "" | "today" | "overdue";

export function localDateString(offsetDays = 0, baseDate = new Date()) {
  const date = new Date(baseDate);
  date.setDate(date.getDate() + offsetDays);
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function formatDueLabel(todo: Todo, now = new Date()) {
  if (todo.due_date) {
    const today = localDateString(0, now);
    const tomorrow = localDateString(1, now);
    if (todo.due_date === today) return "今天";
    if (todo.due_date === tomorrow) return "明天";
    return todo.due_date.slice(5).replace("-", "/");
  }

  if (todo.due_at !== null) {
    return new Intl.DateTimeFormat("zh-CN", {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(todo.due_at));
  }

  return "";
}

export function getDueTone(todo: Todo, now = new Date()): DueTone {
  if (todo.completed) return "";

  const today = localDateString(0, now);
  if (todo.due_date) {
    if (todo.due_date < today) return "overdue";
    if (todo.due_date === today) return "today";
    return "";
  }

  if (todo.due_at !== null) {
    if (todo.due_at < now.getTime()) return "overdue";
    if (localDateString(0, new Date(todo.due_at)) === today) return "today";
  }

  return "";
}

export function isDueTodayOrOverdue(todo: Todo, now = new Date()) {
  if (todo.completed) return false;
  return getDueTone(todo, now) === "today" || getDueTone(todo, now) === "overdue";
}
