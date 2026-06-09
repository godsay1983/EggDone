import { derived, writable } from "svelte/store";

import { todoApi } from "$lib/api/todoApi";
import type { TodoScheduleInput } from "$lib/api/todoApi";
import { scheduleAutoSync } from "$lib/sync/autoSync";
import type { Todo } from "$lib/types";

export interface TodoState {
  items: Todo[];
  loading: boolean;
  error: string | null;
}

const initialState: TodoState = {
  items: [],
  loading: true,
  error: null,
};

export function createTodoStore(api = todoApi, onChanged = scheduleAutoSync) {
  const { subscribe, update } = writable(initialState);

  return {
    subscribe,

    async load() {
      update((state) => ({ ...state, loading: true, error: null }));
      try {
        const items = await api.list();
        update(() => ({ items, loading: false, error: null }));
      } catch (error) {
        update((state) => ({
          ...state,
          loading: false,
          error: getErrorMessage(error),
        }));
      }
    },

    async refresh() {
      try {
        const items = await api.list();
        update((state) => ({ ...state, items, error: null }));
      } catch (error) {
        update((state) => ({
          ...state,
          error: getErrorMessage(error),
        }));
      }
    },

    async add(title: string) {
      const todo = await api.create(title);
      update((state) => ({
        ...state,
        items: [todo, ...state.items],
        error: null,
      }));
      onChanged();
    },

    async toggle(todo: Todo) {
      const updatedTodo = await api.setCompleted(todo.id, !todo.completed);
      update((state) => ({
        ...state,
        items: state.items
          .map((item) => (item.id === updatedTodo.id ? updatedTodo : item))
          .sort(sortTodos),
        error: null,
      }));
      onChanged();
    },

    async edit(id: number, title: string) {
      const updatedTodo = await api.updateTitle(id, title);
      update((state) => ({
        ...state,
        items: state.items.map((item) =>
          item.id === updatedTodo.id ? updatedTodo : item,
        ),
        error: null,
      }));
      onChanged();
    },

    async setPinned(todo: Todo, pinned: boolean) {
      const updatedTodo = await api.setPinned(todo.id, pinned);
      update((state) => ({
        ...state,
        items: state.items
          .map((item) => (item.id === updatedTodo.id ? updatedTodo : item))
          .sort(sortTodos),
        error: null,
      }));
      onChanged();
    },

    async setSchedule(id: number, schedule: TodoScheduleInput) {
      const updatedTodo = await api.setSchedule(id, schedule);
      update((state) => ({
        ...state,
        items: state.items.map((item) =>
          item.id === updatedTodo.id ? updatedTodo : item,
        ),
        error: null,
      }));
      onChanged();
    },

    async reorder(orderedIds: number[]) {
      let previousItems: Todo[] = [];
      update((state) => {
        previousItems = state.items;
        const order = new Map(orderedIds.map((id, index) => [id, index]));
        const reorderedGroup = state.items
          .filter((item) => order.has(item.id))
          .sort((left, right) => order.get(left.id)! - order.get(right.id)!);
        let groupIndex = 0;
        return {
          ...state,
          items: state.items.map((item) =>
            order.has(item.id) ? reorderedGroup[groupIndex++] : item,
          ),
          error: null,
        };
      });

      try {
        const items = await api.reorder(orderedIds);
        update((state) => ({ ...state, items, error: null }));
        onChanged();
      } catch (error) {
        update((state) => ({
          ...state,
          items: previousItems,
          error: getErrorMessage(error),
        }));
        throw error;
      }
    },

    async remove(id: number) {
      const deletedTodo = await api.delete(id);
      update((state) => ({
        ...state,
        items: state.items.filter((item) => item.id !== id),
        error: null,
      }));
      onChanged();
      return deletedTodo;
    },

    async restore(id: number) {
      const restoredTodo = await api.restore(id);
      update((state) => ({
        ...state,
        items: [...state.items, restoredTodo].sort(sortTodos),
        error: null,
      }));
      onChanged();
    },

    async clearCompleted() {
      const clearedCount = await api.clearCompleted();
      update((state) => ({
        ...state,
        items: state.items.filter((item) => !item.completed),
        error: null,
      }));
      if (clearedCount > 0) onChanged();
      return clearedCount;
    },

    reportError(error: unknown) {
      update((state) => ({ ...state, error: getErrorMessage(error) }));
    },
  };
}

function sortTodos(left: Todo, right: Todo) {
  if (left.completed !== right.completed) {
    return Number(left.completed) - Number(right.completed);
  }
  if (left.pinned !== right.pinned) {
    return Number(right.pinned) - Number(left.pinned);
  }
  return left.sort_order - right.sort_order;
}

function getErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export const todos = createTodoStore();
export const remainingCount = derived(
  todos,
  ($todos) => $todos.items.filter((todo) => !todo.completed).length,
);
export const completedCount = derived(
  todos,
  ($todos) => $todos.items.filter((todo) => todo.completed).length,
);
