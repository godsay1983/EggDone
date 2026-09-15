import { invoke } from '@tauri-apps/api/core';
import { scheduleAutoSync } from '$lib/sync/autoSync';
import type { TemplatesDocument, TaskTemplate, TemplateWrite } from '$lib/types/taskTemplate';
import { TemplateLibrarySession } from '$lib/utils/templateLibrarySession';
export function createTemplateLibrarySession() {
  return new TemplateLibrarySession({
    list: () => invoke<TemplatesDocument>('list_task_templates'),
    save: async (request: TemplateWrite) => {
      const result = await invoke<TaskTemplate>('save_task_template', { request });
      try { scheduleAutoSync(); } catch { /* The persisted revision remains dirty. */ }
      return result;
    }
  }, () => crypto.randomUUID());
}
