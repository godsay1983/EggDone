import { get, writable } from 'svelte/store';
import { calendarTodoApi, type CalendarTodoDraft, type CalendarTodoResult } from '$lib/api/calendarTodoApi';
import { calendarTodoIssue } from '$lib/utils/calendarTodoDraft';

type Failure = 'invalid' | 'group' | 'unavailable' | 'retry' | 'uncertain' | null;
export function createCalendarTodoSession(draft: CalendarTodoDraft,
  api = calendarTodoApi, committed: (result: CalendarTodoResult) => void = () => {}) {
  const state = writable({ draft: { ...draft }, busy: false, uncertain: false,
    result: null as CalendarTodoResult | null, error: null as Failure });
  const uuid = draft.uuid;
  let delivered = false;
  function success(result: CalendarTodoResult) {
    state.update(s => ({ ...s, result, uncertain: false, error: null }));
    if (!delivered) {
      delivered = true;
      // Local persistence is already confirmed. UI/sync side effects cannot make it a failed save.
      try { committed(result); } catch { /* The saved task remains available through All/Search. */ }
    }
  }
  function known(error: unknown): Failure {
    const text = String(error);
    if (text.includes('CALENDAR_TODO_UNAVAILABLE')) return 'unavailable';
    if (text.includes('CALENDAR_TODO_GROUP_UNAVAILABLE')) return 'group';
    if (text.includes('CALENDAR_TODO_INVALID')) return 'invalid';
    return null;
  }
  async function resolve() {
    try {
      const result = await api.resolve(uuid);
      if (result) success(result);
      else state.update(s => ({ ...s, uncertain: false, error: 'retry' }));
    } catch (error) {
      const failure = known(error);
      state.update(s => ({ ...s, uncertain: !failure, error: failure || 'uncertain' }));
    }
  }
  return {
    subscribe: state.subscribe,
    edit(patch: Partial<Omit<CalendarTodoDraft, 'uuid'>>) {
      const current = get(state);
      if (!current.busy && !current.uncertain && !current.result && current.error !== 'unavailable')
        state.update(s => ({ ...s, draft: { ...s.draft, ...patch }, error: null }));
    },
    async save() {
      const current = get(state);
      if (current.busy || current.result || current.error === 'unavailable') return;
      if (!current.uncertain && calendarTodoIssue(current.draft)) return;
      state.update(s => ({ ...s, busy: true, error: null }));
      try {
        if (current.uncertain) await resolve();
        else {
          try { success(await api.create({ ...current.draft })); }
          catch (error) {
            const failure = known(error);
            if (failure) state.update(s => ({ ...s, error: failure }));
            else { state.update(s => ({ ...s, uncertain: true })); await resolve(); }
          }
        }
      } finally { state.update(s => ({ ...s, busy: false })); }
    },
  };
}
