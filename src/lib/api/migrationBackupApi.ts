import { invoke } from '@tauri-apps/api/core';
export interface MigrationBackupReport {
  operation: string;
  files: number;
  bytes: number;
  verifiedAt: number | null;
  current: boolean;
  blockers: string[];
}
export const migrationBackupApi = (action: 'status' | 'prepare' | 'verify') =>
  invoke<MigrationBackupReport | null>('migration_local_backup', { action });
