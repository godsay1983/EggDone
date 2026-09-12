import { writable } from "svelte/store";
import { taskNoteLinkApi } from "$lib/api/taskNoteLinkApi";
import type { LinkedTodoDraft, TaskNoteLinkView } from "$lib/types/taskNoteLink";

export function createTaskNoteLinkStore(api = taskNoteLinkApi) {
  const state = writable({ uuid: "", items: [] as TaskNoteLinkView[], loading: false, failed: false });
  let generation = 0;
  let current = "";
  let saving = false;
  return {
    subscribe: state.subscribe,
    async load(uuid: string) {
      const ticket = ++generation;
      const changed = current !== uuid;
      current = uuid;
      state.update(s => ({ uuid, items: changed ? [] : s.items, loading: !!uuid, failed: false }));
      if (!uuid) return;
      try {
        const items = await api.list("note", uuid);
        if (ticket === generation) state.set({ uuid, items, loading: false, failed: false });
      } catch {
        if (ticket === generation) state.update(s => ({ ...s, loading: false, failed: true }));
      }
    },
    async create(draft: LinkedTodoDraft, saveSource: () => Promise<void>, afterCommit: () => Promise<void>) {
      if (saving) throw Error("TASK_NOTE_LINK_BUSY");
      const copy = structuredClone(draft);
      saving = true;
      try {
        await saveSource();
        const link = await api.create(copy);
        // A refresh failure must not turn a committed task into a failed submission.
        let refreshFailed = false;
        try { await afterCommit(); } catch { refreshFailed = true; }
        return { link, refreshFailed };
      } finally { saving = false; }
    },
  };
}
