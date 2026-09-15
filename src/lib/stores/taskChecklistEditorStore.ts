import { taskChecklistEditorApi } from '$lib/api/taskChecklistEditorApi';
import { TaskChecklistEditorSession } from '$lib/utils/taskChecklistEditorSession';
import { scheduleAutoSync } from '$lib/sync/autoSync';
import type { ChecklistEditorRequest, ChecklistEditorSnapshot } from '$lib/types/taskChecklistEditor';
import { taskCopyDraft } from '$lib/utils/taskCopyDraft';

export async function readTaskCopyDraft(sourceUuid: string, groupIds: string[]) {
  const source = await taskChecklistEditorApi.read(sourceUuid);
  if (source.task.todo_uuid !== sourceUuid) throw Error('CHECKLIST_PARENT_MISMATCH');
  return taskCopyDraft(source, groupIds, () => crypto.randomUUID());
}

export function createTaskChecklistEditorSession(creation: ChecklistEditorSnapshot | null = null) {
  return new TaskChecklistEditorSession({
    read: creation === null ? taskChecklistEditorApi.read : async () => structuredClone(creation),
    save: async (request: ChecklistEditorRequest) => {
      const result = await (creation === null ? taskChecklistEditorApi.save(request) : taskChecklistEditorApi.create(request));
      // The committed draft is not retried when optional post-commit sync scheduling fails.
      try { scheduleAutoSync(); } catch { /* Persisted revisions remain dirty. */ }
      return result;
    },
  }, () => crypto.randomUUID());
}
