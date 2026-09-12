import { beforeEach, expect, it, vi } from "vitest";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import { trashApi, type TrashItem } from "./trashApi";
beforeEach(() => { invoke.mockReset(); });
const item: TrashItem = { kind: "note", uuid: "id", title: "Preview", content: "Body", deleted_at: 10,
  updated_at: 10, updated_by: "remote", completed: false, repeating: false, attachments: [] };
it("reads paginated trash and fetches a current preview", async () => {
  invoke.mockResolvedValueOnce([item]).mockResolvedValueOnce(item);
  expect(await trashApi.list(50, 50)).toEqual([item]);
  expect(invoke).toHaveBeenNthCalledWith(1, "list_trash", { offset: 50, limit: 50 });
  expect(await trashApi.preview("note", "id")).toEqual(item);
  expect(invoke).toHaveBeenNthCalledWith(2, "preview_trash", { kind: "note", uuid: "id" });
});
it("submits the preview without a caller clock or replacement ID", async () => {
  invoke.mockResolvedValue(undefined);
  await trashApi.restore(item);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("restore_trash", { expected: item });
});
it("preserves failures instead of claiming empty trash or successful restore", async () => {
  invoke.mockRejectedValue("TRASH_CONFLICT");
  for (const action of [() => trashApi.list(), () => trashApi.preview("note", "id"), () => trashApi.restore(item)]) {
    await expect(action()).rejects.toBe("TRASH_CONFLICT");
  }
});
