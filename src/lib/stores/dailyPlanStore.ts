import { get, writable } from 'svelte/store';
import { dailyPlanApi, type DailyPlanAction, type DailyPlanRequest, type DailyPlanSnapshot } from '$lib/api/dailyPlanApi';
import { scheduleAutoSync } from '$lib/sync/autoSync';
import type { Todo } from '$lib/types';
import { localDateString } from '$lib/utils/todoDates';

export type DailyPlanError = 'load' | 'retry' | 'conflict' | 'unavailable' | 'invalid' | 'limit' | 'rollover';
export interface DailyPlanState {
  date: string;
  snapshot: DailyPlanSnapshot | null;
  loading: boolean;
  writing: boolean;
  pending: DailyPlanRequest | null;
  error: DailyPlanError | null;
}

export function dailyPlanLocked(state: DailyPlanState) {
  return !state.snapshot || state.loading || state.writing || state.pending !== null ||
    (state.error !== null && state.error !== 'conflict' && state.error !== 'rollover' && state.error !== 'unavailable');
}

export function isPlannedToday(state: DailyPlanState, todo: Todo, today = localDateString()) {
  return !todo.completed && todo.deleted_at === null && todo.archived_at === null &&
    state.date === today && state.snapshot?.date === today &&
    state.snapshot.current.some(entry => entry.task_uuid === todo.uuid &&
      entry.plan_date === today && entry.status === 'planned');
}

export function createDailyPlanStore(
  api = dailyPlanApi,
  onCommitted: () => void = scheduleAutoSync,
  now = () => new Date(),
  uuid: () => string = () => crypto.randomUUID(),
) {
  const state = writable<DailyPlanState>({
    date: localDateString(0, now()), snapshot: null, loading: false, writing: false, pending: null, error: null,
  });
  let readVersion = 0;
  let writeActive = false;
  let refreshQueued = false;

  function rollover() {
    const date = localDateString(0, now());
    if (get(state).date === date) return false;
    readVersion++;
    state.update(s => ({ ...s, date, snapshot: null, pending: null, loading: false, error: 'rollover' }));
    return true;
  }

  async function refresh() {
    rollover();
    if (writeActive) { refreshQueued = true; return; }
    const date = get(state).date;
    const version = ++readVersion;
    state.update(s => ({ ...s, loading: true }));
    try {
      const snapshot = await api.list(date);
      if (version !== readVersion) return;
      if (rollover()) { await refresh(); return; }
      if (snapshot.date !== date) throw Error('PLAN_INVALID');
      state.update(s => ({ ...s, snapshot, error: s.pending ? 'retry' :
        s.error === 'conflict' || s.error === 'rollover' || s.error === 'unavailable' ? s.error : null }));
    } catch {
      if (version === readVersion) state.update(s => ({ ...s, error: s.pending ? 'retry' : 'load' }));
    } finally {
      if (version === readVersion) state.update(s => ({ ...s, loading: false }));
    }
  }

  async function execute(request: DailyPlanRequest) {
    writeActive = true;
    readVersion++;
    state.update(s => ({ ...s, writing: true, loading: false, pending: request, error: null }));
    try {
      const snapshot = await api.write({ ...request });
      // A successful write is final even when the post-commit notification fails.
      try { onCommitted(); } catch { /* Sync status owns notification failures. */ }
      rollover();
      state.update(s => ({ ...s, pending: null }));
      if (get(state).date === request.plan_date && snapshot.date === request.plan_date) {
        state.update(s => ({ ...s, snapshot, error: null }));
      } else {
        refreshQueued = true;
      }
    } catch (error) {
      rollover();
      if (get(state).date !== request.plan_date) {
        refreshQueued = true;
      } else {
        const message = String(error);
        const known = ['CONFLICT', 'UNAVAILABLE', 'INVALID', 'LIMIT'].find(code => message.includes('PLAN_' + code));
        state.update(s => ({ ...s, pending: known ? null : request,
          error: known ? known.toLowerCase() as DailyPlanError : 'retry' }));
        if (known === 'CONFLICT' || known === 'UNAVAILABLE') refreshQueued = true;
      }
    } finally {
      writeActive = false;
      state.update(s => ({ ...s, writing: false }));
      if (refreshQueued) { refreshQueued = false; await refresh(); }
    }
  }

  return {
    subscribe: state.subscribe,
    refresh,
    async checkDate() { if (rollover()) await refresh(); },
    async act(taskUuid: string, action: DailyPlanAction) {
      if (rollover()) { await refresh(); return; }
      const current = get(state);
      if (writeActive || dailyPlanLocked(current)) return;
      await execute({ operation_uuid: uuid(), task_uuid: taskUuid, action,
        plan_date: current.date, expected: current.snapshot!.revision });
    },
    async retry() {
      if (rollover()) { await refresh(); return; }
      const current = get(state);
      if (writeActive || current.loading) return;
      if (current.pending) await execute(current.pending);
      else await refresh();
    },
    async dismiss() {
      if (writeActive) return;
      state.update(s => ({ ...s, pending: null, error: null }));
      await refresh();
    },
  };
}

export const dailyPlans = createDailyPlanStore();

// The snapshot owns membership and ordering; Todo remains the only source of task content.
export function dailyPlanRows(snapshot: DailyPlanSnapshot | null, todos: Todo[]) {
  if (!snapshot) return { planned: [], completed: [], previous: [] };
  const byUuid = new Map(todos.filter(todo => todo.deleted_at === null && todo.archived_at === null)
    .map(todo => [todo.uuid, todo]));
  const current = new Set(snapshot.current.map(entry => entry.task_uuid));
  const yesterday = localDateString(-1, new Date(snapshot.date + 'T12:00:00'));
  function rows(entries: DailyPlanSnapshot['current'], status: 'planned' | 'completed', date: string, previous = false) {
    const seen = new Set<string>();
    return [...entries].sort((a, b) => a.position - b.position ||
      (a.task_uuid < b.task_uuid ? -1 : a.task_uuid > b.task_uuid ? 1 : 0)).flatMap(entry => {
      const todo = byUuid.get(entry.task_uuid);
      if (!todo || seen.has(todo.uuid) || entry.plan_date !== date || entry.status !== status ||
        todo.completed !== (status === 'completed') || (previous && current.has(todo.uuid))) return [];
      seen.add(todo.uuid);
      return [todo];
    });
  }
  return { planned: rows(snapshot.current, 'planned', snapshot.date),
    completed: rows(snapshot.current, 'completed', snapshot.date),
    previous: rows(snapshot.previous, 'planned', yesterday, true) };
}
