import { derived, writable } from "svelte/store";

import { todoApi } from "$lib/api/todoApi";
import type { Todo } from "$lib/types";

interface TodoState {
  items: Todo[];
  loading: boolean;
  error: string | null;
}

const initialState: TodoState = {
  items: [],
  loading: true,
  error: null,
};

function createTodoStore() {
  const { subscribe, update } = writable(initialState);

  return {
    subscribe,

    async load() {
      update((state) => ({ ...state, loading: true, error: null }));
      try {
        const items = await todoApi.list();
        update(() => ({ items, loading: false, error: null }));
      } catch (error) {
        update((state) => ({
          ...state,
          loading: false,
          error: getErrorMessage(error),
        }));
      }
    },

    async add(title: string) {
      const todo = await todoApi.create(title);
      update((state) => ({
        ...state,
        items: [todo, ...state.items],
        error: null,
      }));
    },

    async toggle(todo: Todo) {
      const updatedTodo = await todoApi.setCompleted(todo.id, !todo.completed);
      update((state) => ({
        ...state,
        items: state.items
          .map((item) => (item.id === updatedTodo.id ? updatedTodo : item))
          .sort(sortTodos),
        error: null,
      }));
    },

    async remove(id: number) {
      await todoApi.delete(id);
      update((state) => ({
        ...state,
        items: state.items.filter((item) => item.id !== id),
        error: null,
      }));
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
