import { describe, expect, it, vi } from "vitest";
import { createTrashStore, type TrashPort } from "./trashStore";
import type { TrashItem } from "$lib/api/trashApi";

const item: TrashItem = { kind: "todo", uuid: "test", title: "Task", content: "", deleted_at: 1,
  updated_at: 1, updated_by: "remote", completed: false, repeating: false, attachments: [] };
function fixture() {
  const port: TrashPort = { list: vi.fn(async () => [item]), preview: vi.fn(async () => item), restore: vi.fn(async () => {}) };
  return { port, store: createTrashStore(port) };
}
describe("trash store", () => {
  it("pages and gets a fresh preview without writing", async () => {
    const { port, store } = fixture();
    await store.list(50); await store.preview(item);
    expect(port.list).toHaveBeenCalledWith(50, 50);
    expect(port.preview).toHaveBeenCalledWith(item.kind, item.uuid);
    expect(port.restore).not.toHaveBeenCalled();
  });
  it("separates a successful write from failed refresh", async () => {
    const { port, store } = fixture();
    const after = vi.fn(async () => { throw Error("refresh"); });
    expect(await store.restore(item, after)).toBe(true);
    expect(port.restore).toHaveBeenCalledTimes(1);
    expect(after).toHaveBeenCalledTimes(1);
    await store.list();
    expect(port.restore).toHaveBeenCalledTimes(1);
  });
  it("does not refresh failed writes and releases its guard", async () => {
    const { port, store } = fixture();
    vi.mocked(port.restore).mockRejectedValueOnce(Error("TRASH_CONFLICT"));
    const after = vi.fn(async () => {});
    await expect(store.restore(item, after)).rejects.toThrow("TRASH_CONFLICT");
    expect(after).not.toHaveBeenCalled();
    expect(await store.restore(item, after)).toBe(false);
  });
  it("blocks duplicate submissions and isolates the confirmation object", async () => {
    const { port, store } = fixture();
    let release!: () => void;
    vi.mocked(port.restore).mockImplementation(() => new Promise<void>(resolve => { release = resolve; }));
    const expected = structuredClone(item);
    const first = store.restore(expected, async () => {});
    expected.title = "changed";
    await expect(store.restore(item, async () => {})).rejects.toThrow("TRASH_BUSY");
    expect(vi.mocked(port.restore).mock.calls[0][0].title).toBe("Task");
    release(); await first;
    expect(port.restore).toHaveBeenCalledTimes(1);
  });
});
