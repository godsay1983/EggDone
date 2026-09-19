import type { SyncSettings } from "$lib/api/syncApi";

// Display classification only; sync authorization remains in the backend.
export function isManagedSyncPath(key: string) {
  return key === 'eggdone/todos.json' ||
    /^eggdone-spaces\/v2\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\/todos\.json$/.test(key);
}

// Rebase only server-owned target fields. Unsaved credentials and unrelated edits stay in the form.
export function reconcileSyncDraft(draft: SyncSettings, previous: SyncSettings | null, saved: SyncSettings): SyncSettings {
  const sameAccount = previous !== null && previous.endpoint === saved.endpoint && previous.bucket === saved.bucket &&
    draft.endpoint === previous.endpoint && draft.bucket === previous.bucket;
  const followTarget = sameAccount && previous.objectKey !== saved.objectKey && draft.objectKey === previous.objectKey;
  return {
    ...draft,
    credentialsConfigured: saved.credentialsConfigured,
    ...(followTarget ? {
      objectKey: saved.objectKey, noteObjectKey: saved.noteObjectKey,
      noteAttachmentObjectKey: saved.noteAttachmentObjectKey, noteAssetPrefix: saved.noteAssetPrefix,
    } : {}),
  };
}
