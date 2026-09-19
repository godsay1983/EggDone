import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import type { DailyPlanEntry, DailyPlanRequest, DailyPlanSnapshot } from '$lib/api/dailyPlanApi';
import type { Todo } from '$lib/types';
vi.mock('$lib/sync/autoSync', () => ({ scheduleAutoSync: vi.fn() }));
import { createDailyPlanStore, dailyPlanLocked, dailyPlanRows, isPlannedToday } from './dailyPlanStore';

const date = '2026-09-19';
const snapshot = (revision = 'r1', day = date): DailyPlanSnapshot => ({ date: day, revision, current: [], previous: [] });
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((ok, fail) => { resolve = ok; reject = fail; });
  return { promise, resolve, reject };
}
function fixture() {
  let clock = new Date(2026, 8, 19, 12);
  let operation = 0;
  const api = {
    list: vi.fn(async (day: string) => snapshot('r1', day)),
    write: vi.fn(async (_request: DailyPlanRequest) => snapshot('r2')),
  };
  const committed = vi.fn();
  const store = createDailyPlanStore(api, committed, () => clock, () => `operation-${++operation}`);
  return { api, store, committed, setDate: (next: Date) => { clock = next; } };
}

describe('daily planning actions', () => {
  it('cannot write without a snapshot; actions use the current revision and fresh UUIDs', async () => {
    const { api, store, committed } = fixture();
    await store.act('task', 'add');
    expect(api.write).not.toHaveBeenCalled();
    await store.refresh();
    for (const action of ['add', 'up', 'down', 'remove'] as const) await store.act('task', action);
    expect(api.write.mock.calls.map(([request]) => request.action)).toEqual(['add', 'up', 'down', 'remove']);
    expect(api.write.mock.calls[0][0]).toEqual({ task_uuid: 'task', plan_date: date, action: 'add',
      expected: 'r1', operation_uuid: 'operation-1' });
    expect(api.write.mock.calls[1][0].expected).toBe('r2');
    expect(new Set(api.write.mock.calls.map(([r]) => r.operation_uuid)).size).toBe(4);
    expect(committed).toHaveBeenCalledTimes(4);
  });

  it('retains the entire original request for genuine retry, even after refresh', async () => {
    const { api, store } = fixture();
    await store.refresh();
    api.write.mockRejectedValueOnce(Error('lost reply'));
    await store.act('task', 'down');
    expect(get(store).error).toBe('retry');
    expect(dailyPlanLocked(get(store))).toBe(true);
    api.list.mockResolvedValue(snapshot('changed'));
    await store.refresh();
    await store.act('different', 'add');
    expect(api.write).toHaveBeenCalledTimes(1);
    await store.retry();
    expect(api.write.mock.calls[0]).toEqual(api.write.mock.calls[1]);
    expect(get(store).pending).toBeNull();
  });

  it('reloads conflicts without replay; only a new explicit action uses the new revision', async () => {
    const { api, store } = fixture();
    await store.refresh();
    api.write.mockRejectedValueOnce(Error('PLAN_CONFLICT'));
    api.list.mockResolvedValue(snapshot('new'));
    await store.act('task', 'up');
    expect(get(store).error).toBe('conflict');
    expect(get(store).pending).toBeNull();
    expect(get(store).snapshot?.revision).toBe('new');
    expect(api.write).toHaveBeenCalledTimes(1);
    await store.retry();
    expect(api.write).toHaveBeenCalledTimes(1);
    await store.act('task', 'up');
    expect(api.write.mock.calls[1][0]).toMatchObject({ expected: 'new', operation_uuid: 'operation-2' });
  });

  it('offers a read retry when conflict reload fails, never repeats the rejected write', async () => {
    const { api, store } = fixture();
    await store.refresh();
    api.write.mockRejectedValueOnce(Error('PLAN_CONFLICT'));
    api.list.mockRejectedValueOnce(Error('read failed'));
    await store.act('task', 'up');
    expect(get(store).error).toBe('load');
    expect(get(store).pending).toBeNull();
    await store.retry();
    expect(api.write).toHaveBeenCalledTimes(1);
  });

  it('reloads unavailable tasks and retains the explanation without blocking other tasks', async () => {
    const { api, store } = fixture();
    await store.refresh();
    api.write.mockRejectedValueOnce(Error('PLAN_UNAVAILABLE'));
    await store.act('task', 'add');
    expect(get(store)).toMatchObject({ pending: null, error: 'unavailable' });
    expect(dailyPlanLocked(get(store))).toBe(false);
    await store.dismiss();
    expect(get(store).error).toBeNull();
    expect(api.write).toHaveBeenCalledTimes(1);
  });

  it.each(['PLAN_INVALID', 'PLAN_LIMIT'])('requires refresh rather than replay for %s', async error => {
    const { api, store } = fixture();
    await store.refresh();
    api.write.mockRejectedValueOnce(Error(error));
    await store.act('task', 'add');
    expect(get(store).pending).toBeNull();
    expect(dailyPlanLocked(get(store))).toBe(true);
    await store.retry();
    expect(api.write).toHaveBeenCalledTimes(1);
    expect(dailyPlanLocked(get(store))).toBe(false);
  });

  it('serializes writes and queues refreshes behind an in-flight write', async () => {
    const { api, store } = fixture();
    await store.refresh();
    const write = deferred<DailyPlanSnapshot>();
    api.write.mockReturnValueOnce(write.promise);
    const action = store.act('task', 'down');
    await store.act('other', 'add');
    await store.refresh();
    expect(api.write).toHaveBeenCalledTimes(1);
    expect(api.list).toHaveBeenCalledTimes(1);
    api.list.mockResolvedValue(snapshot('after-sync'));
    write.resolve(snapshot('write'));
    await action;
    expect(get(store).snapshot?.revision).toBe('after-sync');
  });

  it('does not turn a post-commit refresh or sync-notification failure into a retryable write', async () => {
    const { api, store, committed } = fixture();
    await store.refresh();
    const write = deferred<DailyPlanSnapshot>();
    api.write.mockReturnValueOnce(write.promise);
    committed.mockImplementationOnce(() => { throw Error('notification'); });
    const action = store.act('task', 'down');
    await store.refresh();
    api.list.mockRejectedValueOnce(Error('refresh'));
    write.resolve(snapshot('saved'));
    await action;
    expect(get(store)).toMatchObject({ pending: null, error: 'load', snapshot: { revision: 'saved' } });
    await store.retry();
    expect(api.write).toHaveBeenCalledTimes(1);
  });

  it('dismisses an uncertain action by reading, not writing', async () => {
    const { api, store } = fixture();
    await store.refresh();
    api.write.mockRejectedValueOnce(Error('PLAN_DATABASE'));
    await store.act('task', 'down');
    await store.dismiss();
    expect(api.write).toHaveBeenCalledTimes(1);
    expect(get(store)).toMatchObject({ pending: null, error: null });
  });
});

