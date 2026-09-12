import { describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { createTaskNoteLinkStore } from "./taskNoteLinkStore";
import type { LinkedTodoDraft, TaskNoteLinkView } from "$lib/types/taskNoteLink";
const draft = (): LinkedTodoDraft => ({ todo_uuid: "task", note_uuid: "note", title: "original", note: "",
  due_date: null, due_at: null, reminder_at: null, group_uuid: null, priority: 0 });
const view = { todo_title: "task" } as TaskNoteLinkView;
function fixture() {
  const api = { list: vi.fn(), create: vi.fn(), pair: vi.fn(), change: vi.fn() };
  api.list.mockResolvedValue([view]); api.create.mockResolvedValue({ uuid: "link" });
  return { api, store: createTaskNoteLinkStore(api) };
}
describe("linked note workflow", () => {
  it("retains same-note results on failure, clears other notes, and ignores late responses", async () => {
    const { api, store } = fixture();
    await store.load("a");
    api.list.mockRejectedValueOnce(Error("offline"));
    await store.load("a"); expect(get(store).items).toEqual([view]); expect(get(store).failed).toBe(true);
    let reply!: (v: TaskNoteLinkView[]) => void;
    api.list.mockImplementationOnce(() => new Promise(resolve => { reply = resolve; }));
    const pending = store.load("b"); expect(get(store).items).toEqual([]);
    await store.load(""); reply([view]); await pending;
    expect(get(store)).toEqual({ uuid: "", items: [], loading: false, failed: false });
  });
  it("saves the source first, freezes the request and blocks concurrent submits", async () => {
    const { api, store } = fixture();
    let saved!: () => void;
    const source = vi.fn(() => new Promise<void>(resolve => { saved = resolve; }));
    const effects = vi.fn().mockResolvedValue(undefined);
    const input = draft();
    const pending = store.create(input, source, effects);
    input.title = "changed";
    expect(api.create).not.toHaveBeenCalled();
    await expect(store.create(draft(), source, effects)).rejects.toThrow("BUSY");
    saved(); await pending;
    expect(api.create).toHaveBeenCalledWith(draft()); expect(effects).toHaveBeenCalledOnce();
  });
  it("preserves the stable identity across failure and does not retry committed work on refresh errors", async () => {
    const { api, store } = fixture();
    const source = vi.fn().mockRejectedValueOnce(Error("source failed")).mockResolvedValue(undefined);
    const effects = vi.fn().mockRejectedValue(Error("refresh failed"));
    await expect(store.create(draft(), source, effects)).rejects.toThrow("source failed");
    expect(api.create).not.toHaveBeenCalled();
    api.create.mockRejectedValueOnce(Error("write failed"));
    await expect(store.create(draft(), source, effects)).rejects.toThrow("write failed");
    expect(effects).not.toHaveBeenCalled();
    expect((await store.create(draft(), source, effects)).refreshFailed).toBe(true);
    expect(api.create.mock.calls.map(call => call[0].todo_uuid)).toEqual(["task", "task"]);
  });
});
