import { noteHistoryApi, type HistoryPreview, type HistorySummary } from "$lib/api/noteHistoryApi";

export interface NoteHistoryPort {
  list(uuid: string): Promise<HistorySummary[]>;
  preview(uuid: string, id: number): Promise<HistoryPreview>;
  restore(expected: HistoryPreview): Promise<boolean>;
}
export interface HistoryRestoreResult { changed: boolean; refreshFailed: boolean }

export function createNoteHistoryStore(port: NoteHistoryPort = noteHistoryApi) {
  let busy = false;
  return {
    list: (uuid: string) => port.list(uuid),
    preview: (uuid: string, id: number) => port.preview(uuid, id),
    async restore(expected: HistoryPreview, beforeRestore: () => void,
      afterRestore: (changed: boolean) => Promise<void>): Promise<HistoryRestoreResult> {
      if (busy) throw Error("NOTE_HISTORY_BUSY");
      const snapshot = structuredClone(expected);
      busy = true;
      try {
        beforeRestore();
        const changed = await port.restore(snapshot);
        // A committed restore must never be retried just because rendering failed.
        try { await afterRestore(changed); return { changed, refreshFailed: false }; }
        catch { return { changed, refreshFailed: true }; }
      } finally { busy = false; }
    },
  };
}
