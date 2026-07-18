import { invoke } from "@tauri-apps/api/core";
import { codedInvoke } from "$lib/i18n/errors";

export interface SyncSettings {
  enabled: boolean;
  endpoint: string;
  region: string;
  bucket: string;
  objectKey: string;
  noteObjectKey: string;
  noteAttachmentObjectKey: string;
  noteAssetPrefix: string;
  pathStyle: boolean;
  allowHttp: boolean;
  credentialsConfigured: boolean;
}

export interface SaveSyncSettings extends Omit<
  SyncSettings,
  | "credentialsConfigured"
  | "noteObjectKey"
  | "noteAttachmentObjectKey"
  | "noteAssetPrefix"
> {
  accessKey: string | null;
  secretKey: string | null;
}

export interface ConnectionTestResult {
  message: string;
  objectExists: boolean;
}

export interface ManualSyncResult {
  message: string;
  todoCount: number;
  noteCount: number;
  noteAttachmentCount: number;
  pendingAttachmentCount: number;
  conflictRetried: boolean;
  todoRemoteEtag: string | null;
  noteRemoteEtag: string | null;
  noteAttachmentRemoteEtag: string | null;
}

export interface RemoteSyncState {
  todoObjectExists: boolean;
  todoEtag: string | null;
  noteObjectExists: boolean;
  noteEtag: string | null;
  noteAttachmentObjectExists: boolean;
  noteAttachmentEtag: string | null;
}

export function getSyncSettings(): Promise<SyncSettings> {
  return invoke("get_sync_settings");
}

export function saveSyncSettings(
  settings: SaveSyncSettings,
): Promise<SyncSettings> {
  return invoke("save_sync_settings", { settings });
}

export function deleteSyncCredentials(): Promise<void> {
  return invoke("delete_sync_credentials");
}

export function testSyncConnection(): Promise<ConnectionTestResult> {
  return codedInvoke(invoke("test_sync_connection"), "SYNC_FAILED");
}

export function syncNow(): Promise<ManualSyncResult> {
  return codedInvoke(invoke("sync_now"), "SYNC_FAILED");
}

export function getRemoteSyncState(): Promise<RemoteSyncState> {
  return codedInvoke(invoke("get_remote_sync_state"), "SYNC_FAILED");
}
