import { writable } from 'svelte/store';
import { taskChecklistApi } from '$lib/api/taskChecklistApi';
import { TaskChecklistDetailSession } from '$lib/utils/taskChecklistDetailSession';
import { scheduleAutoSync } from '$lib/sync/autoSync';
import type { ChecklistEdit, ChecklistPanelSnapshot, ChecklistProgress, ChecklistSave } from '$lib/types/taskChecklist';

export function createChecklistSession(api = taskChecklistApi, changed = scheduleAutoSync, uuid = () => crypto.randomUUID()) {
  let baseline: ChecklistPanelSnapshot | null = null;
  let request: ChecklistSave | null = null;
  let fingerprint = '';
  let busy = false;
  return {
    async load(todoUuid: string) {
      baseline = null;
      const result = await api.read(todoUuid);
      baseline = structuredClone(result); request = null; fingerprint = '';
      return result;
    },
    async save(title: string, note: string, items: ChecklistEdit[]) {
      if (!baseline || baseline.read_only || busy) throw Error('CHECKLIST_PARENT_READ_ONLY');
      const next = JSON.stringify([title.trim(), note, items]);
      if (!request || fingerprint !== next) {
        request = {operation_uuid:uuid(), todo_uuid:baseline.todo_uuid, expected_updated_at:baseline.updated_at,
          expected_items:structuredClone(baseline.items),title:title.trim(),note,items:structuredClone(items)};
        fingerprint = next;
      }
      busy = true;
      try { await api.save(structuredClone(request)); } finally { busy = false; }
      // A local commit cannot be turned into a failed submission by optional sync scheduling.
      try { changed(); } catch { /* The next sync still sees the persisted dirty revision. */ }
    },
  };
}
export const checklistProgress = writable<Record<string, ChecklistProgress> | null>(null);
export function createChecklistDetailSession() {
  return new TaskChecklistDetailSession({ read: taskChecklistApi.read, save: async request => {
    const result = await taskChecklistApi.save(request);
    try { scheduleAutoSync(); } catch { /* The persisted revision stays dirty. */ }
    return result;
  } }, () => crypto.randomUUID());
}
let generation=0;
export async function refreshChecklistProgress() {
  const ticket=++generation;
  try {
    const rows=await taskChecklistApi.progress();
    if(ticket===generation) checklistProgress.set(Object.fromEntries(rows.map(row=>[row.todo_uuid,row])));
  } catch { if(ticket===generation) checklistProgress.set(null); }
}
