import { taskBatchApi } from '$lib/api/taskBatchApi';
import { BatchCreationSession } from '$lib/utils/batchCreationSession';
import { scheduleAutoSync } from '$lib/sync/autoSync';
import { todos } from './todoStore';
import { get } from 'svelte/store';

export function createBatchSession() { return new BatchCreationSession(taskBatchApi, () => crypto.randomUUID()); }
export async function refreshAfterBatch() {
  scheduleAutoSync();
  await todos.refresh();
  if (get(todos).error) throw new Error('BATCH_REFRESH_FAILED');
}
