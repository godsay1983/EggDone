import { describe, expect, it, vi } from "vitest";
import { createNoteHistoryStore, type NoteHistoryPort } from "./noteHistoryStore";
import type { HistoryPreview } from "$lib/api/noteHistoryApi";
const preview: HistoryPreview = { entry: { id: 1, note_uuid: "n", captured_at: 10,
  text: { title: "old", content: "body", updated_at: 1, updated_by: "test" } },
  current: { title: "new", content: "body", updated_at: 2, updated_by: "test" } };
function fixture() {
  const port: NoteHistoryPort = { list: vi.fn(async () => []), preview: vi.fn(async () => preview), restore: vi.fn(async () => true) };
  return { port, store: createNoteHistoryStore(port) };
}
describe("note history orchestration", () => {
  it("lists and previews without writing", async () => {
    const { port, store } = fixture();
    await store.list("n"); await store.preview("n", 1);
    expect(port.list).toHaveBeenCalledWith("n"); expect(port.preview).toHaveBeenCalledWith("n", 1);
    expect(port.restore).not.toHaveBeenCalled();
  });
  it("refuses stale editor state before writing and releases the guard", async () => {
    const { port, store } = fixture(); const after = vi.fn(async () => {});
    await expect(store.restore(preview, () => { throw Error("pending save"); }, after)).rejects.toThrow("pending save");
    expect(port.restore).not.toHaveBeenCalled(); expect(after).not.toHaveBeenCalled();
    expect(await store.restore(preview, () => {}, after)).toEqual({ changed: true, refreshFailed: false });
  });
  it("does not turn refresh failure into a failed write", async () => {
    const { port, store } = fixture();
    expect(await store.restore(preview, () => {}, async () => { throw Error("read failed"); }))
      .toEqual({ changed: true, refreshFailed: true });
    expect(port.restore).toHaveBeenCalledTimes(1);
  });
  it("refreshes no-op text without reporting a change", async () => {
    const { port, store } = fixture(); vi.mocked(port.restore).mockResolvedValue(false);
    const after = vi.fn(async () => {});
    expect(await store.restore(preview, () => {}, after)).toEqual({ changed: false, refreshFailed: false });
    expect(after).toHaveBeenCalledWith(false);
  });
  it("does not refresh failed writes", async () => {
    const { port, store } = fixture(); vi.mocked(port.restore).mockRejectedValue(Error("NOTE_HISTORY_CONFLICT"));
    const after = vi.fn(async () => {});
    await expect(store.restore(preview, () => {}, after)).rejects.toThrow("CONFLICT");
    expect(after).not.toHaveBeenCalled();
  });
  it("blocks duplicate submissions and clones the confirmation", async () => {
    const { port, store } = fixture(); let release!: (value: boolean) => void;
    vi.mocked(port.restore).mockImplementation(() => new Promise(resolve => { release = resolve; }));
    const expected = structuredClone(preview); const after = vi.fn(async () => {});
    const first = store.restore(expected, () => {}, after);
    expected.entry.text.title = "mutated";
    await expect(store.restore(preview, () => {}, after)).rejects.toThrow("BUSY");
    expect(vi.mocked(port.restore).mock.calls[0][0].entry.text.title).toBe("old");
    release(true); await first; expect(port.restore).toHaveBeenCalledTimes(1);
  });
});
