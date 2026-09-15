import { invoke } from '@tauri-apps/api/core';
import type { BatchRequest, BatchResult } from '$lib/types/taskBatch';

// Keep the same request identities when retrying an uncertain response.
export const taskBatchApi = {
  create: (request: BatchRequest) => invoke<BatchResult>('create_task_batch', { request }),
  load: () => invoke<BatchRequest | null>('load_task_batch_recovery'),
  forget: (request: BatchRequest) => invoke<void>('forget_task_batch_recovery', { request }),
};
