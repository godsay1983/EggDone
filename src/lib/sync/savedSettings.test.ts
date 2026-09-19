import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { isManagedSyncPath, reconcileSyncDraft } from './savedSettings';
import { localizedErrorMessage } from '$lib/i18n/errors';
import type { SyncSettings } from '$lib/api/syncApi';

const before: SyncSettings = {
  enabled: true, endpoint: 'https://example.test', bucket: 'bucket', region: 'region',
  objectKey: 'old/todos.json', noteObjectKey: 'old/notes.json', noteAttachmentObjectKey: 'old/attachments.json',
  noteAssetPrefix: 'old/assets/', pathStyle: true, allowHttp: false, credentialsConfigured: true,
};
const saved = { ...before, objectKey: 'new/todos.json', noteObjectKey: 'new/notes.json',
  noteAttachmentObjectKey: 'new/attachments.json', noteAssetPrefix: 'new/assets/' };

describe('automatic sync target settings', () => {
  it('labels default and generated paths without mislabeling custom locations', () => {
    expect(isManagedSyncPath('eggdone/todos.json')).toBe(true);
    expect(isManagedSyncPath('eggdone-spaces/v2/d1998d01-68fb-4f95-9519-b9a0e84eac73/todos.json')).toBe(true);
    for (const key of ['custom/todos.json', '', 'eggdone-spaces/v2/custom/todos.json', 'todos.json']) {
      expect(isManagedSyncPath(key)).toBe(false);
    }
  });
  it('rebases a stale key without discarding unrelated draft edits', () => {
    const draft = { ...before, region: 'unsaved-region', pathStyle: false, enabled: false };
    expect(reconcileSyncDraft(draft, before, saved)).toEqual({ ...draft,
      objectKey: saved.objectKey, noteObjectKey: saved.noteObjectKey,
      noteAttachmentObjectKey: saved.noteAttachmentObjectKey, noteAssetPrefix: saved.noteAssetPrefix });
    expect(draft.objectKey).toBe(before.objectKey);
  });
  it('preserves drafts when there is no association or when configuring another account', () => {
    expect(reconcileSyncDraft(before, before, before)).toEqual(before);
    const draft = { ...before, bucket: 'another' };
    expect(reconcileSyncDraft(draft, before, saved)).toEqual(draft);
    const custom = { ...before, objectKey: 'custom/todos.json' };
    expect(reconcileSyncDraft(custom, before, saved).objectKey).toBe(custom.objectKey);
  });
  it('has no reachable manual migration UI and labels connection-only results', () => {
    const source = readFileSync('src/lib/components/SyncSettings.svelte', 'utf8');
    expect(source).not.toMatch(/SyncSpaceDialog|spaceVisible|spaceActivated/);
    expect(source).toContain('reconcileBeforeSave()');
    expect(source).toContain("sync.connectionOnly");
    expect(source).not.toContain("kind: 'synced'");
  });
  it('maps auto-join errors even when wrapped by codedInvoke', () => {
    const message = localizedErrorMessage('EGGDONE_ERROR::SYNC_FAILED::SYNC_AUTO_JOIN_UNSAFE');
    expect(message).not.toContain('SYNC_AUTO_JOIN_');
    expect(message.length).toBeGreaterThan(20);
  });
});
