import { derived, writable } from "svelte/store";
import { RemotePollState } from "./remotePollState";
import { syncSummaryState } from "./syncSummary";

import {
  getRemoteSyncState,
  getSyncRuntimeState,
  getSyncSettings,
  syncNow,
  type ManualSyncResult,
  type RemoteSyncState,
  type SyncSettings,
  type SyncRuntimeSnapshot,
} from "$lib/api/syncApi";

export type SyncStatusKind =
  | "idle"
  | "pending"
  | "syncing"
  | "synced"
  | "cleanup_pending"
  | "offline"
  | "conflict"
  | "failed";

export interface SyncStatus {
  kind: SyncStatusKind;
  message: string;
  detail?: string;
  updatedAt: number | null;
  pendingAttachmentCount?: number;
}

const AUTO_SYNC_DELAY_MS = 4_000;
const RETRY_DELAYS_MS = [1_500, 3_000];
const FOREGROUND_POLL_INTERVAL_MS = 60_000;

export const syncStatus = writable<SyncStatus>({
  kind: "idle",
  message: "同步未启用",
  updatedAt: null,
});

export const syncRuntimeSnapshot = writable<SyncRuntimeSnapshot | null>(null);
export const savedSyncSettings = writable<SyncSettings | null>(null);

export async function refreshSavedSyncSettings(current: () => boolean = () => true): Promise<SyncSettings> {
  const settings = await getSyncSettings();
  if (current()) savedSyncSettings.set(settings);
  return settings;
}

let settingsUpdate: Promise<unknown> | null = null;

export function runSettingsUpdate<T>(action: () => Promise<T>): Promise<T> {
  const previous = settingsUpdate;
  const update = (async () => {
    if (previous) { try { await previous; } catch { /* A failed settings form must not block the next one. */ } }
    if (running) { try { await running; } catch { /* Read the persisted target even after failure. */ } }
    clearDebounce();
    return action();
  })();
  settingsUpdate = update;
  const release = () => {
    if (settingsUpdate === update) {
      settingsUpdate = null;
      if (pendingAfterRun && enabled) { pendingAfterRun = false; scheduleAutoSync(); }
    }
  };
  void update.then(release, release);
  return update;
}
const syncAvailability = writable<{ enabled: boolean; configured: boolean } | null>(null);
export const syncSummary = derived([syncStatus, syncAvailability], ([status, availability]) =>
  syncSummaryState({
    enabled: availability?.enabled ?? true,
    configured: availability?.configured ?? true,
    busy: status.kind === "syncing",
    kind: status.kind,
    dirty: status.kind === "pending",
    pendingUploads: status.pendingAttachmentCount ?? 0,
    failedUploads: 0,
  }),
);

let enabled = false;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let running: Promise<ManualSyncResult> | null = null;
let pendingAfterRun = false;
let initialized = false;
let foreground = false;
let foregroundVersion = 0;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let remoteCheckRunning = false;
let remoteStateInitialized = false;
let knownTodoRemoteEtag: string | null = null;
let knownNoteRemoteEtag: string | null = null;
let knownNoteAttachmentRemoteEtag: string | null = null;
const pollState = new RemotePollState();

export async function initializeAutoSync() {
  if (initialized) return;
  initialized = true;
  try {
    const [settings, snapshot] = await Promise.all([
      getSyncSettings(),
      getSyncRuntimeState(),
    ]);
    configureAutoSync(settings);
    applyRuntimeSnapshot(snapshot, settings.enabled && settings.credentialsConfigured);
  } catch (reason) {
    setFailureStatus(reason);
  }
}

