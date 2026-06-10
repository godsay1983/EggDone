import { derived, writable } from "svelte/store";

import { todoApi } from "$lib/api/todoApi";
import type { TodoScheduleInput } from "$lib/api/todoApi";
import { scheduleAutoSync } from "$lib/sync/autoSync";
import type { RepeatDeleteScope, Todo, TodoGroup } from "$lib/types";

export interface TodoState {
  items: Todo[];
  groups: TodoGroup[];
  loading: boolean;
  error: string | null;
}

const initialState: TodoState = {
  items: [],
  groups: [],
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
        const [items, groups] = await Promise.all([api.list(), api.listGroups()]);
        update(() => ({ items, groups, loading: false, error: null }));
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
        const [items, groups] = await Promise.all([api.list(), api.listGroups()]);
        update((state) => ({ ...state, items, groups, error: null }));
      } catch (error) {
        update((state) => ({
          ...state,
          error: getErrorMessage(error),
        }));
      }
    },

    async add(title: string, groupUuid: string | null = null) {
      const todo = await api.create(title, groupUuid);
      update((state) => ({
        ...state,
        items: [todo, ...state.items],
        error: null,
      }));
      onChanged();
      return todo;
    },

    async toggle(todo: Todo) {
      const result = await api.setCompleted(todo.id, !todo.completed);
      update((state) => ({
        ...state,
        items: [
          ...state.items.map((item) =>
            item.id === result.updated_todo.id ? result.updated_todo : item,
          ),
          ...(result.created_todo ? [result.created_todo] : []),
        ].sort(sortTodos),
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

    async addGroup(name: string) {
      const group = await api.createGroup(name);
      update((state) => ({
        ...state,
        groups: [...state.groups, group].sort(sortGroups),
        error: null,
      }));
      onChanged();
      return group;
    },

    async renameGroup(uuid: string, name: string) {
      const group = await api.updateGroupName(uuid, name);
      update((state) => ({
        ...state,
        groups: state.groups
          .map((item) => (item.uuid === group.uuid ? group : item))
          .sort(sortGroups),
        error: null,
      }));
      onChanged();
      return group;
    },

    async updateGroupColor(uuid: string, color: string) {
      const group = await api.updateGroupColor(uuid, color);
      update((state) => ({
        ...state,
        groups: state.groups
          .map((item) => (item.uuid === group.uuid ? group : item))
          .sort(sortGroups),
        error: null,
      }));
      onChanged();
      return group;
    },

    async deleteGroup(uuid: string) {
      const group = await api.deleteGroup(uuid);
      update((state) => ({
        ...state,
        groups: state.groups.filter((item) => item.uuid !== uuid),
        items: state.items.map((item) =>
          item.group_uuid === uuid ? { ...item, group_uuid: null } : item,
        ),
        error: null,
      }));
      onChanged();
      return group;
    },

    async reorderGroups(orderedUuids: string[]) {
      let previousGroups: TodoGroup[] = [];
      update((state) => {
        previousGroups = state.groups;
        const order = new Map(
          orderedUuids.map((uuid, index) => [uuid, index]),
        );
        return {
          ...state,
          groups: [...state.groups].sort(
            (left, right) =>
              (order.get(left.uuid) ?? Number.MAX_SAFE_INTEGER) -
              (order.get(right.uuid) ?? Number.MAX_SAFE_INTEGER),
          ),
          error: null,
        };
      });

      try {
        const groups = await api.reorderGroups(orderedUuids);
        update((state) => ({ ...state, groups, error: null }));
        onChanged();
      } catch (error) {
        update((state) => ({
          ...state,
          groups: previousGroups,
          error: getErrorMessage(error),
        }));
        throw error;
      }
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

    async setGroup(todo: Todo, groupUuid: string | null) {
      const updatedTodo = await api.setGroup(todo.id, groupUuid);
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

    async remove(id: number, repeatScope: RepeatDeleteScope = "single") {
      const result = await api.delete(id, repeatScope);
      const deletedIds = new Set(result.deleted_todos.map((todo) => todo.id));
      update((state) => ({
        ...state,
        items: state.items.filter((item) => !deletedIds.has(item.id)),
        error: null,
      }));
      onChanged();
      return result.deleted_todos;
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

function sortGroups(left: TodoGroup, right: TodoGroup) {
  return left.sort_order - right.sort_order || left.created_at - right.created_at;
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
