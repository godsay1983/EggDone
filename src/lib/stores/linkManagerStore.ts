import { taskNoteLinkApi } from "$lib/api/taskNoteLinkApi";
import { noteApi } from "$lib/api/noteApi";
import { todoApi } from "$lib/api/todoApi";
import type { LinkScope, TaskNoteLink, TaskNoteLinkView } from "$lib/types/taskNoteLink";

export interface LinkCandidate { uuid: string; title: string }
export interface LinkChange { todoUuid: string; noteUuid: string; active: boolean; expected: TaskNoteLink | null }
export function linkEndpoints(scope: LinkScope, source: string, target: string) {
  return scope === "todo" ? { todoUuid: source, noteUuid: target } : { todoUuid: target, noteUuid: source };
}
const candidates = async (scope: LinkScope): Promise<LinkCandidate[]> => {
  const items = scope === "todo" ? await noteApi.list() : await todoApi.list();
  return items.map(item => ({ uuid: item.uuid, title: item.title }));
};

export function createLinkManager(api = taskNoteLinkApi, readCandidates = candidates) {
  let busy = false;
  return {
    async load(scope: LinkScope, uuid: string): Promise<{ links: TaskNoteLinkView[]; candidates: LinkCandidate[] }> {
      const links = await api.list(scope, uuid);
      const choices = await readCandidates(scope);
      const linked = new Set(links.map(item => scope === "todo" ? item.link.note_uuid : item.link.todo_uuid));
      return { links, candidates: choices.filter(item => !linked.has(item.uuid)) };
    },
    async prepare(scope: LinkScope, source: string, target: string): Promise<LinkChange> {
      const pair = linkEndpoints(scope, source, target);
      return { ...pair, active: true, expected: structuredClone(await api.pair(pair.todoUuid, pair.noteUuid)) };
    },
    async change(change: LinkChange, saveSource: () => Promise<void>, afterCommit: () => Promise<void>) {
      if (busy) throw Error("TASK_NOTE_LINK_BUSY");
      const copy = structuredClone(change);
      busy = true;
      try {
        await saveSource();
        await api.change(copy.todoUuid, copy.noteUuid, copy.active, copy.expected);
        try { await afterCommit(); return false; } catch { return true; }
      } finally { busy = false; }
    },
  };
}