describe('daily planning dates and reads', () => {
  it('ignores older reads resolving after a newer read', async () => {
    const { api, store } = fixture();
    const old = deferred<DailyPlanSnapshot>();
    api.list.mockReturnValueOnce(old.promise);
    const read = store.refresh();
    await store.refresh();
    old.resolve(snapshot('stale'));
    await read;
    expect(get(store).snapshot?.revision).toBe('r1');
  });

  it('drops yesterday retry and requires an explicit new-day action', async () => {
    const { api, store, setDate } = fixture();
    await store.refresh();
    api.write.mockRejectedValueOnce(Error('lost reply'));
    await store.act('task', 'up');
    setDate(new Date(2026, 8, 20, 0, 1));
    await store.retry();
    expect(api.write).toHaveBeenCalledTimes(1);
    expect(get(store)).toMatchObject({ date: '2026-09-20', pending: null, error: 'rollover' });
    expect(get(store).snapshot?.date).toBe('2026-09-20');
    api.write.mockResolvedValue(snapshot('new', '2026-09-20'));
    await store.act('task', 'add');
    expect(api.write.mock.calls[1][0].plan_date).toBe('2026-09-20');
  });

  it('refreshes local date changes without migrating a previous plan', async () => {
    const { api, store, setDate } = fixture();
    await store.refresh();
    setDate(new Date(2026, 8, 20, 0));
    await store.checkDate();
    expect(api.list).toHaveBeenLastCalledWith('2026-09-20');
    expect(api.write).not.toHaveBeenCalled();
  });

  it('does not install yesterday write response after rollover', async () => {
    const { api, store, setDate } = fixture();
    await store.refresh();
    const write = deferred<DailyPlanSnapshot>();
    api.write.mockReturnValueOnce(write.promise);
    const action = store.act('task', 'add');
    setDate(new Date(2026, 8, 20, 0));
    await store.checkDate();
    write.resolve(snapshot('yesterday'));
    await action;
    expect(get(store).snapshot?.date).toBe('2026-09-20');
    expect(get(store).snapshot?.revision).not.toBe('yesterday');
    expect(api.write).toHaveBeenCalledTimes(1);
  });

  it('rejects mismatched dates and recovers failed initial reads', async () => {
    const { api, store } = fixture();
    api.list.mockResolvedValueOnce(snapshot('invalid', '2026-09-20'));
    await store.refresh();
    expect(get(store)).toMatchObject({ error: 'load', snapshot: null, loading: false });
    await store.retry();
    expect(get(store).snapshot?.date).toBe(date);
  });
});

