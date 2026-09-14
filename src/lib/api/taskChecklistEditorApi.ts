import { invoke } from '@tauri-apps/api/core';
import type { ChecklistEditorRequest, ChecklistEditorResult, ChecklistEditorSnapshot } from '$lib/types/taskChecklistEditor';

export const taskChecklistEditorApi = {
  read: (uuid: string) => invoke<ChecklistEditorSnapshot>('read_task_checklist_editor', { uuid }),
  save: (request: ChecklistEditorRequest) => invoke<ChecklistEditorResult>('save_task_checklist_editor', { request }),
};
