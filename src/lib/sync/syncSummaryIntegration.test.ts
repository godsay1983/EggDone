import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { SyncRuntimeSnapshot } from "$lib/api/syncApi";

vi.mock("$lib/api/syncApi", () => ({
  getSyncSettings: vi.fn(), getSyncRuntimeState: vi.fn(),
  getRemoteSyncState: vi.fn(), syncNow: vi.fn(),
}));

const settings = {
  enabled: true, credentialsConfigured: true, endpoint: "", region: "", bucket: "",
  objectKey: "", noteObjectKey: "", noteAttachmentObjectKey: "", noteAssetPrefix: "",
  pathStyle: true, allowHttp: false,
};
const snapshot: SyncRuntimeSnapshot = {
  schemaVersion: 1, lastAttemptAt: 1, lastSuccessAt: 1, dirtySince: null,
  dirtyDomains: [], lastResult: "success", lastErrorCode: null, lastErrorMessage: null,
  pendingAttachmentCount: 0, updatedAt: 1,
};
const receipt = {
  message: "sync completed", todoCount: 1, noteCount: 0, noteAttachmentCount: 0,
  pendingAttachmentCount: 0, conflictRetried: false, todoRemoteEtag: null,
  noteRemoteEtag: null, noteAttachmentRemoteEtag: null,
};
let sync: typeof import("./autoSync");
let api: typeof import("$lib/api/syncApi");
beforeEach(async () => {
  vi.resetModules();
  vi.useFakeTimers();
  api = await import("$lib/api/syncApi");
  vi.mocked(api.getSyncRuntimeState).mockReset().mockResolvedValue(snapshot);
  vi.mocked(api.getSyncSettings).mockReset().mockResolvedValue(settings);
  vi.mocked(api.syncNow).mockReset().mockResolvedValue(receipt);
  sync = await import("./autoSync");
});
afterEach(() => {
  sync.configureAutoSync({ ...settings, enabled: false });
  vi.useRealTimers();
});

it("disabled sync remains local after a saved edit", () => {
  sync.configureAutoSync({ ...settings, enabled: false });
  sync.scheduleAutoSync();
  expect(get(sync.syncSummary)).toBe("local");
});
it("missing credentials override an old success on startup", async () => {
  vi.mocked(api.getSyncSettings).mockResolvedValue({ ...settings, credentialsConfigured: false });
  await sync.initializeAutoSync();
  expect(get(sync.syncSummary)).toBe("unconfigured");
});
it.each([
  [{ pendingAttachmentCount: 2 }, "upload_pending"],
  [{ dirtyDomains: ["notes"], pendingAttachmentCount: 2 }, "pending"],
  [{ lastResult: "offline", dirtyDomains: ["notes"] }, "offline"],
  [{ lastResult: "interrupted", dirtyDomains: [] }, "pending"],
])("restores pending or failed state from runtime %j", async (patch, expected) => {
  vi.mocked(api.getSyncRuntimeState).mockResolvedValue({ ...snapshot, ...patch } as SyncRuntimeSnapshot);
  await sync.initializeAutoSync();
  expect(get(sync.syncSummary)).toBe(expected);
});
it("uses pending uploads from the receipt when diagnostics fail", async () => {
  sync.configureAutoSync(settings);
  vi.mocked(api.syncNow).mockResolvedValue({ ...receipt, pendingAttachmentCount: 2 });
  vi.mocked(api.getSyncRuntimeState).mockRejectedValue(new Error("diagnostics unavailable"));
  await sync.runManualSync();
  expect(get(sync.syncSummary)).toBe("upload_pending");
});
it("does not report synced when another edit arrives during the sync", async () => {
  sync.configureAutoSync(settings);
  let complete!: (result: typeof receipt) => void;
  vi.mocked(api.syncNow).mockReturnValue(new Promise(resolve => { complete = resolve; }));
  const running = sync.runManualSync();
  sync.scheduleAutoSync();
  complete(receipt);
  await running;
  expect(get(sync.syncSummary)).toBe("pending");
});
it("runtime dirty revisions override a successful receipt", async () => {
  sync.configureAutoSync(settings);
  vi.mocked(api.getSyncRuntimeState).mockResolvedValue({ ...snapshot, dirtyDomains: ["todos"] });
  await sync.runManualSync();
  expect(get(sync.syncSummary)).toBe("pending");
});
it("successful retry clears the earlier failure", async () => {
  sync.configureAutoSync(settings);
  vi.mocked(api.syncNow).mockRejectedValueOnce(new Error("permission denied"));
  await expect(sync.runManualSync()).rejects.toThrow();
  expect(get(sync.syncSummary)).toBe("failed");
  await sync.runManualSync();
  expect(get(sync.syncSummary)).toBe("synced");
});
it("a configuration change during diagnostics cannot restore stale success", async () => {
  sync.configureAutoSync(settings);
  let complete!: (result: SyncRuntimeSnapshot) => void;
  vi.mocked(api.getSyncRuntimeState).mockImplementation(() => new Promise(resolve => { complete = resolve; }));
  const running = sync.runManualSync();
  await vi.advanceTimersByTimeAsync(0);
  sync.configureAutoSync({ ...settings, enabled: false });
  complete(snapshot);
  await expect(running).rejects.toThrow("RECURRENCE_CONFIG_CHANGED");
  expect(get(sync.syncSummary)).toBe("local");
});
