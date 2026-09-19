import { invoke } from '@tauri-apps/api/core';

export interface TaskWorkflowEntry {
  task_uuid: string;
  reason: string;
  review_date: string | null;
  review_due: boolean;
  clock: number;
}
export interface TaskWorkflowSnapshot {
  date: string;
  revision: string;
  entries: TaskWorkflowEntry[];
}
export interface TaskWorkflowRequest {
  operation_uuid: string;
  task_uuid: string;
  state: 'ready' | 'waiting';
  reason: string;
  review_date: string | null;
  date: string;
  remove_from_plan: boolean;
  expected: string;
  expected_plan: string | null;
}
export const taskWorkflowApi = {
  list: (date: string) => invoke<TaskWorkflowSnapshot>('list_task_workflow', { date }),
  write: (request: TaskWorkflowRequest) => invoke<TaskWorkflowSnapshot>('write_task_workflow', { request }),
};
