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
