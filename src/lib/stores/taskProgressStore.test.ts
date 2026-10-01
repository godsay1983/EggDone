import { get } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import { createTaskProgressSession } from './taskProgressStore';
import type { ProgressPage, ProgressView, ProgressWrite } from '$lib/types/taskProgress';
const entry = (uuid = 'record', token = 'old', created_at = 100): ProgressView => ({ token, record: {
  uuid, task_uuid: 'task', body: 'original', created_at, created_by: 'device', updated_at: created_at, updated_by: 'device', clock: 1, deleted_at: null,
} });
const page = (entries: ProgressView[] = []): ProgressPage => ({ task_uuid: 'task', title: 'Server title', read_only: false,
  total: entries.length, entries, next_cursor: null, overwritten: false });
function deferred<T>() { let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function setup() {
  const api = { list: vi.fn(async () => page([entry()])), write: vi.fn(async (_request: ProgressWrite) => page([entry()])),
    counts: vi.fn(async () => []), dismissNotice: vi.fn(async () => {}) };
  const changed = vi.fn(); let id = 0;
  const session = createTaskProgressSession('task', api, changed, () => `uuid-${++id}`);
  return { api, changed, session };
}
describe('progress session async safety', () => {
  it('refresh updates rows but keeps input and expected edit token', async () => {
    const { api, session } = setup(); await session.refresh(); session.edit(entry()); session.setDraft('my edit');
    api.list.mockResolvedValue(page([entry('record', 'new')])); await session.refresh();
    expect(get(session).draft).toBe('my edit'); expect(get(session).editing?.token).toBe('old');
    await session.save(); expect(api.write.mock.calls[0][0].expected_record).toBe('old');
  });
  it('ignores obsolete read success and failure', async () => {
    const { api, session } = setup(); const old = deferred<ProgressPage>(); api.list.mockReturnValueOnce(old.promise);
    const reading = session.refresh(); await session.refresh(); old.resolve({ ...page(), title: 'stale' }); await reading;
    expect(get(session).page?.title).toBe('Server title');
    const stale = deferred<ProgressPage>(); api.list.mockReturnValueOnce(stale.promise);
    const failing = session.refresh(); await session.refresh(); stale.reject('offline'); await failing;
    expect(get(session).error).toBeNull();
  });
  it('queues refresh during commit without turning a committed save into failure', async () => {
    const { api, session, changed } = setup(); await session.refresh(); session.setDraft('body');
    const saving = deferred<ProgressPage>(); api.write.mockReturnValueOnce(saving.promise);
    const write = session.save(); await session.refresh(); api.list.mockRejectedValueOnce('offline'); saving.resolve(page([entry()])); await write;
    expect(get(session).draft).toBe(''); expect(get(session).pending).toBeNull(); expect(changed).toHaveBeenCalledOnce();
    expect(get(session).error).toBe('load');
  });
  it('retries identical operation/record after a lost create response, despite reload', async () => {
    const { api, session, changed } = setup(); await session.refresh(); session.setDraft('  body\n ');
    api.write.mockRejectedValueOnce('response lost'); await session.save();
    expect(get(session).draft).toBe('  body\n '); expect(changed).not.toHaveBeenCalled();
    await session.refresh(); await session.save();
    expect(api.write.mock.calls[0][0]).toEqual(api.write.mock.calls[1][0]);
    expect(api.write.mock.calls[1][0].body).toBe('body'); expect(changed).toHaveBeenCalledOnce(); expect(get(session).draft).toBe('');
  });
  it('changed ambiguous create has a new operation but the original record UUID', async () => {
    const { api, session } = setup(); await session.refresh(); session.setDraft('first');
    api.write.mockRejectedValueOnce('lost'); await session.save(); session.setDraft('changed');
    api.write.mockRejectedValueOnce('PROGRESS_CONFLICT'); await session.save();
    const [first, second] = api.write.mock.calls.map(call => call[0]);
    expect(first.record_uuid).toBe(second.record_uuid); expect(first.operation_uuid).not.toBe(second.operation_uuid);
    expect(second.expected_record).toBeNull(); expect(get(session).draft).toBe('changed');
  });
  it('changed ambiguous edit retains its stale token until explicit rebase', async () => {
    const { api, session } = setup(); await session.refresh(); session.edit(entry()); session.setDraft('first');
    api.write.mockRejectedValueOnce('lost'); await session.save(); api.list.mockResolvedValue(page([entry('record', 'new')]));
    await session.refresh(); session.setDraft('second'); api.write.mockRejectedValueOnce('PROGRESS_CONFLICT'); await session.save();
    expect(api.write.mock.calls[1][0].expected_record).toBe('old'); expect(get(session).draft).toBe('second');
    await session.rebase(); expect(get(session).draft).toBe('second'); expect(get(session).editing?.token).toBe('new');
  });
  it('failed reload keeps data/draft and blocks normal saving until a successful load', async () => {
    const { api, session } = setup(); await session.refresh(); session.setDraft('input'); api.list.mockRejectedValueOnce('offline'); await session.refresh(); await session.save();
    expect(get(session).draft).toBe('input'); expect(get(session).page?.entries).toHaveLength(1); expect(api.write).not.toHaveBeenCalled();
    await session.refresh(); await session.save(); expect(api.write).toHaveBeenCalledOnce();
  });
  it('clears input on success even when optional notification throws', async () => {
    const { api } = setup(); const session = createTaskProgressSession('task', api, () => { throw Error('refresh'); });
    await session.refresh(); session.setDraft('input'); await session.save();
    expect(get(session).pending).toBeNull(); expect(get(session).error).toBeNull(); expect(get(session).draft).toBe('');
  });
  it('deduplicates and orders keyset pages and resets pagination on refresh', async () => {
    const { api, session } = setup(); const cursor = { created_at: 100, uuid: 'z' };
    api.list.mockResolvedValueOnce({ ...page([entry('z'), entry('y')]), next_cursor: cursor, total: 3 });
    await session.refresh(); api.list.mockResolvedValueOnce(page([entry('y'), entry('a', 'token', 90)])); await session.refresh(true);
    expect(api.list).toHaveBeenLastCalledWith('task', cursor); expect(get(session).page?.entries.map(e => e.record.uuid)).toEqual(['z', 'y', 'a']);
    await session.refresh(); expect(get(session).page?.entries).toHaveLength(1);
  });
  it('server read-only pages block mutations, nonarchived completed tasks remain writable', async () => {
    const { api, session } = setup(); api.list.mockResolvedValueOnce({ ...page(), read_only: true }); await session.refresh(); session.setDraft('input'); await session.save();
    expect(api.write).not.toHaveBeenCalled(); await session.refresh(); await session.save(); expect(api.write).toHaveBeenCalledOnce();
  });
  it('delete conflict preserves draft and lost delete response keeps its original token', async () => {
    const { api, session } = setup(); await session.refresh(); session.setDraft('new draft');
    api.write.mockRejectedValueOnce('PROGRESS_CONFLICT'); await session.remove(entry());
    expect(get(session).draft).toBe('new draft'); expect(get(session).error).toBe('conflict');
    api.write.mockRejectedValueOnce('lost'); await session.remove(entry()); await session.retry();
    expect(api.write.mock.calls[1][0]).toEqual(api.write.mock.calls[2][0]); expect(get(session).draft).toBe('new draft');
  });
  it('notice dismissal is explicit and failure keeps the notice and draft', async () => {
    const { api, session } = setup(); api.list.mockResolvedValueOnce({ ...page(), overwritten: true }); await session.refresh(); session.setDraft('input');
    api.dismissNotice.mockRejectedValueOnce('offline'); await session.dismissNotice(); expect(get(session).page?.overwritten).toBe(true);
    expect(get(session).draft).toBe('input'); await session.dismissNotice(); expect(get(session).page?.overwritten).toBe(false);
  });
  it('purges cached bodies/input for unavailable parents and ignores reads after disposal', async () => {
    const { api, session } = setup(); await session.refresh(); session.edit(entry()); session.setDraft('private');
    api.list.mockRejectedValueOnce('PROGRESS_UNAVAILABLE'); await session.refresh();
    expect(get(session).draft).toBe(''); expect(get(session).page).toBeNull(); expect(get(session).editing).toBeNull();
    const delayed = deferred<ProgressPage>(); api.list.mockReturnValueOnce(delayed.promise); const read = session.refresh(); session.dispose(); delayed.resolve(page([entry()])); await read;
    expect(get(session).page).toBeNull();
  });
  it('rejects invalid input without IPC writes', async () => {
    const { api, session } = setup(); await session.refresh();
    for (const input of ['   ', '\ud800', 'a\u202eb', 'x'.repeat(1001)]) { session.setDraft(input); await session.save(); }
    expect(api.write).not.toHaveBeenCalled();
  });
  it('missing record write must not erase input when the parent still exists', async () => {
    const { api, session } = setup(); await session.refresh(); session.edit(entry()); session.setDraft('keep this');
    api.write.mockRejectedValueOnce('PROGRESS_UNAVAILABLE'); api.list.mockResolvedValueOnce(page()); await session.save();
    expect(get(session).draft).toBe('keep this'); expect(get(session).error).toBe('deleted'); expect(get(session).page).not.toBeNull();
  });
});