function todo(uuid: string, overrides: Partial<Todo> = {}): Todo {
  return { id: 1, uuid, title: uuid, note: null, group_uuid: null, completed: false, pinned: false, priority: 0,
    sort_order: 0, created_at: 1, updated_at: 1, completed_at: null, deleted_at: null, archived_at: null,
    due_date: null, due_at: null, reminder_at: null, repeat_rule: null, repeat_next_due_date: null,
    repeat_series_uuid: null, ...overrides };
}
function entry(uuid: string, position: number, day = date, status: DailyPlanEntry['status'] = 'planned'): DailyPlanEntry {
  return { task_uuid: uuid, plan_date: day, status, position };
}

it('badges only active tasks in the current local day, not stale snapshots or completion receipts', async () => {
  const { api, store, setDate } = fixture();
  api.list.mockResolvedValue({ ...snapshot(), current: [entry('task', 0)] });
  await store.refresh();
  const state = get(store);
  expect(isPlannedToday(state, todo('task'), date)).toBe(true);
  expect(isPlannedToday(state, todo('other'), date)).toBe(false);
  for (const change of [{ completed: true }, { archived_at: 1 }, { deleted_at: 1 }]) {
    expect(isPlannedToday(state, todo('task', change), date)).toBe(false);
  }
  expect(isPlannedToday({ ...state, snapshot: null }, todo('task'), date)).toBe(false);
  expect(isPlannedToday(state, todo('task'), '2026-09-20')).toBe(false);
  for (const change of [{ date: '2026-09-18' }, { current: [entry('task', 0, '2026-09-18')] },
    { current: [entry('task', 0, date, 'completed')] }, { current: [] }]) {
    expect(isPlannedToday({ ...state, snapshot: { ...state.snapshot!, ...change } }, todo('task'), date)).toBe(false);
  }
  setDate(new Date(2026, 8, 20));
  api.list.mockRejectedValueOnce(Error('offline'));
  await store.checkDate();
  expect(isPlannedToday(get(store), todo('task'), '2026-09-20')).toBe(false);
});

it('joins live content, independently orders, deduplicates and excludes terminal or mismatched parents', () => {
  const todos = [todo('b', { pinned: true }), todo('a', { title: 'Updated title', sort_order: 99 }),
    todo('done', { completed: true }), todo('deleted', { deleted_at: 2 }), todo('archived', { archived_at: 2 }),
    todo('reopened'), todo('yesterday'), todo('older')];
  const plan = { ...snapshot(), current: [entry('b', 5), entry('a', 1), entry('a', 1), entry('missing', 1),
    entry('deleted', 1), entry('archived', 1), entry('done', 1, date, 'completed'), entry('reopened', 1, date, 'completed')],
    previous: [entry('yesterday', 1, '2026-09-18'), entry('a', 2, '2026-09-18'), entry('older', 3, '2026-09-17')] };
  const original = structuredClone({ todos, plan });
  const rows = dailyPlanRows(plan, todos);
  expect(rows.planned.map(t => t.uuid)).toEqual(['a', 'b']);
  expect(rows.planned[0].title).toBe('Updated title');
  expect(rows.completed.map(t => t.uuid)).toEqual(['done']);
  expect(rows.previous.map(t => t.uuid)).toEqual(['yesterday']);
  expect({ todos, plan }).toEqual(original);
});
