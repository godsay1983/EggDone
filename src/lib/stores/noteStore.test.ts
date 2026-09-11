import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { noteApi } from "$lib/api/noteApi";
import type { Note } from "$lib/types";
import { createNoteStore } from "./noteStore";

function makeNote(id: number, overrides: Partial<Note> = {}): Note {
  return {
    id,
    uuid: `00000000-0000-4000-8000-${id.toString().padStart(12, "0")}`,
    title: `note-${id}`,
    content: `content-${id}`,
    color: "default",
    pinned: false,
    created_at: id,
    updated_at: id,
    deleted_at: null,
    updated_by: "11111111-1111-4111-8111-111111111111",
    ...overrides,
  };
}

function createApi(initialItems: Note[] = []) {
  const api: typeof noteApi = {
    list: vi.fn(async () => [...initialItems]),
    create: vi.fn(async (title, content, color = "default") =>
      makeNote(3, { title, content, color, updated_at: 100 }),
    ),
    update: vi.fn(async (uuid, title, content) =>
      makeNote(1, { uuid, title, content, updated_at: 200 }),
    ),
    setPinned: vi.fn(async (uuid, pinned) =>
      makeNote(1, { uuid, pinned, updated_at: 300 }),
    ),
    setColor: vi.fn(async (uuid, color) =>
      makeNote(1, { uuid, color, updated_at: 300 }),
    ),
    delete: vi.fn(async (uuid) =>
      makeNote(1, { uuid, deleted_at: 400, updated_at: 400 }),
    ),
    restore: vi.fn(async (uuid) => makeNote(1, { uuid, updated_at: 500 })),
  };
  return api;
}

afterEach(() => {
  vi.useRealTimers();
});

