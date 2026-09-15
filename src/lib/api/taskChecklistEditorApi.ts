import { invoke } from '@tauri-apps/api/core';
import type { RecurrenceSchedule } from '$lib/types/recurrence';
import type { ChecklistEditorRequest, ChecklistEditorResult, ChecklistEditorSnapshot } from '$lib/types/taskChecklistEditor';

export const taskChecklistEditorApi = {
  resolveTime: (schedule: RecurrenceSchedule, timezone: string | null) =>
    invoke<number | null>('resolve_checklist_rule_time', { schedule, timezone }),
  read: (uuid: string) => invoke<ChecklistEditorSnapshot>('read_task_checklist_editor', { uuid }),
  save: (request: ChecklistEditorRequest) => invoke<ChecklistEditorResult>('save_task_checklist_editor', { request }),
  create: (request: ChecklistEditorRequest) => invoke<ChecklistEditorResult>('create_task_checklist_editor', { request }),
};
