import { get, writable } from 'svelte/store';
import { taskWorkflowApi, type TaskWorkflowEntry, type TaskWorkflowRequest, type TaskWorkflowSnapshot } from '$lib/api/taskWorkflowApi';
import { localDateString } from '$lib/utils/todoDates';
import type { Todo } from '$lib/types';

export interface TaskWorkflowState {
  snapshot: TaskWorkflowSnapshot | null;
  loading: boolean;
  error: boolean;
}
export function createTaskWorkflowStore(api = taskWorkflowApi, today = () => localDateString()) {
  const state = writable<TaskWorkflowState>({ snapshot: null, loading: false, error: false });
  let sequence = 0;
  async function latestRead(): Promise<TaskWorkflowSnapshot | null> {
    const current = get(state);
    if (!current.loading) return current.error ? null : current.snapshot;
    return new Promise(resolve => {
      const unsubscribe = state.subscribe(value => {
        if (!value.loading) {
          queueMicrotask(unsubscribe);
          resolve(value.error ? null : value.snapshot);
        }
      });
    });
  }
  async function refresh(): Promise<TaskWorkflowSnapshot | null> {
    const ticket = ++sequence;
    const date = today();
    state.update(s => ({ ...s, loading: true }));
    try {
      const snapshot = await api.list(date);
      if (ticket !== sequence) return latestRead();
      if (today() !== date) return refresh();
      if (snapshot.date !== date) throw Error('WORKFLOW_INVALID');
      state.set({ snapshot, loading: false, error: false });
      return snapshot;
    } catch {
      if (ticket !== sequence) return latestRead();
      if (ticket === sequence) state.update(s => ({ ...s, loading: false, error: true }));
      return null;
    }
  }
  return { subscribe: state.subscribe, refresh,
    async checkDate() { if (get(state).snapshot?.date !== today()) await refresh(); },
  };
}
export const taskWorkflow = createTaskWorkflowStore();

export function waitingEntry(state: TaskWorkflowState, todo: Todo, today = localDateString()): TaskWorkflowEntry | undefined {
  if (todo.completed || todo.archived_at !== null || todo.deleted_at !== null || state.snapshot?.date !== today) return;
  return state.snapshot.entries.find(entry => entry.task_uuid === todo.uuid);
}
export type WaitingFilter = 'all' | 'due' | 'undated';
export type WaitingSort = 'date' | 'updated';
export function waitingRows(state: TaskWorkflowState, todos: Todo[], filter: WaitingFilter, sort: WaitingSort,
  today = localDateString()) {
  const entries = new Map((state.snapshot?.date === today ? state.snapshot.entries : []).map(e => [e.task_uuid, e]));
  return todos.filter(todo => !todo.completed && todo.archived_at === null && todo.deleted_at === null)
    .flatMap(todo => {
      const entry = entries.get(todo.uuid);
      if (!entry || (filter === 'due' && !(entry.review_date && entry.review_date <= today)) ||
        (filter === 'undated' && entry.review_date !== null)) return [];
      return [{ todo, entry }];
    }).sort((a, b) => {
      if (sort === 'date') {
        const dateA = a.entry.review_date ?? '9999-99-99', dateB = b.entry.review_date ?? '9999-99-99';
        if (dateA !== dateB) return dateA < dateB ? -1 : 1;
      }
      return b.entry.clock - a.entry.clock || (a.todo.uuid < b.todo.uuid ? -1 : a.todo.uuid > b.todo.uuid ? 1 : 0);
    });
}

export function validReviewDate(value: string) {
  if (value === '') return true;
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value.startsWith('0000')) return false;
  const parsed = new Date(`${value}T12:00:00Z`);
  return !Number.isNaN(parsed.getTime()) && parsed.toISOString().slice(0, 10) === value;
}
export type WorkflowError = 'load' | 'retry' | 'conflict' | 'unavailable' | 'invalid' | 'limit' | 'rollover';
export function workflowError(error: unknown): WorkflowError {
  const code = ['CONFLICT', 'UNAVAILABLE', 'INVALID', 'LIMIT'].find(c => String(error).includes('WORKFLOW_' + c));
  return code ? code.toLowerCase() as WorkflowError : 'retry';
}

// A failed response may follow a committed transaction. Keep its exact request until resolved.
export function createWorkflowWriter(api = taskWorkflowApi, uuid: () => string = () => crypto.randomUUID()) {
  let pending: TaskWorkflowRequest | null = null;
  let busy = false;
  return {
    get pending() { return pending; },
    get busy() { return busy; },
    discard() { if (!busy) pending = null; },
    async save(draft?: Omit<TaskWorkflowRequest, 'operation_uuid'>) {
      if (busy) throw Error('WORKFLOW_CONFLICT');
      if (pending && draft) throw Error('WORKFLOW_CONFLICT');
      const request = pending ?? (draft ? { ...draft, operation_uuid: uuid() } : null);
      if (!request) throw Error('WORKFLOW_INVALID');
      busy = true; pending = request;
      try {
        const snapshot = await api.write({ ...request });
        pending = null;
        return snapshot;
      } catch (error) {
        if (workflowError(error) !== 'retry') pending = null;
        throw error;
      } finally { busy = false; }
    },
  };
}
