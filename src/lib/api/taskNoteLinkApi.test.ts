import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LinkedTodoDraft, TaskNoteLink } from "$lib/types/taskNoteLink";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import { taskNoteLinkApi } from "./taskNoteLinkApi";

const link: TaskNoteLink = {
  uuid: "link", todo_uuid: "todo", note_uuid: "note", created_at: 1,
  updated_at: 2, updated_by: "device", deleted_at: 2,
};
const draft: LinkedTodoDraft = {
  todo_uuid: "todo", note_uuid: "note", title: "task", note: "details",
  group_uuid: null, due_date: null, due_at: null, reminder_at: null, priority: 0,
};

describe("task-note link command boundary", () => {
  beforeEach(() => { invoke.mockReset(); });

  it("loads scoped lists and the current pair including tombstones", async () => {
    invoke.mockResolvedValueOnce([]).mockResolvedValueOnce(link);
    expect(await taskNoteLinkApi.list("note", "note")).toEqual([]);
    expect(invoke).toHaveBeenNthCalledWith(1, "list_task_note_links", { scope: "note", uuid: "note" });
    expect(await taskNoteLinkApi.pair("todo", "note")).toEqual(link);
    expect(invoke).toHaveBeenNthCalledWith(2, "get_task_note_link", { todoUuid: "todo", noteUuid: "note" });
  });

  it("creates atomically without caller-provided clocks or identity", async () => {
    invoke.mockResolvedValue(link);
    expect(await taskNoteLinkApi.create(draft)).toEqual(link);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("create_linked_todo", { draft });
  });

  it("passes expected versions unchanged for explicit relink and unlink", async () => {
    invoke.mockResolvedValueOnce(link).mockResolvedValueOnce(null);
    expect(await taskNoteLinkApi.change("todo", "note", true, link)).toEqual(link);
    expect(invoke).toHaveBeenNthCalledWith(1, "change_task_note_link", {
      todoUuid: "todo", noteUuid: "note", active: true, expected: link,
    });
    expect(await taskNoteLinkApi.change("todo", "note", false, null)).toBeNull();
    expect(invoke).toHaveBeenNthCalledWith(2, "change_task_note_link", {
      todoUuid: "todo", noteUuid: "note", active: false, expected: null,
    });
  });

  it("does not disguise read or commit failures as empty lists or successful changes", async () => {
    invoke.mockRejectedValue("TASK_NOTE_LINK_CONFLICT");
    for (const action of [() => taskNoteLinkApi.list("todo", "todo"), () => taskNoteLinkApi.pair("todo", "note"),
      () => taskNoteLinkApi.create(draft), () => taskNoteLinkApi.change("todo", "note", true, link)]) {
      await expect(action()).rejects.toBe("TASK_NOTE_LINK_CONFLICT");
    }
  });
});
