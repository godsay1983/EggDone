import { writable } from 'svelte/store';
import { workReviewApi } from '$lib/api/workReviewApi';
import { contentSearchApi, type SearchTarget } from '$lib/api/contentSearchApi';
import type { ReviewCursor, ReviewPage, ReviewQuery, ReviewRow, ReviewSnapshot } from '$lib/types/workReview';
import { compareReviewRows, formatReviewSummary, normalizeReviewQuery, type ReviewSummaryOptions } from '$lib/utils/workReview';

export type ReviewError = 'invalid' | 'database' | 'changed' | 'limit' | 'clipboard' | 'unavailable';
export interface WorkReviewPort {
  list(query: ReviewQuery, cursor: ReviewCursor | null): Promise<ReviewPage>;
  snapshot(query: ReviewQuery): Promise<ReviewSnapshot>;
  validate(query: ReviewQuery, snapshotToken: string): Promise<boolean>;
}
export interface WorkReviewState {
  query: ReviewQuery | null;
  rows: ReviewRow[];
  cursor: ReviewCursor | null;
  entries: number;
  tasks: number;
  token: string | null;
  loading: boolean;
  ready: boolean;
  copying: boolean;
  copied: boolean;
  opening: boolean;
  blocked: boolean;
  error: ReviewError | null;
}
export function reviewError(reason: unknown): ReviewError {
  const message = reason instanceof Error ? reason.message : String(reason);
  const code = message.match(/\bREVIEW_(INVALID|DATABASE|CHANGED|LIMIT|CLIPBOARD)\b/)?.[1];
  return ({ INVALID: 'invalid', DATABASE: 'database', CHANGED: 'changed', LIMIT: 'limit', CLIPBOARD: 'clipboard' } as const)[code as 'INVALID'] ?? 'database';
}