export function configureAutoSync(settings: SyncSettings) {
  savedSyncSettings.set(settings);
  pollState.reset();
  clearDebounce();
  enabled = settings.enabled && settings.credentialsConfigured;
  syncAvailability.set({ enabled: settings.enabled, configured: settings.credentialsConfigured });
  remoteStateInitialized = false;
  knownTodoRemoteEtag = null;
  knownNoteRemoteEtag = null;
  knownNoteAttachmentRemoteEtag = null;
  if (!enabled) {
    clearDebounce();
    stopForegroundPolling();
    pendingAfterRun = false;
    syncStatus.set({
      kind: "idle",
      message: settings.enabled ? "同步凭据未配置" : "同步未启用",
      updatedAt: null,
    });
  } else {
    syncStatus.set({ kind: "pending", message: "有修改未同步", updatedAt: Date.now() });
    if (foreground) {
      startForegroundPolling();
      void checkRemoteAndSync();
    }
  }
}

export function setAutoSyncForeground(value: boolean) {
  if (foreground !== value) {
    foregroundVersion += 1;
    pollState.invalidateProbe();
  }
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

export function syncOnPanelShown() {
  // Native tray activation is explicit; it must not depend on a WebView focus event
  // or wait for a full remote HEAD sweep before starting synchronization.
  if (!foreground) foregroundVersion += 1;
  foreground = true;
  if (!enabled) return;
  startForegroundPolling();
  void runAutomaticSync();
}

export function scheduleAutoSync() {
  syncStatus.update((current) =>
    current.kind === "syncing"
      ? current
      : {
          kind: "pending",
          message: "有修改未同步",
          updatedAt: Date.now(),
        },
  );
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

export async function refreshSyncRuntimeState(): Promise<SyncRuntimeSnapshot> {
  const snapshot = await getSyncRuntimeState();
  syncRuntimeSnapshot.set(snapshot);
  return snapshot;
}

function applyRuntimeSnapshot(snapshot: SyncRuntimeSnapshot, syncEnabled: boolean) {
  syncRuntimeSnapshot.set(snapshot);
  if (!syncEnabled) return;
  if (snapshot.dirtyDomains.length > 0 && !["offline", "conflict", "failed"].includes(snapshot.lastResult)) {
    syncStatus.set({
      kind: "pending",
      message: "有修改未同步",
      updatedAt: snapshot.dirtySince,
      pendingAttachmentCount: snapshot.pendingAttachmentCount,
    });
    return;
  }
  if (snapshot.lastResult === "success") {
    syncStatus.set({
      kind: snapshot.lastErrorCode?.startsWith("SYNC_CLEANUP_") ? "cleanup_pending" : "synced",
      message: snapshot.lastErrorMessage ?? "同步完成",
      updatedAt: snapshot.lastSuccessAt,
      pendingAttachmentCount: snapshot.pendingAttachmentCount,
    });
  } else if (["offline", "conflict", "failed"].includes(snapshot.lastResult)) {
    const kind = snapshot.lastResult as "offline" | "conflict" | "failed";
    syncStatus.set({
      kind,
      message: failureMessage(kind),
      detail: snapshot.lastErrorMessage ?? undefined,
      updatedAt: snapshot.updatedAt,
    });
  } else if (snapshot.lastResult === "interrupted") {
    syncStatus.set({
      kind: "pending",
      message: "上次同步被中断，等待重试",
      updatedAt: snapshot.lastAttemptAt,
    });
  }
}

export async function runManualSync(): Promise<ManualSyncResult> {
  clearDebounce();
  if (!running) pendingAfterRun = false;
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
  if (settingsUpdate) return settingsUpdate.then(() => runSyncWithRetry());
  clearDebounce();
  if (running) return running;

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
  const generation = pollState.beginSync();
  let lastError: unknown;
  for (let attempt = 0; attempt <= RETRY_DELAYS_MS.length; attempt += 1) {
    if (!pollState.isGenerationCurrent(generation)) throw new Error("RECURRENCE_CONFIG_CHANGED");
    syncStatus.set({
      kind: "syncing",
      message:
        attempt === 0 ? "正在同步…" : `网络异常，正在第 ${attempt} 次重试…`,
      updatedAt: null,
    });
    try {
      const result = await syncNow();
      if (!pollState.isGenerationCurrent(generation)) throw new Error("RECURRENCE_CONFIG_CHANGED");
      // A backend auto-join belongs to this session: publishing settings must not reset its generation.
      await refreshSavedSyncSettings(() => pollState.isGenerationCurrent(generation));
      if (!pollState.isGenerationCurrent(generation)) throw new Error("RECURRENCE_CONFIG_CHANGED");
      if (result.recurrenceRemoteToken !== undefined) pollState.acknowledgeRules(generation, result.recurrenceRemoteToken);
      if (result.linkRemoteToken !== undefined) pollState.acknowledgeLinks(generation, result.linkRemoteToken);
      if (result.checklistRemoteToken !== undefined) pollState.acknowledgeChecklists(generation, result.checklistRemoteToken);
      if (result.templateRemoteToken !== undefined) pollState.acknowledgeTemplates(generation, result.templateRemoteToken);
      if (result.planRemoteToken !== undefined) pollState.acknowledgePlans(generation, result.planRemoteToken);
      if (result.workflowRemoteToken !== undefined) pollState.acknowledgeWorkflow(generation, result.workflowRemoteToken);
      const cleanupNotice = result.message.includes("远端附件");
      let snapshot: SyncRuntimeSnapshot | null = null;
      try {
        snapshot = await refreshSyncRuntimeState();
      } catch {
        // Use the sync receipt if refreshing diagnostics fails.
      }
      if (!pollState.isGenerationCurrent(generation)) throw new Error("RECURRENCE_CONFIG_CHANGED");
      const dirty = pendingAfterRun || (snapshot?.dirtyDomains.length ?? 0) > 0;
      syncStatus.set({
        kind: dirty ? "pending" : result.cleanupWarning ? "cleanup_pending" : "synced",
        message: cleanupNotice
          ? result.message
          : result.conflictRetried
            ? `冲突已合并：任务 ${result.todoCount}，便签 ${result.noteCount}，附件 ${result.noteAttachmentCount}`
            : `同步完成：任务 ${result.todoCount}，便签 ${result.noteCount}，附件 ${result.noteAttachmentCount}`,
        updatedAt: Date.now(),
        pendingAttachmentCount: snapshot?.pendingAttachmentCount ?? result.pendingAttachmentCount,
      });
      knownTodoRemoteEtag = result.todoRemoteEtag;
      knownNoteRemoteEtag = result.noteRemoteEtag;
      knownNoteAttachmentRemoteEtag = result.noteAttachmentRemoteEtag;
      remoteStateInitialized = true;
      return result;
    } catch (reason) {
      if (!pollState.isGenerationCurrent(generation)) throw reason;
      try { await refreshSavedSyncSettings(() => pollState.isGenerationCurrent(generation)); } catch { /* Keep the original sync error. */ }
      if (!pollState.isGenerationCurrent(generation)) throw reason;
      remoteStateInitialized = false;
      lastError = reason;
      if (!isRetryable(reason) || attempt === RETRY_DELAYS_MS.length) {
        setFailureStatus(reason);
        try {
          await refreshSyncRuntimeState();
        } catch {
          // Keep the in-memory failure status when diagnostics are unavailable.
        }
        throw reason;
      }
      await delay(RETRY_DELAYS_MS[attempt]);
    }
  }
  throw lastError;
}

async function checkRemoteAndSync() {
  if (!enabled || !foreground || remoteCheckRunning || settingsUpdate) return;
  if (running) return;

  remoteCheckRunning = true;
  const ticket = pollState.capture();
  const visibilityVersion = foregroundVersion;
  let startedSync = false;
  try {
    const remote = await getRemoteStateWithRetry(() => pollState.isCurrent(ticket) && enabled && foreground);
    if (!pollState.isCurrent(ticket) || !enabled || !foreground || running || settingsUpdate) return;
    const changed =
      remote.targetChanged === true ||
      pollState.rulesChanged(remote.recurrenceToken) ||
      pollState.linksChanged(remote.linkToken) ||
      pollState.checklistsChanged(remote.checklistToken) ||
      pollState.templatesChanged(remote.templateToken) ||
      pollState.plansChanged(remote.planToken) ||
      pollState.workflowChanged(remote.workflowToken) ||
      !remoteStateInitialized ||
      remote.todoObjectExists !== (knownTodoRemoteEtag !== null) ||
      remote.todoEtag !== knownTodoRemoteEtag ||
      remote.noteObjectExists !== (knownNoteRemoteEtag !== null) ||
      remote.noteEtag !== knownNoteRemoteEtag ||
      remote.noteAttachmentObjectExists !== (knownNoteAttachmentRemoteEtag !== null) ||
      remote.noteAttachmentEtag !== knownNoteAttachmentRemoteEtag;
    if (changed) {
      startedSync = true;
      // Do not consume the observation on failure, or the next unchanged HEAD would skip retry.
      const result = await runSyncWithRetry();
      if (!remote.targetChanged && !result.targetChanged) {
        pollState.acknowledgeRules(ticket.generation, result.recurrenceRemoteToken ?? remote.recurrenceToken);
      }
    }
  } catch (reason) {
    if ((pollState.isCurrent(ticket) || (startedSync && pollState.isGenerationCurrent(ticket.generation))) &&
      enabled && foreground) setFailureStatus(reason);
  } finally {
    remoteCheckRunning = false;
    if ((!pollState.isGenerationCurrent(ticket.generation) ||
      (!startedSync && visibilityVersion !== foregroundVersion)) && enabled && foreground && !running) {
      void checkRemoteAndSync();
    }
  }
}

async function getRemoteStateWithRetry(current: () => boolean): Promise<RemoteSyncState> {
  let lastError: unknown;
  for (let attempt = 0; attempt <= RETRY_DELAYS_MS.length; attempt += 1) {
    if (!current()) throw new Error("RECURRENCE_CONFIG_CHANGED");
    try {
      return await getRemoteSyncState();
    } catch (reason) {
      if (!current()) throw reason;
      lastError = reason;
      if (!isRetryable(reason) || attempt === RETRY_DELAYS_MS.length) {
        throw reason;
      }
      await delay(RETRY_DELAYS_MS[attempt]);
    }
  }
  throw lastError;
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
  const detail = errorMessage(reason);
  const kind = isConflict(detail)
    ? "conflict"
    : isRetryable(detail)
      ? "offline"
      : "failed";
  syncStatus.set({
    kind,
    message: failureMessage(kind),
    detail,
    updatedAt: Date.now(),
  });
}

function failureMessage(kind: Exclude<SyncStatusKind, "idle" | "syncing" | "synced">) {
  if (kind === "offline") return "网络暂时不可用，将在下次同步时重试";
  if (kind === "conflict") return "远端内容持续变化，请稍后再次同步";
  return "同步未完成，请重试";
}

function isRetryable(reason: unknown) {
  const message = errorMessage(reason).toLowerCase();
  if (
    message.includes("凭据") ||
    message.includes("权限") ||
    message.includes("配置") ||
    message.includes("recurrence_config_changed") ||
    message.includes("sync_target_save_incomplete") ||
    message.includes("sync_auto_join_") ||
    isConflict(message)
  ) {
    return false;
  }
  const statusCode = message.match(/(?:状态码\s*|_http:)(\d{3})/);
  if (statusCode) {
    const code = Number(statusCode[1]);
    return code === 408 || code === 425 || code === 429 || (code >= 500 && code <= 599);
  }
  return [
    "连接",
    "网络",
    "超时",
    "timeout",
    "offline",
    "network",
    "connection",
    "dns",
    "request",
    "temporarily unavailable",
    "connection reset",
    "broken pipe",
    "下载同步文件失败",
    "上传同步文件失败",
    "检查远端同步文件失败",
    "下载便签同步文件失败",
    "上传便签同步文件失败",
    "下载附件元数据失败",
    "上传附件元数据失败",
    "检查远端附件失败",
    "上传附件失败",
    "下载附件失败",
  ].some((keyword) => message.includes(keyword));
}

function isConflict(message: string) {
  return message.includes("远端文件持续发生变化") || message.toLowerCase().includes("recurrence_sync_conflict") || message.toLowerCase().includes("task_note_link_sync_conflict");
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
