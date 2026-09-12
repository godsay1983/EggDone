import { trashApi, type TrashItem, type TrashKind } from "$lib/api/trashApi";

export interface TrashPort {
  list(offset: number, limit: number): Promise<TrashItem[]>;
  preview(kind: TrashKind, uuid: string): Promise<TrashItem>;
  restore(expected: TrashItem): Promise<void>;
}

export function createTrashStore(port: TrashPort = trashApi) {
  let busy = false;
  return {
    list: (offset = 0) => port.list(offset, 50),
    preview: (item: TrashItem) => port.preview(item.kind, item.uuid),
    async restore(expected: TrashItem, afterCommit: () => Promise<void>): Promise<boolean> {
      if (busy) throw Error("TRASH_BUSY");
      const snapshot = structuredClone(expected);
      busy = true;
      try {
        await port.restore(snapshot);
        // A refresh failure is not a failed write and must not offer the same restore again.
        try { await afterCommit(); return false; } catch { return true; }
      } finally { busy = false; }
    },
  };
}
