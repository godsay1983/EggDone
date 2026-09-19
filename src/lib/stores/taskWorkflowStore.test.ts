import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { createTaskWorkflowStore, createWorkflowWriter, waitingEntry, waitingRows, validReviewDate } from './taskWorkflowStore';
import type { TaskWorkflowRequest, TaskWorkflowSnapshot } from '$lib/api/taskWorkflowApi';
import type { Todo } from '$lib/types';

const date = '2026-09-19';
const snapshot = (day = date): TaskWorkflowSnapshot => ({ date: day, revision: 'a'.repeat(64), entries: [
  { task_uuid: 'one', reason: 'Keep original  ', review_date: date, review_due: true, clock: 2 },
  { task_uuid: 'two', reason: '', review_date: null, review_due: false, clock: 3 },
  { task_uuid: 'three', reason: 'Later', review_date: '2026-09-20', review_due: false, clock: 1 },
] });
const todo = (uuid = 'one', changes: Partial<Todo> = {}): Todo => ({ id: 1, uuid, title: uuid, note: null,
  completed: false, archived_at: null, deleted_at: null, pinned: false, priority: 0, group_uuid: null,
  due_date: null, due_at: null, reminder_at: null, repeat_rule: null, repeat_series_uuid: null,
  repeat_next_due_date: null, sort_order: 0, created_at: 1, updated_at: 1, completed_at: null, ...changes });
const draft: Omit<TaskWorkflowRequest, 'operation_uuid'> = { task_uuid: 'one', state: 'waiting', reason: '  Exact text  ',
  review_date: date, date, remove_from_plan: false, expected: 'a'.repeat(64), expected_plan: null };

describe('waiting projection', () => {
  const state = { snapshot: snapshot(), loading: false, error: false };
  it('shows only active parents and current local date', () => {
    expect(waitingEntry(state, todo(), date)?.reason).toBe('Keep original  ');
    for (const changes of [{ completed: true }, { archived_at: 2 }, { deleted_at: 3 }]) {
      expect(waitingEntry(state, todo('one', changes), date)).toBeUndefined();
    }
    expect(waitingEntry(state, todo(), '2026-09-20')).toBeUndefined();
  });
  it('filters due/undated and sorts dates before undated or newest first', () => {
    const todos = ['two', 'three', 'one', 'missing'].map(id => todo(id));
    const ids = (filter: 'all' | 'due' | 'undated', sort: 'date' | 'updated') => waitingRows(state, todos, filter, sort, date).map(r => r.todo.uuid);
    expect(ids('all', 'date')).toEqual(['one', 'three', 'two']);
    expect(ids('all', 'updated')).toEqual(['two', 'one', 'three']);
    expect(ids('due', 'date')).toEqual(['one']);
    expect(ids('undated', 'date')).toEqual(['two']);
    expect(waitingRows(state, todos, 'all', 'date', '2026-09-20')).toEqual([]);
  });
  it('validates exact calendar dates without UTC/local shifts', () => {
    for (const good of ['', '0001-01-01', '2024-02-29', '9999-12-31']) expect(validReviewDate(good)).toBe(true);
    for (const bad of ['0000-01-01', '2026-02-29', '2026-04-31', '2026-9-1', '2026-13-01', 'tomorrow']) expect(validReviewDate(bad)).toBe(false);
  });
});

describe('waiting reads and date rollover', () => {
  it('does not let an older read replace a newer snapshot', async () => {
    let release!: (value: TaskWorkflowSnapshot) => void;
    const api = { list: vi.fn().mockImplementationOnce(() => new Promise(r => release = r)).mockResolvedValue({ ...snapshot(), revision: 'b'.repeat(64) }), write: vi.fn() };
    const store = createTaskWorkflowStore(api, () => date);
    const first = store.refresh(); await store.refresh(); release(snapshot()); await first;
    expect(get(store).snapshot?.revision).toBe('b'.repeat(64));
  });
  it('refetches when date changes during a read', async () => {
    let day = date;
    const api = { list: vi.fn().mockImplementation(async (asked: string) => { day = '2026-09-20'; return snapshot(asked); }), write: vi.fn() };
    const store = createTaskWorkflowStore(api, () => day); await store.refresh();
    expect(get(store).snapshot?.date).toBe('2026-09-20');
    expect(api.list).toHaveBeenCalledTimes(2);
  });
  it('exposes read failure without losing the last snapshot', async () => {
    const api = { list: vi.fn().mockResolvedValueOnce(snapshot()).mockRejectedValueOnce(Error('offline')), write: vi.fn() };
    const store = createTaskWorkflowStore(api, () => date); await store.refresh(); await store.refresh();
    expect(get(store).error).toBe(true); expect(get(store).loading).toBe(false);
    expect(get(store).snapshot?.entries).toHaveLength(3);
  });
});

describe('waiting write receipts', () => {
  it('retries identical request after an unknown result and forbids editing while unresolved', async () => {
    const api = { list: vi.fn(), write: vi.fn().mockRejectedValueOnce(Error('lost response')).mockResolvedValue(snapshot()) };
    const writer = createWorkflowWriter(api, () => 'operation-one');
    await expect(writer.save(draft)).rejects.toThrow('lost response');
    expect(writer.pending?.reason).toBe('  Exact text  ');
    await expect(writer.save({ ...draft, reason: 'changed' })).rejects.toThrow('WORKFLOW_CONFLICT');
    expect(api.write).toHaveBeenCalledTimes(1);
    await writer.save();
    expect(api.write.mock.calls[0][0]).toEqual(api.write.mock.calls[1][0]);
    expect(writer.pending).toBeNull();
  });
  it('never retries a known conflict without a new confirmed action', async () => {
    const api = { list: vi.fn(), write: vi.fn().mockRejectedValue(Error('WORKFLOW_CONFLICT')) };
    const writer = createWorkflowWriter(api, () => 'operation-one');
    await expect(writer.save(draft)).rejects.toThrow('WORKFLOW_CONFLICT');
    expect(writer.pending).toBeNull();
    await expect(writer.save()).rejects.toThrow('WORKFLOW_INVALID');
    expect(api.write).toHaveBeenCalledTimes(1);
  });
  it('guards simultaneous writes and preserves the atomic removal payload', async () => {
    let release!: (value: TaskWorkflowSnapshot) => void;
    const api = { list: vi.fn(), write: vi.fn().mockImplementation(() => new Promise(r => release = r)) };
    const writer = createWorkflowWriter(api, () => 'operation-one');
    const pending = writer.save({ ...draft, remove_from_plan: true, expected_plan: 'plan-revision' });
    await expect(writer.save(draft)).rejects.toThrow('WORKFLOW_CONFLICT');
    expect(api.write.mock.calls[0][0]).toMatchObject({ remove_from_plan: true, expected_plan: 'plan-revision' });
    release(snapshot()); await pending;
    expect(writer.busy).toBe(false); expect(writer.pending).toBeNull();
  });
});
