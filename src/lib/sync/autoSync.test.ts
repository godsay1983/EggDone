import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as syncApi from "$lib/api/syncApi";
import {
  configureAutoSync,
  runManualSync,
  scheduleAutoSync,
  setAutoSyncForeground,
  syncStatus,
  savedSyncSettings,
  runSettingsUpdate,
  syncOnPanelShown,
} from "./autoSync";

vi.mock("$lib/api/syncApi", () => ({
  getSyncSettings: vi.fn(),
  getRemoteSyncState: vi.fn(),
  syncNow: vi.fn(),
}));

const enabledSettings = {
  enabled: true,
  endpoint: "http://127.0.0.1:9000",
  region: "us-east-1",
  bucket: "eggdone",
  objectKey: "todos.json",
  noteObjectKey: "notes.json",
  noteAttachmentObjectKey: "note-attachments.json",
  noteAssetPrefix: "note-assets/v1/",
  pathStyle: true,
  allowHttp: true,
  credentialsConfigured: true,
};

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
  setAutoSyncForeground(false);
  configureAutoSync({ ...enabledSettings, enabled: false });
});

describe("auto sync", () => {
  it("does not queue another sync when focus or polling occurs during an active sync", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    const active = deferred<syncApi.ManualSyncResult>();
    vi.mocked(syncApi.syncNow).mockReturnValueOnce(active.promise).mockResolvedValue(syncResult());
    const first = runManualSync();
    setAutoSyncForeground(true);
    await vi.advanceTimersByTimeAsync(60_000);
    const joined = runManualSync();
    active.resolve(syncResult());
    await Promise.all([first, joined]);
    expect(get(syncStatus).kind).toBe('synced');
    await vi.advanceTimersByTimeAsync(4_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
  });

  it("preserves local changes made during an active sync", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    const active = deferred<syncApi.ManualSyncResult>();
    vi.mocked(syncApi.syncNow).mockReturnValueOnce(active.promise).mockResolvedValue(syncResult());
    const first = runManualSync();
    scheduleAutoSync();
    const joined = runManualSync();
    active.resolve(syncResult());
    await Promise.all([first, joined]);
    expect(get(syncStatus).kind).toBe('pending');
    await vi.advanceTimersByTimeAsync(4_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
    expect(get(syncStatus).kind).toBe('synced');
  });

  it("syncs immediately on native panel show and coalesces focus and repeated show events", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    const active = deferred<syncApi.ManualSyncResult>();
    vi.mocked(syncApi.syncNow).mockReturnValueOnce(active.promise).mockResolvedValue(syncResult());
    scheduleAutoSync();
    syncOnPanelShown();
    setAutoSyncForeground(true);
    syncOnPanelShown();
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    expect(syncApi.getRemoteSyncState).not.toHaveBeenCalled();
    expect(get(syncStatus).kind).toBe('syncing');
    active.resolve(syncResult());
    await vi.advanceTimersByTimeAsync(4_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    expect(get(syncStatus).kind).toBe('synced');
    setAutoSyncForeground(false);
    syncOnPanelShown();
    await vi.advanceTimersByTimeAsync(0);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });

  it("does not sync on panel show when synchronization is disabled", () => {
    configureAutoSync({ ...enabledSettings, enabled: false });
    syncOnPanelShown();
    expect(syncApi.syncNow).not.toHaveBeenCalled();
    expect(get(syncStatus).kind).toBe('idle');
  });

  it("consumes the local debounce when a foreground probe starts synchronization", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remoteProbe());
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    scheduleAutoSync();
    setAutoSyncForeground(true);
    await vi.advanceTimersByTimeAsync(4_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
  });

  it("does not discard a probe on duplicate foreground notifications", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    const probe = deferred<syncApi.RemoteSyncState>();
    vi.mocked(syncApi.getRemoteSyncState).mockReturnValueOnce(probe.promise);
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    setAutoSyncForeground(true);
    setAutoSyncForeground(true);
    probe.resolve(remoteProbe());
    await vi.advanceTimersByTimeAsync(0);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
  });

  it("rechecks immediately when the panel reopens before an old probe finishes", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    const probe = deferred<syncApi.RemoteSyncState>();
    vi.mocked(syncApi.getRemoteSyncState).mockReturnValueOnce(probe.promise).mockResolvedValue(remoteProbe());
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    setAutoSyncForeground(true);
    setAutoSyncForeground(false);
    setAutoSyncForeground(true);
    probe.resolve(remoteProbe());
    await vi.advanceTimersByTimeAsync(0);
    expect(syncApi.getRemoteSyncState).toHaveBeenCalledTimes(2);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
  });

  it("serializes form saves after the current sync and queues later syncs until settings are saved", async () => {
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getSyncSettings).mockResolvedValue(enabledSettings);
    const first = deferred<syncApi.ManualSyncResult>();
    vi.mocked(syncApi.syncNow).mockReturnValueOnce(first.promise).mockResolvedValue(syncResult());
    const active = runManualSync();
    const saved = deferred<void>();
    let saving = false;
    const update = runSettingsUpdate(async () => {
      saving = true;
      await saved.promise;
      configureAutoSync({ ...enabledSettings, objectKey: 'verified/todos.json' });
    });
    expect(saving).toBe(false);
    const queued = runManualSync();
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    first.resolve(syncResult());
    await active;
    await Promise.resolve();
    expect(saving).toBe(true);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    saved.resolve(); await update; await queued;
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
    expect(get(syncStatus).kind).toBe('synced');
  });
  it("detects an association change with unchanged old ETags and does not invalidate its session", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    const remote = remoteProbe();
    const result = { ...syncResult(), recurrenceRemoteToken: remote.recurrenceToken };
    vi.mocked(syncApi.getSyncSettings).mockResolvedValue(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remote);
    vi.mocked(syncApi.syncNow).mockResolvedValue(result);
    setAutoSyncForeground(true);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remote, targetChanged: true });
    vi.mocked(syncApi.getSyncSettings).mockResolvedValue({ ...enabledSettings, objectKey: 'verified/todos.json' });
    vi.mocked(syncApi.syncNow).mockResolvedValue({ ...result, targetChanged: true });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
    expect(get(savedSyncSettings)?.objectKey).toBe('verified/todos.json');
    expect(get(syncStatus).kind).toBe('synced');
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remote);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });

  it("reads the persisted target after failure and retries even when the later probe is unchanged", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    const remote = remoteProbe();
    const result = { ...syncResult(), recurrenceRemoteToken: remote.recurrenceToken };
    vi.mocked(syncApi.getSyncSettings).mockResolvedValue(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remote);
    vi.mocked(syncApi.syncNow).mockResolvedValue(result);
    setAutoSyncForeground(true);
    await vi.advanceTimersByTimeAsync(0);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remote, targetChanged: true });
    vi.mocked(syncApi.getSyncSettings).mockResolvedValue({ ...enabledSettings, objectKey: 'verified/todos.json' });
    vi.mocked(syncApi.syncNow).mockRejectedValueOnce(Error('SYNC_AUTO_JOIN_UNSAFE'));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(get(syncStatus).kind).toBe('failed');
    expect(get(savedSyncSettings)?.objectKey).toBe('verified/todos.json');
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remote);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(3);
    expect(get(syncStatus).kind).toBe('synced');
  });
  it.each([
    ['planToken', 'planRemoteToken'], ['workflowToken', 'workflowRemoteToken'],
  ] as const)("syncs a %s-only remote change and consumes only its successful receipt", async (token, receipt) => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    const remote = {
      recurrenceToken: "missing", [token]: 'etag:"plan-1"',
      todoObjectExists: true, todoEtag: '"todo"',
      noteObjectExists: true, noteEtag: '"note"',
      noteAttachmentObjectExists: true, noteAttachmentEtag: '"attachment"',
    };
    const result = {
      message: "Synced", todoCount: 0, noteCount: 0, noteAttachmentCount: 0,
      pendingAttachmentCount: 0, conflictRetried: false,
      recurrenceRemoteToken: "missing", [receipt]: 'etag:"plan-1"',
      todoRemoteEtag: '"todo"', noteRemoteEtag: '"note"', noteAttachmentRemoteEtag: '"attachment"',
    };
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remote);
    vi.mocked(syncApi.syncNow).mockResolvedValue(result);
    setAutoSyncForeground(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remote, [token]: 'etag:"plan-2"' });
    vi.mocked(syncApi.syncNow).mockRejectedValueOnce(new Error("PLAN_REMOTE_MISSING"));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
    vi.mocked(syncApi.syncNow).mockResolvedValue({ ...result, [receipt]: 'etag:"plan-2"' });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(3);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(3);
  });

  it("debounces local changes for four seconds", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.syncNow).mockResolvedValue({
      message: "同步完成",
      todoCount: 1,
      noteCount: 1,
      noteAttachmentCount: 0,
      pendingAttachmentCount: 0,
      conflictRetried: false,
      todoRemoteEtag: "\"etag-1\"",
      noteRemoteEtag: "\"note-etag-1\"",
      noteAttachmentRemoteEtag: "\"attachment-etag-1\"",
    });

    scheduleAutoSync();
    scheduleAutoSync();
    await vi.advanceTimersByTimeAsync(3_999);
    expect(syncApi.syncNow).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
  });

  it("retries retryable failures with bounded backoff", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.syncNow)
      .mockRejectedValueOnce(new Error("connection refused"))
      .mockRejectedValueOnce(new Error("timeout"))
      .mockResolvedValueOnce({
        message: "同步完成",
        todoCount: 2,
        noteCount: 1,
        noteAttachmentCount: 1,
        pendingAttachmentCount: 0,
        conflictRetried: false,
        todoRemoteEtag: "\"etag-2\"",
        noteRemoteEtag: "\"note-etag-2\"",
        noteAttachmentRemoteEtag: "\"attachment-etag-2\"",
      });

    const resultPromise = runManualSync();
    await vi.advanceTimersByTimeAsync(4_500);

    await expect(resultPromise).resolves.toMatchObject({ todoCount: 2 });
    expect(syncApi.syncNow).toHaveBeenCalledTimes(3);
    expect(get(syncStatus).kind).toBe("synced");
  });

  it("retries transient note and attachment stage failures", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.syncNow)
      .mockRejectedValueOnce(
        new Error("附件元数据同步失败：上传附件元数据失败，S3 服务返回状态码 503"),
      )
      .mockResolvedValueOnce({
        message: "同步完成",
        todoCount: 1,
        noteCount: 1,
        noteAttachmentCount: 1,
        pendingAttachmentCount: 0,
        conflictRetried: false,
        todoRemoteEtag: '"etag"',
        noteRemoteEtag: '"note-etag"',
        noteAttachmentRemoteEtag: '"attachment-etag"',
      });

    const resultPromise = runManualSync();
    await vi.advanceTimersByTimeAsync(1_500);

    await expect(resultPromise).resolves.toMatchObject({ noteAttachmentCount: 1 });
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });

  it.each(["RECURRENCE_TRANSPORT_NETWORK", "RECURRENCE_DOWNLOAD_HTTP:503"])(
    "retries transient rule errors: %s", async (message) => {
      vi.useFakeTimers();
      configureAutoSync(enabledSettings);
      vi.mocked(syncApi.syncNow).mockRejectedValue(new Error(message));
      const result = expect(runManualSync()).rejects.toThrow(message);
      await vi.runAllTimersAsync();
      await result;
      expect(syncApi.syncNow).toHaveBeenCalledTimes(3);
      expect(get(syncStatus).kind).toBe("offline");
    },
  );

  it.each([
    "RECURRENCE_CONFIG_CHANGED", "SYNC_TARGET_SAVE_INCOMPLETE",
    "RECURRENCE_DOWNLOAD_HTTP:403", "RECURRENCE_DOWNLOAD_HTTP:401",
    "RECURRENCE_SYNC_CONFLICT",
  ])("does not retry permanent rule errors: %s", async (message) => {
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.syncNow).mockRejectedValue(new Error(message));
    await expect(runManualSync()).rejects.toThrow(message);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    if (message === "RECURRENCE_SYNC_CONFLICT") {
      expect(get(syncStatus).kind).toBe("conflict");
    }
  });

  it("reports conflicts without network retries", async () => {
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.syncNow).mockRejectedValue(
      new Error("远端文件持续发生变化，已停止上传并保留本地数据"),
    );

    await expect(runManualSync()).rejects.toThrow("远端文件持续发生变化");
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    expect(get(syncStatus).kind).toBe("conflict");
  });

  it("keeps non-blocking remote cleanup warnings visible", async () => {
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.syncNow).mockResolvedValue({
      message: "任务、便签和附件同步完成；远端附件清理未完成：没有对象删除权限",
      todoCount: 1,
      noteCount: 1,
      noteAttachmentCount: 1,
      pendingAttachmentCount: 0,
      conflictRetried: false,
      todoRemoteEtag: '"etag"',
      noteRemoteEtag: '"note-etag"',
      noteAttachmentRemoteEtag: '"attachment-etag"',
    });

    await runManualSync();

    expect(get(syncStatus).kind).toBe("synced");
    expect(get(syncStatus).message).toContain("远端附件清理未完成");
  });

  it("checks ETag on focus and every minute without downloading unchanged data", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({
      recurrenceToken: 'missing',
      todoObjectExists: true,
      todoEtag: "\"etag-remote\"",
      noteObjectExists: true,
      noteEtag: "\"note-etag-remote\"",
      noteAttachmentObjectExists: true,
      noteAttachmentEtag: "\"attachment-etag-remote\"",
    });
    vi.mocked(syncApi.syncNow).mockResolvedValue({
      message: "同步完成",
      todoCount: 1,
      noteCount: 1,
      noteAttachmentCount: 1,
      pendingAttachmentCount: 0,
      conflictRetried: false,
      todoRemoteEtag: "\"etag-remote\"",
      noteRemoteEtag: "\"note-etag-remote\"",
      noteAttachmentRemoteEtag: "\"attachment-etag-remote\"",
    });

    setAutoSyncForeground(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(syncApi.getRemoteSyncState).toHaveBeenCalledTimes(1);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.getRemoteSyncState).toHaveBeenCalledTimes(2);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
  });

  it("retries a transient ETag check before reporting failure", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState)
      .mockRejectedValueOnce(
        new Error("检查远端同步文件失败，S3 服务返回状态码 503"),
      )
      .mockResolvedValueOnce({
        todoObjectExists: true,
        recurrenceToken: 'missing',
        todoEtag: '"etag-remote"',
        noteObjectExists: true,
        noteEtag: '"note-etag-remote"',
        noteAttachmentObjectExists: true,
        noteAttachmentEtag: '"attachment-etag-remote"',
      });
    vi.mocked(syncApi.syncNow).mockResolvedValue({
      message: "同步完成",
      todoCount: 1,
      noteCount: 1,
      noteAttachmentCount: 1,
      pendingAttachmentCount: 0,
      conflictRetried: false,
      todoRemoteEtag: '"etag-remote"',
      noteRemoteEtag: '"note-etag-remote"',
      noteAttachmentRemoteEtag: '"attachment-etag-remote"',
    });

    setAutoSyncForeground(true);
    await vi.advanceTimersByTimeAsync(1_500);

    expect(syncApi.getRemoteSyncState).toHaveBeenCalledTimes(2);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    expect(get(syncStatus).kind).toBe("synced");
  });
});

