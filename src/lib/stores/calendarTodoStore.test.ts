import { get } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import { createCalendarTodoSession } from './calendarTodoStore';
import type { CalendarTodoResult } from '$lib/api/calendarTodoApi';
import type { Todo } from '$lib/types';
const draft = { uuid: 'd30b19ec-36c6-4aad-83d3-6b63f492c6f2', title: 'Meeting follow-up', note: 'plain', group_uuid: null };
const result: CalendarTodoResult = { created: true, todo: { uuid: draft.uuid, title: 'Committed current title' } as Todo };
describe('calendar todo save recovery', () => {
  it('freezes draft and generates no additional identity across retries', async () => {
    const create = vi.fn().mockRejectedValueOnce(Error('offline')).mockResolvedValue(result);
    const resolve = vi.fn().mockResolvedValue(null), committed = vi.fn();
    const source = { ...draft };
    const session = createCalendarTodoSession(source, { create, resolve }, committed);
    source.title = 'Source later changed';
    await session.save(); expect(get(session).error).toBe('retry');
    session.edit({ title: 'User correction' }); await session.save();
    expect(create.mock.calls.map(call => call[0].uuid)).toEqual([draft.uuid, draft.uuid]);
    expect(create.mock.calls[0][0].title).toBe('Meeting follow-up'); expect(committed).toHaveBeenCalledTimes(1);
  });
  it('lost response resolves committed current data without replacing later edits', async () => {
    const create = vi.fn().mockRejectedValue(Error('reply lost'));
    const resolve = vi.fn().mockResolvedValue({ ...result, created: false });
    const committed = vi.fn(), session = createCalendarTodoSession(draft, { create, resolve }, committed);
    await session.save(); await session.save();
    expect(create).toHaveBeenCalledTimes(1); expect(get(session).result?.todo.title).toBe('Committed current title');
    expect(committed).toHaveBeenCalledTimes(1);
  });
  it('unknown outcome locks edits and resolves before permitting retry', async () => {
    const create = vi.fn().mockRejectedValue(Error('database'));
    const resolve = vi.fn().mockRejectedValueOnce(Error('database')).mockResolvedValueOnce(null);
    const session = createCalendarTodoSession(draft, { create, resolve });
    await session.save(); expect(get(session).uncertain).toBe(true);
    session.edit({ title: 'Not applied' }); expect(get(session).draft.title).toBe(draft.title);
    await session.save(); expect(create).toHaveBeenCalledTimes(1); expect(get(session).uncertain).toBe(false);
  });
  it.each(['CALENDAR_TODO_UNAVAILABLE','CALENDAR_TODO_GROUP_UNAVAILABLE','CALENDAR_TODO_INVALID'])('keeps deterministic failure separate: %s', async code => {
    const api = { create: vi.fn().mockRejectedValue(Error(code)), resolve: vi.fn() };
    const session = createCalendarTodoSession(draft, api); await session.save();
    expect(api.resolve).not.toHaveBeenCalled(); expect(get(session).uncertain).toBe(false);
    if (code === 'CALENDAR_TODO_UNAVAILABLE') { session.edit({ title: 'No resurrection' }); await session.save(); expect(api.create).toHaveBeenCalledTimes(1); }
  });
  it('disables concurrent saves and treats postcommit callback failure as success', async () => {
    let release!: (r: CalendarTodoResult) => void;
    const create = vi.fn(() => new Promise<CalendarTodoResult>(r => release = r));
    const session = createCalendarTodoSession(draft, { create, resolve: vi.fn() }, () => { throw Error('refresh failed'); });
    const first = session.save(); await session.save(); session.edit({ title: 'Blocked' });
    release(result); await first; await session.save();
    expect(create).toHaveBeenCalledTimes(1); expect(get(session).result).toBe(result);
  });
  it('does not send invalid notes and cancellation by dropping session causes no write', async () => {
    const api = { create: vi.fn(), resolve: vi.fn() };
    createCalendarTodoSession(draft, api); expect(api.create).not.toHaveBeenCalled();
    const session = createCalendarTodoSession({ ...draft, note: '😀'.repeat(501) }, api);
    await session.save(); expect(api.create).not.toHaveBeenCalled();
  });
});
