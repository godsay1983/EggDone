import { get } from "svelte/store";
import { describe, expect, it, vi } from "vitest";

import type { todoApi } from "$lib/api/todoApi";
import type { Todo, TodoGroup } from "$lib/types";
import { createTodoStore } from "./todoStore";

function makeTodo(id: number, overrides: Partial<Todo> = {}): Todo {
  return {
    id,
    uuid: `00000000-0000-4000-8000-${id.toString().padStart(12, "0")}`,
    title: `todo-${id}`,
    group_uuid: null,
    completed: false,
    pinned: false,
    sort_order: id * 1024,
    created_at: id,
    updated_at: id,
    completed_at: null,
    deleted_at: null,
    due_date: null,
    due_at: null,
    reminder_at: null,
    ...overrides,
  };
}

function makeGroup(id: number, name = `group-${id}`): TodoGroup {
  return {
    id,
    uuid: `00000000-0000-4000-8000-${(id + 100).toString().padStart(12, "0")}`,
    name,
    color: "yellow",
    sort_order: id * 1024,
    created_at: id,
    updated_at: id,
    deleted_at: null,
  };
}

function createApi(initialItems: Todo[] = []) {
  const items = [...initialItems];
  const api: typeof todoApi = {
    list: vi.fn(async () => [...items]),
    listGroups: vi.fn(async () => []),
    create: vi.fn(async (title, groupUuid = null) =>
      makeTodo(3, { title, group_uuid: groupUuid }),
    ),
    createGroup: vi.fn(async (name) => makeGroup(1, name)),
    setCompleted: vi.fn(async (id, completed) =>
      makeTodo(id, {
        completed,
        completed_at: completed ? 100 : null,
      }),
    ),
    updateTitle: vi.fn(async (id, title) => makeTodo(id, { title })),
    setPinned: vi.fn(async (id, pinned) => makeTodo(id, { pinned })),
    setSchedule: vi.fn(async (id, schedule) => makeTodo(id, schedule)),
    setGroup: vi.fn(async (id, groupUuid) => makeTodo(id, { group_uuid: groupUuid })),
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
    expect(get(store).items[0].group_uuid).toBeNull();
    await store.edit(1, "edited");
    expect(get(store).items.find((todo) => todo.id === 1)?.title).toBe("edited");

    await store.setPinned(get(store).items.find((todo) => todo.id === 1)!, true);
    expect(get(store).items.find((todo) => todo.id === 1)?.pinned).toBe(true);

    await store.setSchedule(1, {
      due_date: "2026-06-10",
      due_at: null,
      reminder_at: null,
    });
    expect(get(store).items.find((todo) => todo.id === 1)?.due_date).toBe(
      "2026-06-10",
    );

    await store.toggle(get(store).items.find((todo) => todo.id === 1)!);
    expect(get(store).items.find((todo) => todo.id === 1)?.completed).toBe(true);

    const deleted = await store.remove(1);
    expect(deleted.deleted_at).toBe(100);
    expect(get(store).items.some((todo) => todo.id === 1)).toBe(false);

    await store.restore(1);
    expect(get(store).items.some((todo) => todo.id === 1)).toBe(true);
    expect(onChanged).toHaveBeenCalledTimes(7);
  });

  it("creates groups and adds todos into the selected group", async () => {
    const api = createApi();
    const onChanged = vi.fn();
    const store = createTodoStore(api, onChanged);
    await store.load();

    const group = await store.addGroup("工作");
    await store.add("grouped", group.uuid);

    expect(get(store).groups.map((item) => item.name)).toEqual(["工作"]);
    expect(get(store).items[0].group_uuid).toBe(group.uuid);
    expect(onChanged).toHaveBeenCalledTimes(2);
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

  it("orders pinned todos before normal todos within each completion group", async () => {
    const api = createApi([
      makeTodo(1, { sort_order: 0 }),
      makeTodo(2, { sort_order: 1024 }),
      makeTodo(3, {
        completed: true,
        completed_at: 100,
        sort_order: 0,
      }),
    ]);
    vi.mocked(api.setPinned).mockImplementation(async (id, pinned) =>
      makeTodo(id, {
        pinned,
        completed: id === 3,
        completed_at: id === 3 ? 100 : null,
        sort_order: id === 1 ? 0 : 1024,
      }),
    );
    const store = createTodoStore(api, vi.fn());

    await store.load();
    await store.setPinned(get(store).items[1], true);

    expect(get(store).items.map((todo) => todo.id)).toEqual([2, 1, 3]);
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
