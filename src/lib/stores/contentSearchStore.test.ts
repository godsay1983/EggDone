import { get } from "svelte/store";
import { expect, it, vi } from "vitest";
import { createContentSearchStore } from "./contentSearchStore";
import type { SearchPage, SearchScope } from "$lib/api/contentSearchApi";
const result = (scope: SearchScope, query: string, offset = 0): SearchPage => ({ scope, query, offset, limit: 20, total: 0, items: [] });
function fixture() {
  const search = vi.fn(async (scope: SearchScope, query: string, offset: number) => result(scope, query, offset));
  return { search, store: createContentSearchStore({ search }) };
}
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
it("searches every category, normalizes whitespace and keeps literal input", async () => {
  const { search, store } = fixture(); store.setQuery("\u0085 100%_计划 \uFEFF"); await store.search();
  expect(search.mock.calls).toEqual([['todo','100%_计划',0,20,true],['note','100%_计划',0,20,true],['attachment','100%_计划',0,20,true]]);
  expect(get(store).groups.map(g => g.status)).toEqual(['ready','ready','ready']);
});
it("does not enumerate on blank, rejects oversized or NUL queries before IPC", async () => {
  const { search, store } = fixture();
  for (const text of [' ', '\u0085\uFEFF', 'x'.repeat(101), 'a\0b']) { store.setQuery(text); await store.search(); }
  expect(search).not.toHaveBeenCalled(); expect(get(store).invalid).toBe(true);
  store.setQuery('😀'.repeat(100)); await store.search(); expect(search).toHaveBeenCalledTimes(3);
});
it("ignores responses from an older query even when they arrive last", async () => {
  const { search, store } = fixture(); const old = deferred<SearchPage>(); search.mockImplementationOnce(() => old.promise);
  store.setQuery('old'); const first = store.search(); store.setQuery('new'); await store.search();
  old.resolve({ ...result('todo','old'), total: 99 }); await first;
  expect(get(store).query).toBe('new'); expect(get(store).groups[0].total).toBe(0);
});
it("guards racing pages independently without discarding other categories", async () => {
  const { search, store } = fixture(); store.setQuery('term'); await store.search();
  const old = deferred<SearchPage>(); search.mockImplementationOnce(() => old.promise);
  const first = store.page('todo',20); await store.page('todo',40);
  old.resolve({ ...result('todo','term',20), total: 99 }); await first;
  expect(get(store).groups.map(g=>g.offset)).toEqual([40,0,0]); expect(get(store).groups[0].total).toBe(0);
});
it("keeps partial failure explicit and retries the failed group at the same offset", async () => {
  const { search, store } = fixture(); search.mockImplementationOnce(async () => { throw Error('private'); });
  store.setQuery('term'); await store.search(); expect(get(store).groups.map(g=>g.status)).toEqual(['failed','ready','ready']);
  await store.page('todo',0); expect(get(store).groups[0].status).toBe('ready'); expect(get(store).groups[1].status).toBe('ready');
});
it("clear and dispose invalidate pending reads without repopulating results", async () => {
  for (const dispose of [false,true]) {
    const { search, store } = fixture(); const pending = deferred<SearchPage>(); search.mockImplementationOnce(()=>pending.promise);
    store.setQuery('old'); const request = store.search();
    if (dispose) store.dispose(); else store.setQuery(''); const current = get(store);
    pending.resolve({ ...result('todo','old'), total: 99 }); await request;
    expect(get(store)).toEqual(current);
  }
});
it("rejects invalid page requests and paging before a submitted query", async () => {
  const { search, store } = fixture(); await store.page('todo',0); expect(search).not.toHaveBeenCalled();
  store.setQuery('term'); await store.search(); search.mockClear();
  for (const offset of [-1,0.5,NaN,100001]) await store.page('todo',offset);
  expect(search).not.toHaveBeenCalled();
});
it("filters archived tasks before requesting fresh first pages and retains the choice", async () => {
  const { search, store } = fixture(); store.setQuery('term'); await store.search();
  await store.page('todo', 20); await store.setIncludeArchived(false);
  expect(get(store).groups.map(g => g.offset)).toEqual([0,0,0]);
  expect(search).toHaveBeenLastCalledWith('attachment','term',0,20,false);
  store.setQuery('next'); await store.search(); expect(get(store).includeArchived).toBe(false);
});
it("refreshes the current pages and retreats only if the last page became empty", async () => {
  const { search, store } = fixture();
  search.mockImplementation(async (scope, query, offset) => ({ ...result(scope,query,offset), total: 45 }));
  store.setQuery('term'); await store.search(); await store.page('todo',40); await store.refresh();
  expect(get(store).groups[0].offset).toBe(40);
  search.mockImplementation(async (scope, query, offset) => ({ ...result(scope,query,offset), total: 40 }));
  await store.refresh(); expect(get(store).groups[0].offset).toBe(20);
});
it("does not let an old refresh repage a newer query", async () => {
  const { search, store } = fixture(); store.setQuery('old'); await store.search(); await store.page('todo',20);
  const old = deferred<SearchPage>(); search.mockImplementationOnce(() => old.promise);
  const refresh = store.refresh(); store.setQuery('new'); await store.search(); await store.page('todo',20);
  old.resolve(result('todo','old',20)); await refresh;
  expect(get(store).query).toBe('new'); expect(get(store).groups[0].offset).toBe(20);
});
