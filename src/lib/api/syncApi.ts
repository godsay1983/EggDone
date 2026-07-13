import { invoke } from "@tauri-apps/api/core";

export interface SyncSettings {
  enabled: boolean;
  endpoint: string;
  region: string;
  bucket: string;
  objectKey: string;
  pathStyle: boolean;
  allowHttp: boolean;
  credentialsConfigured: boolean;
}

export interface SaveSyncSettings extends Omit<
  SyncSettings,
  "credentialsConfigured"
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
  conflictRetried: boolean;
  todoRemoteEtag: string | null;
  noteRemoteEtag: string | null;
}

export interface RemoteSyncState {
  todoObjectExists: boolean;
  todoEtag: string | null;
  noteObjectExists: boolean;
  noteEtag: string | null;
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
  return invoke("test_sync_connection");
}

export function syncNow(): Promise<ManualSyncResult> {
  return invoke("sync_now");
}

export function getRemoteSyncState(): Promise<RemoteSyncState> {
  return invoke("get_remote_sync_state");
}
