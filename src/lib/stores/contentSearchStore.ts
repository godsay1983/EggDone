import { writable } from "svelte/store";
import { contentSearchApi, type SearchItem, type SearchPage, type SearchScope } from "$lib/api/contentSearchApi";

export interface SearchGroup {
  scope: SearchScope;
  status: "idle" | "loading" | "ready" | "failed";
  items: SearchItem[];
  total: number;
  offset: number;
}
export interface ContentSearchState { query: string; started: boolean; invalid: boolean; groups: SearchGroup[] }
export interface ContentSearchReader {
  search(scope: SearchScope, query: string, offset: number, limit: number): Promise<SearchPage>;
}
export const SEARCH_PAGE_SIZE = 20;
const scopes: SearchScope[] = ["todo", "note", "attachment"];
function emptyGroups(): SearchGroup[] {
  return scopes.map(scope => ({ scope, status: "idle", items: [], total: 0, offset: 0 }));
}
export function createContentSearchStore(reader: ContentSearchReader = contentSearchApi) {
  let value: ContentSearchState = { query: "", started: false, invalid: false, groups: emptyGroups() };
  const state = writable(value);
  let generation = 0, disposed = false;
  const requests = { todo: 0, note: 0, attachment: 0 };
  function publish(next: ContentSearchState) { value = next; state.set(next); }
  function setQuery(query: string) {
    if (disposed || query === value.query) return;
    generation++;
    publish({ query, started: false, invalid: false, groups: emptyGroups() });
  }
  function group(scope: SearchScope, next: SearchGroup) {
    publish({ ...value, groups: value.groups.map(item => item.scope === scope ? next : item) });
  }
  async function page(scope: SearchScope, offset: number) {
    if (disposed || !value.started || value.invalid || !Number.isInteger(offset) || offset < 0 || offset > 100000) return;
    const current = value.groups.find(item => item.scope === scope)!;
    const epoch = generation, request = ++requests[scope], query = value.query;
    group(scope, { ...current, offset, items: [], status: "loading" });
    const live = () => !disposed && epoch === generation && request === requests[scope];
    try {
      const result = await reader.search(scope, query, offset, SEARCH_PAGE_SIZE);
      if (live()) group(scope, { scope, status: "ready", offset, total: result.total, items: result.items });
    } catch {
      if (live()) group(scope, { scope, status: "failed", offset, total: 0, items: [] });
    }
  }
  async function search() {
    if (disposed) return;
    generation++;
    const query = value.query.replace(/^[\s\u0085]+|[\s\u0085]+$/g, "");
    const invalid = Array.from(query).length > 100 || query.includes("\0");
    publish({ query, invalid, started: query.length > 0 && !invalid, groups: emptyGroups() });
    if (value.started) await Promise.all(scopes.map(scope => page(scope, 0)));
  }
  return { subscribe: state.subscribe, setQuery, search, page,
    dispose() { disposed = true; generation++; } };
}