describe("note store", () => {
  it("retains failed edits through refresh and retries without typing again", async () => {
    const note = makeNote(1);
    const api = createApi([note]);
    const changed = vi.fn();
    const store = createNoteStore(api, changed);
    await store.load();
    vi.mocked(api.update).mockRejectedValueOnce(new Error('save failed'));
    store.scheduleUpdate(note, 'retained', 'latest');
    await expect(store.flushPending()).rejects.toThrow('save failed');
    expect(get(store).saving).toBe(false);
    expect(changed).not.toHaveBeenCalled();
    await store.refresh();
    expect(get(store).error).toBe('save failed');
    await store.flushPending();
    expect(api.update).toHaveBeenCalledTimes(2);
    expect(get(store).items[0].title).toBe('retained');
    expect(get(store).error).toBeNull();
  });

  it("joins in-flight saves and drains newer edits before resolving either flush", async () => {
    const note = makeNote(1);
    const api = createApi([note]);
    let finish!: (note: Note) => void;
    vi.mocked(api.update).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const store = createNoteStore(api, vi.fn());
    await store.load();
    store.scheduleUpdate(note, 'first', 'one');
    let resolved = false;
    const first = store.flushPending();
    const second = store.flushPending().then(() => { resolved = true; });
    await Promise.resolve();
    expect(resolved).toBe(false);
    expect(get(store).saving).toBe(true);
    store.scheduleUpdate(note, 'last', 'two');
    finish({ ...note, title: 'first', content: 'one' });
    await Promise.all([first, second]);
    expect(api.update).toHaveBeenCalledTimes(2);
    expect(get(store).items[0].title).toBe('last');
    expect(get(store).saving).toBe(false);
  });

  it("keeps different note UUIDs queued independently", async () => {
    const api = createApi([makeNote(1), makeNote(2)]);
    const store = createNoteStore(api, vi.fn());
    await store.load();
    store.scheduleUpdate(makeNote(1), 'one', 'a');
    store.scheduleUpdate(makeNote(2), 'two', 'b');
    await store.flushPending();
    expect(api.update).toHaveBeenNthCalledWith(1, makeNote(1).uuid, 'one', 'a');
    expect(api.update).toHaveBeenNthCalledWith(2, makeNote(2).uuid, 'two', 'b');
  });

  it("cancel does not retry a failed in-flight write", async () => {
    const note = makeNote(1);
    const api = createApi([note]);
    let fail!: (error: Error) => void;
    vi.mocked(api.update).mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject; }));
    const store = createNoteStore(api, vi.fn());
    store.scheduleUpdate(note, 'cancelled', 'body');
    const result = store.flushPending().catch(() => undefined);
    store.cancelPending();
    expect(get(store).saving).toBe(true);
    fail(new Error('failed'));
    await result;
    await store.flushPending();
    expect(api.update).toHaveBeenCalledTimes(1);
  });

  it("a failed old snapshot never replaces edits typed during that write", async () => {
    const note = makeNote(1);
    const api = createApi([note]);
    let fail!: (error: Error) => void;
    vi.mocked(api.update).mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject; }));
    const store = createNoteStore(api, vi.fn());
    await store.load();
    store.scheduleUpdate(note, 'old', 'a');
    const result = store.flushPending().catch(() => undefined);
    store.scheduleUpdate(note, 'new', 'b');
    fail(new Error('failed'));
    await result;
    await store.flushPending();
    expect(api.update).toHaveBeenLastCalledWith(note.uuid, 'new', 'b');
    expect(get(store).items[0].title).toBe('new');
  });

  it("pinning cannot hide an unresolved save failure", async () => {
    const note = makeNote(1);
    const api = createApi([note]);
    vi.mocked(api.update).mockRejectedValue(new Error('failed'));
    const store = createNoteStore(api, vi.fn());
    store.scheduleUpdate(note, 'pending', 'body');
    await expect(store.setPinned(note, true)).rejects.toThrow();
    expect(api.setPinned).not.toHaveBeenCalled();
    expect(get(store).error).toBe('failed');
    store.cancelPending();
  });

  it("loads creates edits and marks local changes dirty", async () => {
    const api = createApi([makeNote(1)]);
    const changed = vi.fn();
    const store = createNoteStore(api, changed);

    await store.load();
    expect(get(store).dirty).toBe(false);
    await store.add("new", "body", "yellow");
    expect(get(store).items[0].title).toBe("new");
    expect(get(store).dirty).toBe(true);
    expect(get(store).dirtySince).not.toBeNull();

    store.markSynced();
    expect(get(store).dirty).toBe(false);
    expect(changed).toHaveBeenCalledTimes(1);
  });

  it("debounces editor saves for 600 milliseconds and flushes on demand", async () => {
    vi.useFakeTimers();
    const note = makeNote(1);
    const api = createApi([note]);
    const changed = vi.fn();
    const store = createNoteStore(api, changed);
    await store.load();

    store.scheduleUpdate(note, "first", "draft");
    store.scheduleUpdate(note, "latest", "body");
    await vi.advanceTimersByTimeAsync(599);
    expect(api.update).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(api.update).toHaveBeenCalledTimes(1);
    expect(api.update).toHaveBeenCalledWith(note.uuid, "latest", "body");
    expect(get(store).items[0].title).toBe("latest");
    expect(get(store).saving).toBe(false);
    expect(changed).toHaveBeenCalledTimes(1);

    store.scheduleUpdate(get(store).items[0], "flushed", "now");
    await store.flushPending();
    expect(api.update).toHaveBeenCalledTimes(2);
    expect(get(store).items[0].title).toBe("flushed");
  });

  it("pins colors removes and restores notes", async () => {
    const note = makeNote(1);
    const api = createApi([note]);
    const store = createNoteStore(api, vi.fn());
    await store.load();

    const pinned = await store.setPinned(note, true);
    expect(pinned.pinned).toBe(true);
    const colored = await store.setColor(pinned, "blue");
    expect(colored.color).toBe("blue");
    const deleted = await store.remove(colored);
    expect(get(store).items).toEqual([]);
    await store.restore(deleted);
    expect(get(store).items).toHaveLength(1);
  });

  it("stores load and save failures in state", async () => {
    const api = createApi();
    vi.mocked(api.list).mockRejectedValueOnce(new Error("load failed"));
    const store = createNoteStore(api, vi.fn());

    await store.load();
    expect(get(store).error).toBe("load failed");

    vi.mocked(api.create).mockRejectedValueOnce(new Error("save failed"));
    await expect(store.add("title", "body")).rejects.toThrow("save failed");
    expect(get(store).error).toBe("save failed");
  });
});
