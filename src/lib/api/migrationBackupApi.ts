import { invoke } from '@tauri-apps/api/core';
export interface MigrationBackupReport {
  cloud?: { operation: string; objects: number; files: number; bytes: number; verifiedAt: number | null; current: boolean } | null;
  operation: string;
  files: number;
  bytes: number;
  verifiedAt: number | null;
  current: boolean;
  blockers: string[];
}
export const migrationBackupApi = (action: 'status' | 'prepare' | 'verify' | 'prepareCloud') =>
  invoke<MigrationBackupReport | null>('migration_local_backup', { action });
