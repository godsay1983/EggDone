import { describe, expect, it } from 'vitest';
import { migrationFailure } from './migrationFailure';

describe('migration failure diagnostics', () => {
  it.each([
    ['ASSET_NOT_FOUND', 'assetNotFound'], ['ASSET_DENIED', 'assetDenied'],
    ['ASSET_DOWNLOAD', 'assetDownload'], ['ASSET_INVALID', 'assetMissing'],
    ['ASSET_CREDENTIALS', 'assetConfig'], ['FILE', 'assetMissing'], ['IO', 'failed'],
  ])('maps backup %s without hiding the cause', (code, key) => {
    const raw = `MIGRATION_BACKUP_${code}`;
    expect(migrationFailure(new Error(raw))).toEqual({message: `migrationBackup.${key}`, diagnostic: raw});
  });
  it('retains only validated attachment identity', () => {
    const raw = 'MIGRATION_BACKUP_ASSET_NOT_FOUND:00000000-0000-4000-8000-000000000002:original';
    expect(migrationFailure(raw)).toEqual({message:'migrationBackup.assetNotFound',diagnostic:raw});
    for (const unsafe of ['https://private.invalid/data', 'MIGRATION_BACKUP_IO:private/path', `${raw}\nsecret`]) {
      expect(migrationFailure(unsafe)).toEqual({message:'space.failed', diagnostic:''});
    }
  });
  it.each([
    ['MIGRATION_RECOVERY_FAILED','recoveryFailed'], ['MIGRATION_CLOUD_INVALID','cloudFailed'],
    ['MIGRATION_LOCAL_NOT_SETTLED','unsettled'], ['MIGRATION_SYNC_BUSY','busy'],
    ['MIGRATION_BACKUP_CHANGED','changed'], ['MIGRATION_BACKUP_LIMIT','limit'],
  ])('keeps stage-specific errors for %s', (code, key) => {
    expect(migrationFailure(code).message).toBe(`migrationBackup.${key}`);
  });
});
