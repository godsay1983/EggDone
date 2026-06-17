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
    note: null,
    group_uuid: null,
    completed: false,
    pinned: false,
    priority: 0,
    sort_order: id * 1024,
    created_at: id,
    updated_at: id,
    completed_at: null,
    deleted_at: null,
    archived_at: null,
    due_date: null,
    due_at: null,
    reminder_at: null,
    repeat_rule: null,
    repeat_next_due_date: null,
    repeat_series_uuid: null,
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
    updateGroupName: vi.fn(async (uuid, name) => ({
      ...makeGroup(1, name),
      uuid,
      updated_at: 100,
    })),
    updateGroupColor: vi.fn(async (uuid, color) => ({
      ...makeGroup(1),
      uuid,
      color,
      updated_at: 100,
    })),
    deleteGroup: vi.fn(async (uuid) => ({
      ...makeGroup(1),
      uuid,
      deleted_at: 100,
    })),
    reorderGroups: vi.fn(async (orderedUuids: string[]) =>
      orderedUuids.map((uuid: string, index: number) => ({
        ...makeGroup(index + 1),
        uuid,
        sort_order: index * 1024,
      })),
    ),
    setCompleted: vi.fn(async (id, completed) => ({
      updated_todo: makeTodo(id, {
        completed,
        completed_at: completed ? 100 : null,
      }),
      created_todo: null,
    })),
    setCompletedByUuid: vi.fn(async (uuid, completed) => ({
      updated_todo: makeTodo(1, {
        uuid,
        completed,
        completed_at: completed ? 100 : null,
      }),
      created_todo: null,
    })),
    updateTitle: vi.fn(async (id, title) => ({
      updated_todos: [makeTodo(id, { title })],
    })),
    updateNote: vi.fn(async (id, note) => ({
      updated_todos: [makeTodo(id, { note })],
    })),
    setPinned: vi.fn(async (id, pinned) => makeTodo(id, { pinned })),
    setPriority: vi.fn(async (id, priority) => makeTodo(id, { priority })),
    setSchedule: vi.fn(async (id, schedule) => ({
      updated_todos: [makeTodo(id, schedule)],
    })),
    setGroup: vi.fn(async (id, groupUuid) => ({
      updated_todos: [makeTodo(id, { group_uuid: groupUuid })],
    })),
    reorder: vi.fn(async (orderedIds: number[]) =>
      orderedIds.map((id: number, index: number) =>
        makeTodo(id, { sort_order: index * 1024 }),
      ),
    ),
    delete: vi.fn(async (id) => ({
      deleted_todos: [makeTodo(id, { deleted_at: 100 })],
    })),
    restore: vi.fn(async (id) => makeTodo(id)),
    clearCompleted: vi.fn(async () => items.filter((todo) => todo.completed).length),
    archiveCompleted: vi.fn(async () => items.filter((todo) => todo.completed).length),
    hidePanel: vi.fn(async () => undefined),
    openFocusWindow: vi.fn(async () => undefined),
    publishFocusNotification: vi.fn(async () => undefined),
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

    await store.setNote(1, "context");
    expect(get(store).items.find((todo) => todo.id === 1)?.note).toBe("context");

    await store.setPinned(get(store).items.find((todo) => todo.id === 1)!, true);
    expect(get(store).items.find((todo) => todo.id === 1)?.pinned).toBe(true);

    await store.setSchedule(1, {
      due_date: "2026-06-10",
      due_at: null,
      reminder_at: null,
      repeat_rule: null,
    });
    expect(get(store).items.find((todo) => todo.id === 1)?.due_date).toBe(
      "2026-06-10",
    );

    await store.toggle(get(store).items.find((todo) => todo.id === 1)!);
    expect(get(store).items.find((todo) => todo.id === 1)?.completed).toBe(true);

    const deleted = await store.remove(1);
    expect(deleted[0].deleted_at).toBe(100);
    expect(get(store).items.some((todo) => todo.id === 1)).toBe(false);

    await store.restore(1);
    expect(get(store).items.some((todo) => todo.id === 1)).toBe(true);
    expect(onChanged).toHaveBeenCalledTimes(8);
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

  it("renames reorders and deletes groups", async () => {
    const first = makeGroup(1, "工作");
    const second = makeGroup(2, "生活");
    const api = createApi([makeTodo(1, { group_uuid: first.uuid })]);
    vi.mocked(api.listGroups).mockResolvedValue([first, second]);
    const onChanged = vi.fn();
    const store = createTodoStore(api, onChanged);
    await store.load();

    await store.renameGroup(first.uuid, "深度工作");
    expect(get(store).groups[0].name).toBe("深度工作");

    await store.updateGroupColor(first.uuid, "green");
    expect(get(store).groups[0].color).toBe("green");

    await store.reorderGroups([second.uuid, first.uuid]);
    expect(get(store).groups.map((group) => group.uuid)).toEqual([
      second.uuid,
      first.uuid,
    ]);

    await store.deleteGroup(first.uuid);
    expect(get(store).groups.map((group) => group.uuid)).toEqual([second.uuid]);
    expect(get(store).items[0].group_uuid).toBeNull();
    expect(onChanged).toHaveBeenCalledTimes(4);
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

  it("archives completed todos", async () => {
    const api = createApi([
      makeTodo(1),
      makeTodo(2, { completed: true, completed_at: 100 }),
    ]);
    const onChanged = vi.fn();
    const store = createTodoStore(api, onChanged);

    await store.load();
    expect(await store.archiveCompleted()).toBe(1);
    expect(get(store).items.map((todo) => todo.id)).toEqual([1]);
    expect(onChanged).toHaveBeenCalledTimes(1);
  });

  it("applies batch todo operations", async () => {
    const group = makeGroup(1, "工作");
    const api = createApi([
      makeTodo(1),
      makeTodo(2),
      makeTodo(3, { completed: true, completed_at: 100 }),
    ]);
    const onChanged = vi.fn();
    const store = createTodoStore(api, onChanged);

    await store.load();
    await store.completeMany([1, 3]);
    expect(get(store).items.find((todo) => todo.id === 1)?.completed).toBe(true);
    expect(api.setCompleted).toHaveBeenCalledTimes(1);

    await store.moveManyToGroup([1, 2], group.uuid);
    expect(get(store).items.filter((todo) => todo.group_uuid === group.uuid).length).toBe(2);

    const deleted = await store.removeMany([1, 2]);
    expect(deleted.map((todo) => todo.id)).toEqual([1, 2]);
    expect(get(store).items.map((todo) => todo.id)).toEqual([3]);
    expect(api.delete).toHaveBeenCalledWith(1, "single");
    expect(api.delete).toHaveBeenCalledWith(2, "single");
    expect(onChanged).toHaveBeenCalledTimes(3);
  });

  it("adds the generated next instance when completing a repeating todo", async () => {
    const current = makeTodo(1, {
      due_date: "2026-06-10",
      repeat_rule: "daily",
      repeat_next_due_date: "2026-06-11",
    });
    const next = makeTodo(2, {
      due_date: "2026-06-11",
      repeat_rule: "daily",
      repeat_next_due_date: "2026-06-12",
      sort_order: -1024,
    });
    const api = createApi([current]);
    vi.mocked(api.setCompleted).mockResolvedValueOnce({
      updated_todo: makeTodo(1, {
        completed: true,
        completed_at: 100,
        due_date: "2026-06-10",
        repeat_rule: "daily",
        repeat_next_due_date: "2026-06-11",
      }),
      created_todo: next,
    });
    const store = createTodoStore(api, vi.fn());

    await store.load();
    await store.toggle(current);

    expect(get(store).items.map((todo) => todo.id)).toEqual([2, 1]);
  });

  it("removes all todos returned by a repeat series deletion", async () => {
    const current = makeTodo(1, {
      repeat_rule: "daily",
      repeat_next_due_date: "2026-06-11",
      repeat_series_uuid: "00000000-0000-4000-8000-000000000001",
    });
    const next = makeTodo(2, {
      repeat_rule: "daily",
      repeat_next_due_date: "2026-06-12",
      repeat_series_uuid: "00000000-0000-4000-8000-000000000001",
    });
    const api = createApi([current, next, makeTodo(3)]);
    vi.mocked(api.delete).mockResolvedValueOnce({
      deleted_todos: [
        { ...current, deleted_at: 100 },
        { ...next, deleted_at: 100 },
      ],
    });
    const store = createTodoStore(api, vi.fn());

    await store.load();
    const deleted = await store.remove(2, "series");

    expect(deleted.map((todo) => todo.id)).toEqual([1, 2]);
    expect(get(store).items.map((todo) => todo.id)).toEqual([3]);
    expect(api.delete).toHaveBeenCalledWith(2, "series");
  });

  it("applies future repeat edit results returned by the backend", async () => {
    const current = makeTodo(1, {
      title: "old",
      repeat_rule: "daily",
      repeat_next_due_date: "2026-06-11",
      repeat_series_uuid: "00000000-0000-4000-8000-000000000001",
    });
    const next = makeTodo(2, {
      title: "old",
      repeat_rule: "daily",
      repeat_next_due_date: "2026-06-12",
      repeat_series_uuid: "00000000-0000-4000-8000-000000000001",
    });
    const api = createApi([current, next]);
    vi.mocked(api.updateTitle).mockResolvedValueOnce({
      updated_todos: [
        { ...current, title: "new" },
        { ...next, title: "new" },
      ],
    });
    const store = createTodoStore(api, vi.fn());

    await store.load();
    await store.edit(current.id, "new", "future");

    expect(get(store).items.map((todo) => todo.title)).toEqual(["new", "new"]);
    expect(api.updateTitle).toHaveBeenCalledWith(current.id, "new", "future");
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