export function createWorkReviewStore(api: WorkReviewPort = workReviewApi,
  clipboard: (text: string) => Promise<void> = text => navigator.clipboard.writeText(text),
  resolve: (uuid: string) => Promise<SearchTarget> = uuid => contentSearchApi.resolve('todo', uuid)) {
  const empty = (): WorkReviewState => ({ query: null, rows: [], cursor: null, entries: 0, tasks: 0, token: null,
    loading: false, ready: false, copying: false, copied: false, opening: false, blocked: false, error: null });
  let value = empty(), generation = 0, disposed = false, target = '', retainedDepth = 0;
  const state = writable(value);
  const publish = (next: WorkReviewState) => { value = next; state.set(next); };
  const live = (ticket: number) => !disposed && !value.blocked && generation === ticket;
  function clear(error: ReviewError | null = null) {
    generation++;
    publish({ ...empty(), query: value.query, blocked: value.blocked, error });
  }
  function setQuery(query: ReviewQuery | null) {
    if (disposed) return;
    const normalized = query ? normalizeReviewQuery(query) : null;
    if (JSON.stringify(normalized) === JSON.stringify(value.query)) return;
    retainedDepth = 0;
    clear(normalized ? null : 'invalid');
    publish({ ...value, query: normalized });
  }
  async function refresh(keepDepth = false) {
    if (disposed) return;
    const desired = keepDepth ? retainedDepth : 0;
    if (!keepDepth) retainedDepth = 0;
    clear(value.query ? null : 'invalid');
    if (!value.query || value.blocked) return;
    const ticket = generation;
    await page(false);
    while (live(ticket) && value.ready && value.cursor && value.rows.length < desired) {
      const previous = value.rows.length;
      await page(true);
      if (value.rows.length <= previous) break;
    }
  }
  async function page(append: boolean) {
    if (disposed || value.blocked || !value.query || value.loading || (append && (!value.ready || !value.cursor))) return;
    const query = { ...value.query }, cursor = append ? value.cursor : null, ticket = generation;
    publish({ ...value, loading: true, error: null, copied: false });
    try {
      const result = await api.list(query, cursor);
      if (!live(ticket)) return;
      if (append && result.snapshot_token !== value.token) throw Error('REVIEW_CHANGED');
      const rows = [...new Map([...(append ? value.rows : []), ...result.rows].map(row => [row.record_uuid, row])).values()].sort(compareReviewRows);
      retainedDepth = Math.max(retainedDepth, rows.length);
      publish({ ...value, rows, cursor: result.next_cursor, entries: result.matching_entry_count,
        tasks: result.matching_task_count, token: result.snapshot_token, loading: false, ready: true });
    } catch (reason) {
      if (!live(ticket)) return;
      const error = reviewError(reason);
      if (append && error === 'changed') { await refresh(); return; }
      clear(error);
    }
  }
  async function copy(options: ReviewSummaryOptions): Promise<boolean> {
    if (disposed || !value.query || value.blocked || !value.ready || value.loading || value.copying || !value.entries) return false;
    const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    const ticket = generation, query = { ...value.query }, fixed = { ...options, dates: { ...options.dates }, timeZone };
    publish({ ...value, copying: true, copied: false, error: null });
    try {
      const snapshot = await api.snapshot(query);
      if (!live(ticket)) return false;
      if (!snapshot.rows.length) throw Error('REVIEW_CHANGED');
      if (query.group_scope === 'group' && snapshot.rows[0].group_name) fixed.groupName = snapshot.rows[0].group_name;
      const text = formatReviewSummary(snapshot, fixed);
      const valid = await api.validate(query, snapshot.snapshot_token);
      if (!live(ticket)) return false;
      if (!valid) throw Error('REVIEW_CHANGED');
      if (Intl.DateTimeFormat().resolvedOptions().timeZone !== timeZone) throw Error('REVIEW_CHANGED');
      // The browser write cannot be revoked once handed to the OS.
      try { await clipboard(text); } catch { throw Error('REVIEW_CLIPBOARD'); }
      if (!live(ticket)) return false;
      publish({ ...value, copied: true });
      return true;
    } catch (reason) {
      if (live(ticket)) {
        const error = reviewError(reason);
        if (error === 'changed') {
          const reloading = refresh(), reloadTicket = generation;
          await reloading;
          if (live(reloadTicket)) publish({ ...value, error: 'changed' });
        }
        else publish({ ...value, error });
      }
      return false;
    } finally { if (live(ticket)) publish({ ...value, copying: false }); }
  }
  async function open(row: ReviewRow): Promise<SearchTarget | null> {
    if (disposed || value.blocked || value.loading || value.opening || !value.query || !value.token || !value.rows.some(item => item.record_uuid === row.record_uuid)) return null;
    const ticket = generation, query = { ...value.query }, token = value.token;
    publish({ ...value, opening: true, error: null });
    try {
      if (!await api.validate(query, token)) throw Error('REVIEW_CHANGED');
      if (!live(ticket)) return null;
      const result = await resolve(row.task_uuid);
      if (!live(ticket)) return null;
      if (!await api.validate(query, token)) throw Error('REVIEW_CHANGED');
      if (!live(ticket)) return null;
      return result;
    } catch (reason) {
      if (live(ticket)) {
        const error = reviewError(reason) === 'changed' ? 'changed' : 'unavailable';
        const reloading = refresh(), reloadTicket = generation;
        await reloading;
        if (live(reloadTicket)) publish({ ...value, error });
      }
      return null;
    } finally { if (live(ticket)) publish({ ...value, opening: false }); }
  }
  return { subscribe: state.subscribe, setQuery, refresh, loadMore: () => page(true), copy, open,
    invalidate: () => refresh(true),
    setTarget(identity: string, blocked = false) {
      if (disposed || (identity === target && blocked === value.blocked)) return;
      target = identity;
      publish({ ...value, blocked });
      void refresh();
    },
    dispose() { disposed = true; clear(); },
  };
}
