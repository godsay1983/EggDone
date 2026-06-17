import type { TodoListView } from "./todoFilters";

export type DefaultListViewMode = TodoListView | "remember";

export const DEFAULT_LIST_VIEW_KEY = "eggdone-default-list-view";
export const LAST_LIST_VIEW_KEY = "eggdone-list-view";

export function normalizeDefaultListViewMode(
  value: string | null,
): DefaultListViewMode {
  return value === "all" || value === "today" || value === "quadrants"
    ? value
    : "remember";
}

export function normalizeListView(value: string | null): TodoListView {
  return value === "today" || value === "quadrants" ? value : "all";
}

export function initialListView(
  defaultMode: DefaultListViewMode,
  lastView: string | null,
): TodoListView {
  return defaultMode === "remember" ? normalizeListView(lastView) : defaultMode;
}
