import { invoke } from '@tauri-apps/api/core';
import type { ProgressCount, ProgressCursor, ProgressPage, ProgressWrite } from '$lib/types/taskProgress';

export const taskProgressApi = {
  list: (taskUuid: string, cursor: ProgressCursor | null = null) =>
    invoke<ProgressPage>('list_task_progress', { taskUuid, cursor }),
  write: (request: ProgressWrite) => invoke<ProgressPage>('write_task_progress', { request }),
  counts: (taskUuids: string[]) => invoke<ProgressCount[]>('count_task_progress', { taskUuids }),
  dismissNotice: (taskUuid: string) => invoke<void>('dismiss_task_progress_notice', { taskUuid }),
};
