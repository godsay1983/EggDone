import { describe, expect, it, vi } from 'vitest';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { taskProgressApi } from './taskProgressApi';
import type { ProgressWrite } from '$lib/types/taskProgress';
describe('frozen progress IPC', () => {
  it('uses camelCase arguments and untouched snake_case requests', async () => {
    invoke.mockReset();
    const cursor = { created_at: 100, uuid: 'record' };
    const request: ProgressWrite = { operation_uuid: 'op', task_uuid: 'task', record_uuid: 'record', action: 'edit', body: 'body', expected_record: 'token' };
    await taskProgressApi.list('task', cursor); await taskProgressApi.write(request);
    await taskProgressApi.counts(['task', 'other']); await taskProgressApi.dismissNotice('task');
    expect(invoke.mock.calls).toEqual([
      ['list_task_progress', { taskUuid: 'task', cursor }], ['write_task_progress', { request }],
      ['count_task_progress', { taskUuids: ['task', 'other'] }], ['dismiss_task_progress_notice', { taskUuid: 'task' }],
    ]);
  });
  it('propagates failures, never fabricating an empty page', async () => {
    invoke.mockRejectedValueOnce('PROGRESS_DATABASE');
    await expect(taskProgressApi.list('task')).rejects.toBe('PROGRESS_DATABASE');
    expect(invoke).toHaveBeenLastCalledWith('list_task_progress', { taskUuid: 'task', cursor: null });
  });
});
