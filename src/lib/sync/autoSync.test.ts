import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as syncApi from "$lib/api/syncApi";
import {
  configureAutoSync,
  runManualSync,
  scheduleAutoSync,
  syncStatus,
} from "./autoSync";

vi.mock("$lib/api/syncApi", () => ({
  getSyncSettings: vi.fn(),
  syncNow: vi.fn(),
}));

const enabledSettings = {
  enabled: true,
  endpoint: "http://127.0.0.1:9000",
  region: "us-east-1",
  bucket: "eggdone",
  objectKey: "todos.json",
  pathStyle: true,
  allowHttp: true,
  credentialsConfigured: true,
};

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
  configureAutoSync({ ...enabledSettings, enabled: false });
});

describe("auto sync", () => {
  it("debounces local changes for four seconds", async () => {
    vi.useFakeTimers();
    configureAutoSync(enabledSettings);
    vi.mocked(syncApi.syncNow).mockResolvedValue({
      message: "同步完成",
      todoCount: 1,
      conflictRetried: false,
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
        conflictRetried: false,
      });

    const resultPromise = runManualSync();
    await vi.advanceTimersByTimeAsync(4_500);

    await expect(resultPromise).resolves.toMatchObject({ todoCount: 2 });
    expect(syncApi.syncNow).toHaveBeenCalledTimes(3);
    expect(get(syncStatus).kind).toBe("synced");
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
});
