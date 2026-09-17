import { archiveApi, type ArchivePreview, type ArchiveRequest, type ArchiveAction } from "$lib/api/archiveApi";
export type ArchivePort = typeof archiveApi;

export function createArchiveStore(port: ArchivePort = archiveApi, identity: () => string = () => crypto.randomUUID()) {
  let busy = false;
  return {
    list: port.list,
    preview: (item: ArchivePreview) => port.preview(item.expected.uuid),
    prepare(action: ArchiveAction, item: ArchivePreview): ArchiveRequest {
      return { operation: identity(), action, expected: structuredClone(item.expected) };
    },
    async apply(request: ArchiveRequest, refresh: () => Promise<void>) {
      if (busy) throw Error("ARCHIVE_BUSY");
      const snapshot = structuredClone(request);
      busy = true;
      try {
        const outcome = await port.apply(snapshot);
        let refreshNeeded = false;
        try { await refresh(); } catch { refreshNeeded = true; }
        return { outcome, refreshNeeded };
      } finally { busy = false; }
    },
  };
}