const remoteProbe = (recurrenceToken = 'etag:"rules-1"'): syncApi.RemoteSyncState => ({
  recurrenceToken, todoObjectExists: true, todoEtag: '"t"',
  noteObjectExists: true, noteEtag: '"n"', noteAttachmentObjectExists: true, noteAttachmentEtag: '"a"',
});
const syncResult = (): syncApi.ManualSyncResult => ({
  message: "同步完成", todoCount: 1, noteCount: 1, noteAttachmentCount: 0, pendingAttachmentCount: 0,
  conflictRetried: false, todoRemoteEtag: '"t"', noteRemoteEtag: '"n"', noteAttachmentRemoteEtag: '"a"',
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

describe("independent rule polling", () => {
  it("detects template-only changes and never consumes an incomplete receipt", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), templateToken: 'missing' });
    vi.mocked(syncApi.syncNow).mockResolvedValue({ ...syncResult(), templateRemoteToken: 'own' });
    setAutoSyncForeground(true); await vi.advanceTimersByTimeAsync(0);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), templateToken: 'own' });
    await vi.advanceTimersByTimeAsync(60_000); expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), templateToken: 'peer' });
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    await vi.advanceTimersByTimeAsync(60_000); expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(60_000); expect(syncApi.syncNow).toHaveBeenCalledTimes(3);
  });
  it("detects checklist-only changes using the complete upload receipt", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), checklistToken: 'missing' });
    vi.mocked(syncApi.syncNow).mockResolvedValue({ ...syncResult(), checklistRemoteToken: 'own' });
    setAutoSyncForeground(true); await vi.advanceTimersByTimeAsync(0);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), checklistToken: 'own' });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), checklistToken: 'peer' });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });
  it("does not consume checklist HEAD after incomplete upload acknowledgement", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), checklistToken: 'peer' });
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    setAutoSyncForeground(true); await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });
  it("uses link PUT receipt and detects link-only peer changes", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), linkToken: 'missing' });
    vi.mocked(syncApi.syncNow).mockResolvedValue({ ...syncResult(), linkRemoteToken: 'etag:"own-link"' });
    setAutoSyncForeground(true); await vi.advanceTimersByTimeAsync(0);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), linkToken: 'etag:"own-link"' });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), linkToken: 'etag:"peer-link"' });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });
  it("does not acknowledge a link HEAD when the upload receipt is stale", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), linkToken: 'etag:"link"' });
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    setAutoSyncForeground(true); await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });
  it("retries link conflict without consuming its remote observation", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue({ ...remoteProbe(), linkToken: 'missing' });
    vi.mocked(syncApi.syncNow).mockRejectedValueOnce(new Error('TASK_NOTE_LINK_SYNC_CONFLICT'))
      .mockResolvedValue({ ...syncResult(), linkRemoteToken: 'missing' });
    setAutoSyncForeground(true); await vi.advanceTimersByTimeAsync(0);
    expect(get(syncStatus).kind).toBe('conflict');
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });
  it("uses the PUT receipt and detects a later peer write", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remoteProbe('etag:"before"'));
    vi.mocked(syncApi.syncNow).mockResolvedValue({ ...syncResult(), recurrenceRemoteToken: 'etag:"own-put"' });
    setAutoSyncForeground(true); await vi.advanceTimersByTimeAsync(0);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remoteProbe('etag:"own-put"'));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remoteProbe('etag:"peer-put"'));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });
  it.each(['etag:"rules-2"', 'missing', 'denied'])("syncs a rule-only change: %s", async token => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remoteProbe());
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    setAutoSyncForeground(true);
    await vi.advanceTimersByTimeAsync(0);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remoteProbe(token));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(2);
  });

  it("does not consume changed rule state when synchronization fails", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remoteProbe());
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    setAutoSyncForeground(true); await vi.advanceTimersByTimeAsync(0);
    vi.mocked(syncApi.getRemoteSyncState).mockResolvedValue(remoteProbe('etag:"rules-2"'));
    vi.mocked(syncApi.syncNow).mockRejectedValueOnce(new Error("RECURRENCE_SYNC_CONFLICT"));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(get(syncStatus).kind).toBe("conflict");
    await vi.advanceTimersByTimeAsync(60_000);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(3);
    expect(get(syncStatus).kind).toBe("synced");
  });

  it.each([false, true])("discards old configuration probe, rejection=%s", async reject => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    const old = deferred<syncApi.RemoteSyncState>();
    vi.mocked(syncApi.getRemoteSyncState).mockReturnValueOnce(old.promise).mockResolvedValue(remoteProbe());
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    setAutoSyncForeground(true);
    configureAutoSync({ ...enabledSettings, bucket: "new-target" });
    if (reject) old.reject(new Error("RECURRENCE_HEAD_HTTP:503")); else old.resolve(remoteProbe('etag:"old"'));
    await vi.advanceTimersByTimeAsync(0);
    expect(syncApi.getRemoteSyncState).toHaveBeenCalledTimes(2);
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
    expect(get(syncStatus).kind).toBe("synced");
  });

  it("discards a late probe after disabling sync", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    const old = deferred<syncApi.RemoteSyncState>();
    vi.mocked(syncApi.getRemoteSyncState).mockReturnValueOnce(old.promise);
    setAutoSyncForeground(true);
    configureAutoSync({ ...enabledSettings, enabled: false });
    old.resolve(remoteProbe()); await vi.advanceTimersByTimeAsync(0);
    expect(syncApi.syncNow).not.toHaveBeenCalled();
    expect(get(syncStatus).kind).toBe("idle");
  });

  it("does not let an older probe overwrite a completed manual sync", async () => {
    vi.useFakeTimers(); configureAutoSync(enabledSettings);
    const old = deferred<syncApi.RemoteSyncState>();
    vi.mocked(syncApi.getRemoteSyncState).mockReturnValueOnce(old.promise);
    vi.mocked(syncApi.syncNow).mockResolvedValue(syncResult());
    setAutoSyncForeground(true); await runManualSync();
    old.reject(new Error("RECURRENCE_HEAD_HTTP:503")); await vi.advanceTimersByTimeAsync(0);
    expect(get(syncStatus).kind).toBe("synced");
    expect(syncApi.syncNow).toHaveBeenCalledTimes(1);
  });

  it("does not publish a late sync result after configuration changes", async () => {
    configureAutoSync(enabledSettings);
    const old = deferred<syncApi.ManualSyncResult>();
    vi.mocked(syncApi.syncNow).mockReturnValueOnce(old.promise);
    const result = expect(runManualSync()).rejects.toThrow("RECURRENCE_CONFIG_CHANGED");
    configureAutoSync({ ...enabledSettings, enabled: false });
    old.resolve(syncResult()); await result;
    expect(get(syncStatus).kind).toBe("idle");
  });
});
