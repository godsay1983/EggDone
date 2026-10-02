import { get } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import fixture from '../../../docs/fixtures/work-review-v1.json';
import type { ReviewPage, ReviewQuery, ReviewSnapshot } from '$lib/types/workReview';
import type { SearchTarget } from '$lib/api/contentSearchApi';
import { createWorkReviewStore, reviewError } from './workReviewStore';

const query = fixture.query as ReviewQuery;
const options = { dates: fixture.dates, groupName: 'All groups', locale: 'en-US' as const };
const page = (overrides: Partial<ReviewPage> = {}): ReviewPage => ({ ...structuredClone(fixture.snapshot), next_cursor: null, ...overrides });
const deferred = <T>() => { let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
function setup() {
  const api = { list: vi.fn(async (_query: ReviewQuery, _cursor: ReviewPage['next_cursor']) => page()),
    snapshot: vi.fn(async (_query: ReviewQuery): Promise<ReviewSnapshot> => structuredClone(fixture.snapshot)),
    validate: vi.fn(async (_query: ReviewQuery, _token: string) => true) };
  const clipboard = vi.fn(async (_text: string) => {});
  const resolve = vi.fn(async (uuid: string): Promise<SearchTarget> => ({ kind: 'todo', uuid, title: 'Current title',
    content: '', parent_uuid: null, parent_title: null, completed: true, archived: true }));
  const store = createWorkReviewStore(api, clipboard, resolve); store.setQuery(query);
  return { store, api, clipboard, resolve };
}

describe('review request ownership, pagination and export', () => {
  it('ignores late pages after filter change, target change, data invalidation and disposal', async () => {
    for (const kind of ['query', 'target', 'data', 'dispose']) {
      const { store, api } = setup(), held = deferred<ReviewPage>();
      api.list.mockImplementationOnce(() => held.promise);
      const first = store.refresh();
      if (kind === 'query') { store.setQuery({ ...query, keyword: 'new' }); await store.refresh(); }
      if (kind === 'target') store.setTarget('new-target', true);
      if (kind === 'data') await store.invalidate();
      if (kind === 'dispose') store.dispose();
      held.resolve(page({ rows: [{ ...fixture.snapshot.rows[0], body: 'obsolete' }] })); await first;
      expect(get(store).rows.some(row => row.body === 'obsolete')).toBe(false);
    }
  });
  it('clears failed reload results and offers a successful retry', async () => {
    const { store, api } = setup(); await store.refresh();
    api.list.mockRejectedValueOnce('REVIEW_DATABASE'); await store.invalidate();
    expect(get(store)).toMatchObject({ rows: [], entries: 0, ready: false, loading: false, error: 'database' });
    await store.refresh(); expect(get(store).rows).toHaveLength(3);
  });
  it('deduplicates appended UUIDs, preserves total counts and carries the bound cursor', async () => {
    const { store, api } = setup(), rows = fixture.snapshot.rows;
    const cursor = { created_at: rows[1].created_at, record_uuid: rows[1].record_uuid, query_key: 'q', snapshot_token: fixture.snapshot.snapshot_token };
    api.list.mockResolvedValueOnce(page({ rows: rows.slice(0, 2), next_cursor: cursor }));
    await store.refresh(); api.list.mockResolvedValueOnce(page({ rows: rows.slice(1) })); await store.loadMore();
    expect(api.list).toHaveBeenLastCalledWith(query, cursor);
    expect(get(store)).toMatchObject({ rows, entries: 3, tasks: 2, cursor: null });
  });
  it('restarts pagination when backend or token rejects a mixed snapshot', async () => {
    for (const rejected of [true, false]) {
      const { store, api } = setup();
      api.list.mockResolvedValueOnce(page({ next_cursor: { created_at: 1, record_uuid: 'x', query_key: 'q', snapshot_token: 'old' } }));
      await store.refresh();
      if (rejected) api.list.mockRejectedValueOnce('REVIEW_CHANGED'); else api.list.mockResolvedValueOnce(page({ snapshot_token: 'new' }));
      await store.loadMore(); expect(api.list).toHaveBeenLastCalledWith(query, null);
      expect(get(store)).toMatchObject({ loading: false, ready: true, cursor: null, rows: fixture.snapshot.rows });
    }
  });
  it('copies all rows from a fresh independent snapshot rather than just the loaded page', async () => {
    const { store, api, clipboard } = setup(); api.list.mockResolvedValueOnce(page({ rows: fixture.snapshot.rows.slice(0, 1) }));
    await store.refresh(); expect(await store.copy(options)).toBe(true);
    expect(api.validate).toHaveBeenCalledWith(query, fixture.snapshot.snapshot_token);
    expect(clipboard.mock.calls[0][0]).toContain('Connection status complete.');
    expect(clipboard.mock.calls[0][0]).toContain('Mapped 100%_done\\ok.');
    expect(get(store).copied).toBe(true);
  });
  it('abandons snapshots and validations invalidated by filters, data, target or close', async () => {
    for (const stage of ['snapshot', 'validate']) for (const action of ['query', 'data', 'target', 'dispose']) {
      const { store, api, clipboard } = setup(); await store.refresh();
      const held = deferred<ReviewSnapshot>(), validation = deferred<boolean>();
      if (stage === 'snapshot') api.snapshot.mockImplementationOnce(() => held.promise);
      else api.validate.mockImplementationOnce(() => validation.promise);
      const copy = store.copy(options); await Promise.resolve(); await Promise.resolve();
      if (action === 'query') store.setQuery({ ...query, keyword: 'changed' });
      if (action === 'data') await store.invalidate();
      if (action === 'target') store.setTarget('next');
      if (action === 'dispose') store.dispose();
      held.resolve(fixture.snapshot); validation.resolve(true);
      expect(await copy).toBe(false); expect(clipboard).not.toHaveBeenCalled(); expect(get(store).copied).toBe(false);
    }
  });
  it('refuses stale tokens and oversize full summaries without invoking clipboard', async () => {
    const { store, api, clipboard } = setup(); await store.refresh();
    api.validate.mockResolvedValueOnce(false); expect(await store.copy(options)).toBe(false);
    expect(get(store).error).toBe('changed'); expect(clipboard).not.toHaveBeenCalled();
    api.snapshot.mockResolvedValueOnce({ ...fixture.snapshot, rows: [{ ...fixture.snapshot.rows[0], body: 'x'.repeat(100000) }] });
    expect(await store.copy(options)).toBe(false); expect(get(store).error).toBe('limit'); expect(clipboard).not.toHaveBeenCalled();
  });
  it('never publishes an obsolete open/copy error after its recovery reload is superseded', async () => {
    for (const operation of ['open', 'copy']) {
      const { store, api } = setup(); await store.refresh();
      api.validate.mockResolvedValueOnce(false);
      const held = deferred<ReviewPage>(); api.list.mockImplementationOnce(() => held.promise);
      const running = operation === 'open' ? store.open(fixture.snapshot.rows[0]) : store.copy(options);
      await vi.waitFor(() => expect(api.list).toHaveBeenCalledTimes(2));
      store.setQuery({ ...query, keyword: 'new filter' }); await store.refresh();
      held.resolve(page()); await running;
      expect(get(store)).toMatchObject({ error: null, query: { ...query, keyword: 'new filter' } });
    }
  });
  it('retains loaded depth when overlapping invalidation reloads clear intermediate rows', async () => {
    const { store, api } = setup(), rows = Array.from({ length: 60 }, (_, i) => ({ ...fixture.snapshot.rows[0], record_uuid: String(i), created_at: 100-i }));
    const cursor = { created_at: 71, record_uuid: '29', query_key: 'q', snapshot_token: fixture.snapshot.snapshot_token };
    const first = page({ rows: rows.slice(0, 30), next_cursor: cursor, matching_entry_count: 60 });
    const second = page({ rows: rows.slice(30), matching_entry_count: 60 });
    api.list.mockResolvedValueOnce(first).mockResolvedValueOnce(second);
    await store.refresh(); await store.loadMore(); expect(get(store).rows).toHaveLength(60);
    const held = deferred<ReviewPage>(); api.list.mockImplementationOnce(() => held.promise);
    const obsolete = store.invalidate();
    api.list.mockResolvedValueOnce(first).mockResolvedValueOnce(second);
    await store.invalidate(); held.resolve(first); await obsolete;
    expect(get(store).rows).toHaveLength(60);
  });
  it('reports actual clipboard refusal and preserves the query for retry', async () => {
    const { store, clipboard } = setup(); await store.refresh(); clipboard.mockRejectedValueOnce(Error('denied'));
    expect(await store.copy(options)).toBe(false);
    expect(get(store)).toMatchObject({ query, ready: true, copying: false, copied: false, error: 'clipboard' });
    expect(await store.copy(options)).toBe(true);
  });
  it('uses the snapshot current group name rather than a stale filter label in copied text', async () => {
    const { store, api, clipboard } = setup();
    store.setQuery({ ...query, group_scope: 'group', group_uuid: 'group' }); await store.refresh();
    api.snapshot.mockResolvedValueOnce({ ...fixture.snapshot, rows: [{ ...fixture.snapshot.rows[0], group_uuid: 'group', group_name: 'Renamed group' }] });
    expect(await store.copy({ ...options, groupName: 'Old group' })).toBe(true);
    expect(clipboard.mock.calls[0][0]).toContain('Group: Renamed group');
  });
  it('only opens live targets with current archive/title state and ignores a late resolver', async () => {
    const { store, api, resolve } = setup(); await store.refresh();
    expect(await store.open(fixture.snapshot.rows[0])).toMatchObject({ archived: true, title: 'Current title' });
    const held = deferred<SearchTarget>(); resolve.mockImplementationOnce(() => held.promise);
    const opening = store.open(fixture.snapshot.rows[0]); await Promise.resolve(); await Promise.resolve();
    store.setTarget('other', true); held.resolve({ kind: 'todo', uuid: 'old', title: 'old' } as SearchTarget);
    expect(await opening).toBeNull();
    store.setTarget('other', false); await store.refresh(); api.validate.mockResolvedValueOnce(false);
    expect(await store.open(fixture.snapshot.rows[0])).toBeNull(); expect(get(store).error).toBe('changed');
  });
  it('maps fixed error categories without exposing diagnostics', () => {
    expect(reviewError(Error('REVIEW_INVALID'))).toBe('invalid');
    expect(reviewError('REVIEW_CLIPBOARD')).toBe('clipboard'); expect(reviewError('private details')).toBe('database');
  });
});
