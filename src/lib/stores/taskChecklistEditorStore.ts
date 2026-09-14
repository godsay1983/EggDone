import { taskChecklistEditorApi } from '$lib/api/taskChecklistEditorApi';
import { TaskChecklistEditorSession } from '$lib/utils/taskChecklistEditorSession';
import { scheduleAutoSync } from '$lib/sync/autoSync';
import type { ChecklistEditorRequest } from '$lib/types/taskChecklistEditor';

export function createTaskChecklistEditorSession() {
  return new TaskChecklistEditorSession({
    read: taskChecklistEditorApi.read,
    save: async (request: ChecklistEditorRequest) => {
      const result = await taskChecklistEditorApi.save(request);
      // The committed draft is not retried when optional post-commit sync scheduling fails.
      try { scheduleAutoSync(); } catch { /* Persisted revisions remain dirty. */ }
      return result;
    },
  }, () => crypto.randomUUID());
}
