import { get, writable } from 'svelte/store';
import { taskProgressApi } from '$lib/api/taskProgressApi';
import { normalizeProgressBody } from '$lib/utils/taskProgressBody';
import type { ProgressPage, ProgressView, ProgressWrite } from '$lib/types/taskProgress';

export type ProgressError = 'load' | 'retry' | 'conflict' | 'deleted' | 'unavailable' | 'readOnly' | 'invalid' | 'limit';
export function progressError(error: unknown): ProgressError {
  const code = String(error);
  if (code.includes('PROGRESS_READ_ONLY')) return 'readOnly';
  for (const name of ['CONFLICT', 'DELETED', 'UNAVAILABLE', 'INVALID', 'LIMIT'] as const) {
    if (code.includes(`PROGRESS_${name}`)) return name.toLowerCase() as ProgressError;
  }
  return 'retry';
}
export interface ProgressState {
  page: ProgressPage | null;
  draft: string;
  editing: ProgressView | null;
  loading: boolean;
  busy: boolean;
  error: ProgressError | null;
  loadFailed: boolean;
  pending: ProgressWrite | null;
}
function sorted(entries: ProgressView[]) {
  return [...new Map(entries.map(entry => [entry.record.uuid, entry])).values()]
    .filter(entry => entry.record.deleted_at === null)
    .sort((a, b) => b.record.created_at - a.record.created_at || (a.record.uuid < b.record.uuid ? 1 : a.record.uuid > b.record.uuid ? -1 : 0));
}
export function createTaskProgressSession(taskUuid: string, api = taskProgressApi,
  changed: (page: ProgressPage) => void = () => {}, uuid: () => string = () => crypto.randomUUID()) {
  const state = writable<ProgressState>({ page: null, draft: '', editing: null, loading: false,
    busy: false, error: null, loadFailed: false, pending: null });
  let sequence = 0, disposed = false, refreshQueued = false;
  let creationUuid: string | null = null;
  async function refresh(more = false) {
    const current = get(state);
    if (disposed) return;
    if (current.busy) { refreshQueued = true; return; }
    if (more && (current.loading || !current.page?.next_cursor)) return;
    const ticket = ++sequence;
    const cursor = more ? current.page?.next_cursor ?? null : null;
    state.update(s => ({ ...s, loading: true }));
    try {
      const page = await api.list(taskUuid, cursor);
      if (disposed || ticket !== sequence) return;
      state.update(s => ({ ...s, loading: false, loadFailed: false,
        error: s.error === 'load' ? null : s.error === 'unavailable' ? 'deleted' : s.error,
        page: { ...page, entries: sorted([...(more ? s.page?.entries ?? [] : []), ...page.entries]) } }));
    } catch (error) {
      if (disposed || ticket !== sequence) return;
      const failure = progressError(error);
      // An unavailable parent is the backend's verified deletion/missing boundary, not an empty page.
      if (failure === 'unavailable') { creationUuid = null; state.update(s => ({ ...s, page: null, draft: '', editing: null, pending: null })); }
      state.update(s => ({ ...s, loading: false, loadFailed: true, error: failure === 'retry' ? 'load' : failure }));
    }
  }
  async function submit(request: ProgressWrite) {
    if (disposed || get(state).busy) return;
    ++sequence;
    state.update(s => ({ ...s, busy: true, loading: false, pending: request, error: null }));
    try {
      const page = await api.write({ ...request });
      if (disposed) return;
      ++sequence;
      const clearDraft = request.action !== 'delete';
      if (clearDraft) creationUuid = null;
      state.update(s => ({ ...s, page: { ...page, entries: sorted(page.entries) }, pending: null,
        busy: false, loadFailed: false, error: null,
        draft: clearDraft ? '' : s.draft, editing: clearDraft ? null : s.editing }));
      // Optional refresh/sync work cannot turn a committed transaction into a retry.
      try { changed(page); } catch { /* Persisted dirty state remains authoritative. */ }
    } catch (error) {
      if (disposed) return;
      const failure = progressError(error);
      state.update(s => ({ ...s, busy: false, error: failure, pending: failure === 'retry' ? request : null }));
      // Writes also use UNAVAILABLE for a missing record. Confirm the parent before erasing input.
      if (failure === 'unavailable' || failure === 'readOnly' || failure === 'deleted') refreshQueued = true;
    } finally {
      if (refreshQueued && !disposed) { refreshQueued = false; await refresh(); }
    }
  }
  return {
    subscribe: state.subscribe, refresh,
    setDraft(draft: string) { if (!get(state).busy) state.update(s => ({ ...s, draft })); },
    edit(entry: ProgressView) {
      if (get(state).busy || get(state).pending) return;
      creationUuid = null;
      state.update(s => ({ ...s, editing: structuredClone(entry), draft: entry.record.body, error: null }));
    },
    cancelEdit() {
      if (get(state).busy) return;
      creationUuid = null;
      state.update(s => ({ ...s, editing: null, draft: '', pending: null, error: null }));
    },
    async save() {
      const s = get(state);
      if (s.busy || s.loading || s.loadFailed || !s.page || s.page.read_only) return;
      const body = normalizeProgressBody(s.draft);
      if (body.error) { state.update(s => ({ ...s, error: body.error })); return; }
      if (s.pending?.action === 'delete') return;
      const recordUuid = s.editing?.record.uuid ?? (creationUuid ??= uuid());
      const draft = { task_uuid: taskUuid, record_uuid: recordUuid,
        action: s.editing ? 'edit' as const : 'create' as const, body: body.body,
        expected_record: s.editing?.token ?? null };
      const same = s.pending && Object.entries(draft).every(([key, value]) => s.pending?.[key as keyof ProgressWrite] === value);
      // Changed input uses a new operation but the same record identity and stale expected token.
      await submit(same ? s.pending! : { ...draft, operation_uuid: uuid() });
    },
    async remove(entry: ProgressView) {
      const s = get(state);
      if (s.busy || s.loading || s.loadFailed || !s.page || s.page.read_only || s.pending) return;
      await submit({ operation_uuid: uuid(), task_uuid: taskUuid, record_uuid: entry.record.uuid,
        action: 'delete', body: '', expected_record: entry.token });
    },
    async retry() { const s = get(state); if (s.pending && !s.busy) await submit(s.pending); },
    async rebase() {
      if (get(state).busy || get(state).pending) return;
      await refresh();
      const editingUuid = get(state).editing?.record.uuid;
      while (editingUuid && !get(state).loadFailed && get(state).page?.next_cursor &&
        !get(state).page?.entries.some(entry => entry.record.uuid === editingUuid)) await refresh(true);
      state.update(s => {
        if (s.loadFailed || !s.page) return s;
        const current = s.editing && s.page.entries.find(e => e.record.uuid === s.editing?.record.uuid);
        if (s.editing && !current) return { ...s, error: 'deleted' };
        return { ...s, editing: current ? structuredClone(current) : s.editing,
          error: !s.editing && creationUuid && s.page.entries.some(e => e.record.uuid === creationUuid) ? 'conflict' : null };
      });
    },
    async dismissNotice() {
      try { await api.dismissNotice(taskUuid); if (!disposed) state.update(s => ({ ...s, page: s.page && { ...s.page, overwritten: false } })); }
      catch { if (!disposed) state.update(s => ({ ...s, error: 'retry' })); }
    },
    dispose() { disposed = true; ++sequence; creationUuid = null; state.set({ page: null, draft: '', editing: null, loading: false, busy: false, error: null, loadFailed: false, pending: null }); },
  };
}
