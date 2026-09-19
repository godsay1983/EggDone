import type { TranslationKey } from '$lib/i18n';

export function migrationFailure(error: unknown): { message: TranslationKey; diagnostic: string } {
  const raw = error instanceof Error ? error.message : String(error);
  const diagnostic = /^MIGRATION_[A-Z_]+(?::[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}:(?:original|preview\.jpg))?$/.test(raw) ? raw : '';
  const code = diagnostic.split(':')[0];
  const message: TranslationKey = code.includes('PLANNING_ACTIVE') ? 'dailyPlan.migrationBlocked' : code.includes('LOCAL_NOT_SETTLED') ? 'migrationBackup.unsettled' :
    code.includes('BUSY') ? 'migrationBackup.busy' : code.includes('LIMIT') ? 'migrationBackup.limit' :
    code.includes('ASSET_NOT_FOUND') ? 'migrationBackup.assetNotFound' :
    code.includes('ASSET_DENIED') ? 'migrationBackup.assetDenied' :
    code.includes('ASSET_CONFIG') || code.includes('ASSET_CREDENTIALS') ? 'migrationBackup.assetConfig' :
    code.includes('ASSET_DOWNLOAD') ? 'migrationBackup.assetDownload' :
    code.includes('ASSET') || code === 'MIGRATION_BACKUP_FILE' ? 'migrationBackup.assetMissing' :
    code.includes('RECOVERY') ? 'migrationBackup.recoveryFailed' :
    code.includes('CHANGED') ? 'migrationBackup.changed' :
    code.includes('CLOUD') ? 'migrationBackup.cloudFailed' :
    code.startsWith('MIGRATION_BACKUP_') ? 'migrationBackup.failed' : 'space.failed';
  return { message, diagnostic };
}
