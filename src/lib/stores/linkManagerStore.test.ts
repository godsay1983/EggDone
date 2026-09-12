import { describe, it, expect, vi } from "vitest";
import { createLinkManager, linkEndpoints } from "./linkManagerStore";
import type { TaskNoteLink, TaskNoteLinkView } from "$lib/types/taskNoteLink";
const link: TaskNoteLink = { uuid: "link", todo_uuid: "todo", note_uuid: "note", created_at: 1, updated_at: 2, updated_by: "device", deleted_at: null };
function fixture() {
  const api = { list: vi.fn().mockResolvedValue([{ link } as TaskNoteLinkView]), pair: vi.fn().mockResolvedValue(link),
    change: vi.fn().mockResolvedValue(link), create: vi.fn() };
  const candidates = vi.fn().mockResolvedValue([{ uuid: "note", title: "linked" }, { uuid: "candidate", title: "choice" }]);
  return { api, candidates, manager: createLinkManager(api, candidates) };
}
describe("link manager", () => {
  it("maps both scopes and excludes active links without hiding read errors", async () => {
    expect(linkEndpoints("todo", "todo", "note")).toEqual(linkEndpoints("note", "note", "todo"));
    const { api, manager, candidates } = fixture();
    expect((await manager.load("todo", "todo")).candidates).toEqual([{ uuid: "candidate", title: "choice" }]);
    candidates.mockResolvedValueOnce([{ uuid: "todo", title: "linked" }]);
    expect((await manager.load("note", "note")).candidates).toEqual([]);
    candidates.mockRejectedValueOnce(Error("read failed"));
    await expect(manager.load("note", "note")).rejects.toThrow("read failed");
    api.list.mockRejectedValueOnce(Error("links failed"));
    await expect(manager.load("todo", "todo")).rejects.toThrow("links failed");
  });
  it("retains the observed tombstone instead of rereading before confirmation", async () => {
    const { api, manager } = fixture();
    const tombstone = { ...link, deleted_at: 3, updated_at: 3 };
    api.pair.mockResolvedValueOnce(tombstone);
    const change = await manager.prepare("note", "note", "todo");
    tombstone.updated_at = 100;
    await manager.change(change, async () => {}, async () => {});
    expect(api.pair).toHaveBeenCalledOnce();
    expect(api.change).toHaveBeenCalledWith("todo", "note", true, { ...link, deleted_at: 3, updated_at: 3 });
  });
  it("saves the source first, blocks double submit and copies the expected version", async () => {
    const { api, manager } = fixture();
    const change = { todoUuid: "todo", noteUuid: "note", active: false, expected: { ...link } };
    let release!: () => void;
    const saving = manager.change(change, () => new Promise<void>(resolve => { release = resolve; }), async () => {});
    change.expected.updated_at = 9;
    expect(api.change).not.toHaveBeenCalled();
    await expect(manager.change(change, async () => {}, async () => {})).rejects.toThrow("BUSY");
    release(); await saving;
    expect(api.change).toHaveBeenCalledWith("todo", "note", false, link);
  });
  it("never retries conflicts automatically and separates committed refresh failures", async () => {
    const { api, manager } = fixture();
    const change = await manager.prepare("todo", "todo", "note");
    const effect = vi.fn().mockResolvedValue(undefined);
    await expect(manager.change(change, async () => { throw Error("save failed"); }, effect)).rejects.toThrow("save failed");
    expect(api.change).not.toHaveBeenCalled();
    api.change.mockRejectedValueOnce(Error("CONFLICT"));
    await expect(manager.change(change, async () => {}, effect)).rejects.toThrow("CONFLICT");
    expect(api.change).toHaveBeenCalledOnce(); expect(effect).not.toHaveBeenCalled();
    effect.mockRejectedValueOnce(Error("refresh failed"));
    expect(await manager.change(change, async () => {}, effect)).toBe(true);
  });
});
