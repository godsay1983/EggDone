import { invoke } from '@tauri-apps/api/core';
export interface MigrationBackupReport {
  publication?: { operation: string; digest: string; total: number; completed: number; bytes: number; confirmed: boolean; published: boolean; current: boolean } | null;
  cloud?: { operation: string; objects: number; files: number; bytes: number; verifiedAt: number | null; current: boolean } | null;
  operation: string;
  files: number;
  bytes: number;
  verifiedAt: number | null;
  current: boolean;
  blockers: string[];
}
export type MigrationBackupAction = 'status' | 'prepare' | 'verify' | 'prepareCloud' | 'preparePublication' | 'publish';
export const migrationBackupApi = (action: MigrationBackupAction, expected: string | null = null) =>
  invoke<MigrationBackupReport | null>('migration_local_backup', { action, expected });
