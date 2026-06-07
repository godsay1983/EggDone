import { get } from "svelte/store";
import { describe, expect, it, vi } from "vitest";

import type { todoApi } from "$lib/api/todoApi";
import type { Todo } from "$lib/types";
import { createTodoStore } from "./todoStore";

function makeTodo(id: number, overrides: Partial<Todo> = {}): Todo {
  return {
    id,
    uuid: `00000000-0000-4000-8000-${id.toString().padStart(12, "0")}`,
    title: `todo-${id}`,
    completed: false,
    sort_order: id * 1024,
    created_at: id,
    updated_at: id,
    completed_at: null,
    deleted_at: null,
    ...overrides,
  };
}

function createApi(initialItems: Todo[] = []) {
  const items = [...initialItems];
  const api: typeof todoApi = {
    list: vi.fn(async () => [...items]),
    create: vi.fn(async (title) => makeTodo(3, { title })),
    setCompleted: vi.fn(async (id, completed) =>
      makeTodo(id, {
        completed,
        completed_at: completed ? 100 : null,
      }),
    ),
    updateTitle: vi.fn(async (id, title) => makeTodo(id, { title })),
    reorder: vi.fn(async (orderedIds: number[]) =>
      orderedIds.map((id: number, index: number) =>
        makeTodo(id, { sort_order: index * 1024 }),
      ),
    ),
    delete: vi.fn(async (id) => makeTodo(id, { deleted_at: 100 })),
    restore: vi.fn(async (id) => makeTodo(id)),
    clearCompleted: vi.fn(async () => items.filter((todo) => todo.completed).length),
    hidePanel: vi.fn(async () => undefined),
    markPanelInteraction: vi.fn(async () => undefined),
  };
  return api;
}

describe("todo store", () => {
  it("handles the todo lifecycle", async () => {
    const first = makeTodo(1);
    const api = createApi([first]);
    const onChanged = vi.fn();
    const store = createTodoStore(api, onChanged);

    await store.load();
    await store.add("new");
    await store.edit(1, "edited");
    expect(get(store).items.find((todo) => todo.id === 1)?.title).toBe("edited");

    await store.toggle(get(store).items.find((todo) => todo.id === 1)!);
    expect(get(store).items.find((todo) => todo.id === 1)?.completed).toBe(true);

    const deleted = await store.remove(1);
    expect(deleted.deleted_at).toBe(100);
    expect(get(store).items.some((todo) => todo.id === 1)).toBe(false);

    await store.restore(1);
    expect(get(store).items.some((todo) => todo.id === 1)).toBe(true);
    expect(onChanged).toHaveBeenCalledTimes(5);
  });

  it("clears completed todos", async () => {
    const api = createApi([
      makeTodo(1),
      makeTodo(2, { completed: true, completed_at: 100 }),
    ]);
    const store = createTodoStore(api, vi.fn());

    await store.load();
    expect(await store.clearCompleted()).toBe(1);
    expect(get(store).items.map((todo) => todo.id)).toEqual([1]);
  });

  it("restores the previous order when persistence fails", async () => {
    const first = makeTodo(1, { sort_order: 0 });
    const second = makeTodo(2, { sort_order: 1024 });
    const api = createApi([first, second]);
    vi.mocked(api.reorder).mockRejectedValueOnce(new Error("offline"));
    const store = createTodoStore(api, vi.fn());

    await store.load();
    await expect(store.reorder([2, 1])).rejects.toThrow("offline");

    expect(get(store).items.map((todo) => todo.id)).toEqual([1, 2]);
    expect(get(store).error).toBe("offline");
  });

  it("refreshes synchronized todos without showing the initial loading state", async () => {
    const first = makeTodo(1);
    const api = createApi([first]);
    const store = createTodoStore(api, vi.fn());
    await store.load();

    let resolveList: ((items: Todo[]) => void) | undefined;
    vi.mocked(api.list).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveList = resolve;
        }),
    );

    const refresh = store.refresh();
    expect(get(store).loading).toBe(false);
    expect(get(store).items).toEqual([first]);

    const synchronized = makeTodo(2);
    resolveList?.([synchronized]);
    await refresh;
    expect(get(store).items).toEqual([synchronized]);
    expect(get(store).loading).toBe(false);
  });
});
