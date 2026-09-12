import { beforeEach, expect, it, vi } from "vitest";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import { noteHistoryApi, type HistoryPreview } from "./noteHistoryApi";
const text = { title: "Title", content: "Body", updated_at: 10, updated_by: "local" };
const preview: HistoryPreview = { entry: { id: 1, note_uuid: "note", text, captured_at: 20 }, current: { ...text, content: "Current" } };
beforeEach(() => invoke.mockReset());
it("uses bounded list, full preview and guarded restore commands", async () => {
  invoke.mockResolvedValueOnce([]).mockResolvedValueOnce(preview).mockResolvedValueOnce(true);
  expect(await noteHistoryApi.list("note")).toEqual([]);
  expect(await noteHistoryApi.preview("note", 1)).toEqual(preview);
  expect(await noteHistoryApi.restore(preview)).toBe(true);
  expect(invoke.mock.calls).toEqual([
    ["list_note_history", { uuid: "note" }], ["preview_note_history", { uuid: "note", id: 1 }],
    ["restore_note_history", { expected: preview }],
  ]);
});
it("preserves read/restore failures and no-op result", async () => {
  invoke.mockRejectedValue("NOTE_HISTORY_CONFLICT");
  for (const action of [() => noteHistoryApi.list("note"), () => noteHistoryApi.preview("note",1), () => noteHistoryApi.restore(preview)]) {
    await expect(action()).rejects.toBe("NOTE_HISTORY_CONFLICT");
  }
  invoke.mockResolvedValue(false);
  expect(await noteHistoryApi.restore(preview)).toBe(false);
});
