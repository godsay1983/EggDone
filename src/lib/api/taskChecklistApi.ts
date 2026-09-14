import { invoke } from '@tauri-apps/api/core';
import type { ChecklistPanelSnapshot, ChecklistProgress, ChecklistSave } from '$lib/types/taskChecklist';
export const taskChecklistApi = {
  read: (uuid: string) => invoke<ChecklistPanelSnapshot>('read_task_checklist', { uuid }),
  progress: () => invoke<ChecklistProgress[]>('list_task_checklist_progress'),
  save: (request: ChecklistSave) => invoke<number>('save_task_checklist', { request }),
};
