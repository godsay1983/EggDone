import { writable } from "svelte/store";

import {
  getRemoteSyncState,
  getSyncSettings,
  syncNow,
  type ManualSyncResult,
  type SyncSettings,
} from "$lib/api/syncApi";

export type SyncStatusKind =
  | "idle"
  | "syncing"
  | "synced"
  | "offline"
  | "conflict"
  | "failed";

export interface SyncStatus {
  kind: SyncStatusKind;
  message: string;
  updatedAt: number | null;
}

const AUTO_SYNC_DELAY_MS = 4_000;
const RETRY_DELAYS_MS = [1_500, 3_000];
const FOREGROUND_POLL_INTERVAL_MS = 60_000;

export const syncStatus = writable<SyncStatus>({
  kind: "idle",
  message: "同步未启用",
  updatedAt: null,
});

let enabled = false;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let running: Promise<ManualSyncResult> | null = null;
let pendingAfterRun = false;
let initialized = false;
let foreground = false;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let remoteCheckRunning = false;
let remoteStateInitialized = false;
let knownTodoRemoteEtag: string | null = null;
let knownNoteRemoteEtag: string | null = null;

export async function initializeAutoSync() {
  if (initialized) return;
  initialized = true;
  try {
    const settings = await getSyncSettings();
    configureAutoSync(settings);
  } catch (reason) {
    setFailureStatus(reason);
  }
}

export function configureAutoSync(settings: SyncSettings) {
  enabled = settings.enabled && settings.credentialsConfigured;
  remoteStateInitialized = false;
  knownTodoRemoteEtag = null;
  knownNoteRemoteEtag = null;
  if (!enabled) {
    clearDebounce();
    stopForegroundPolling();
    pendingAfterRun = false;
    syncStatus.set({
      kind: "idle",
      message: settings.enabled ? "同步凭据未配置" : "同步未启用",
      updatedAt: null,
    });
  } else if (foreground) {
    startForegroundPolling();
    void checkRemoteAndSync();
  }
}

export function setAutoSyncForeground(value: boolean) {
  foreground = value;
  if (!foreground) {
    stopForegroundPolling();
    return;
  }
  if (enabled) {
    startForegroundPolling();
    void checkRemoteAndSync();
  }
}

export function scheduleAutoSync() {
  if (!enabled) return;
  if (running) {
    pendingAfterRun = true;
    return;
  }
  clearDebounce();
  debounceTimer = setTimeout(() => {
    debounceTimer = null;
    void runAutomaticSync();
  }, AUTO_SYNC_DELAY_MS);
}

export async function runManualSync(): Promise<ManualSyncResult> {
  clearDebounce();
  pendingAfterRun = false;
  return runSyncWithRetry();
}

async function runAutomaticSync() {
  try {
    await runSyncWithRetry();
  } catch {
    // Status is reported through syncStatus; local Todo operations remain successful.
  }
}

function runSyncWithRetry(): Promise<ManualSyncResult> {
  if (running) {
    pendingAfterRun = true;
    return running;
  }

  running = performSyncWithRetry().finally(() => {
    running = null;
    if (pendingAfterRun && enabled) {
      pendingAfterRun = false;
      scheduleAutoSync();
    }
  });
  return running;
}

async function performSyncWithRetry(): Promise<ManualSyncResult> {
  let lastError: unknown;
  for (let attempt = 0; attempt <= RETRY_DELAYS_MS.length; attempt += 1) {
    syncStatus.set({
      kind: "syncing",
      message:
        attempt === 0 ? "正在同步…" : `网络异常，正在第 ${attempt} 次重试…`,
      updatedAt: null,
    });
    try {
      const result = await syncNow();
      syncStatus.set({
        kind: "synced",
        message: result.conflictRetried
          ? `冲突已合并：任务 ${result.todoCount}，便签 ${result.noteCount}`
          : `同步完成：任务 ${result.todoCount}，便签 ${result.noteCount}`,
        updatedAt: Date.now(),
      });
      knownTodoRemoteEtag = result.todoRemoteEtag;
      knownNoteRemoteEtag = result.noteRemoteEtag;
      remoteStateInitialized = true;
      return result;
    } catch (reason) {
      lastError = reason;
      if (!isRetryable(reason) || attempt === RETRY_DELAYS_MS.length) {
        setFailureStatus(reason);
        throw reason;
      }
      await delay(RETRY_DELAYS_MS[attempt]);
    }
  }
  throw lastError;
}

async function checkRemoteAndSync() {
  if (!enabled || !foreground || remoteCheckRunning) return;
  if (running) {
    pendingAfterRun = true;
    return;
  }

  remoteCheckRunning = true;
  try {
    const remote = await getRemoteSyncState();
    const changed =
      !remoteStateInitialized ||
      remote.todoObjectExists !== (knownTodoRemoteEtag !== null) ||
      remote.todoEtag !== knownTodoRemoteEtag ||
      remote.noteObjectExists !== (knownNoteRemoteEtag !== null) ||
      remote.noteEtag !== knownNoteRemoteEtag;
    remoteStateInitialized = true;
    knownTodoRemoteEtag = remote.todoEtag;
    knownNoteRemoteEtag = remote.noteEtag;
    if (changed) {
      await runAutomaticSync();
    }
  } catch (reason) {
    setFailureStatus(reason);
  } finally {
    remoteCheckRunning = false;
  }
}

function startForegroundPolling() {
  if (pollTimer || !enabled || !foreground) return;
  pollTimer = setInterval(() => {
    void checkRemoteAndSync();
  }, FOREGROUND_POLL_INTERVAL_MS);
}

function stopForegroundPolling() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

function setFailureStatus(reason: unknown) {
  const message = errorMessage(reason);
  const kind = isConflict(message)
    ? "conflict"
    : isRetryable(message)
      ? "offline"
      : "failed";
  syncStatus.set({
    kind,
    message: `${statusLabel(kind)}：${message}`,
    updatedAt: Date.now(),
  });
}

function statusLabel(kind: Exclude<SyncStatusKind, "idle" | "syncing" | "synced">) {
  if (kind === "offline") return "离线";
  if (kind === "conflict") return "同步冲突";
  return "同步失败";
}

function isRetryable(reason: unknown) {
  const message = errorMessage(reason).toLowerCase();
  if (
    message.includes("凭据") ||
    message.includes("权限") ||
    message.includes("配置") ||
    isConflict(message)
  ) {
    return false;
  }
  return [
    "连接",
    "网络",
    "超时",
    "timeout",
    "offline",
    "connection",
    "dns",
    "下载同步文件失败",
    "上传同步文件失败",
  ].some((keyword) => message.includes(keyword));
}

function isConflict(message: string) {
  return message.includes("远端文件持续发生变化");
}

function clearDebounce() {
  if (debounceTimer) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
}

function delay(milliseconds: number) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
