import { get } from 'svelte/store';
import { afterEach, describe, expect, it, vi } from 'vitest';
import active from '../../../tests/fixtures/system-calendar-v1-active.json';
import withdrawn from '../../../tests/fixtures/system-calendar-v1-withdrawn.json';
import type { CalendarDocument, SystemCalendarState } from '$lib/types/systemCalendar';
import type { SyncSettings } from '$lib/api/syncApi';
import { createSystemCalendarStore } from './systemCalendarStore';
import { CALENDAR_REFRESH_MS } from '$lib/utils/systemCalendarDates';

const settings: SyncSettings = { enabled: true, credentialsConfigured: true, endpoint: 'https://fixture.invalid',
  bucket: 'fixture', region: 'test', objectKey: 'eggdone/todos.json', pathStyle: true, allowHttp: false,
  noteObjectKey: '', noteAttachmentObjectKey: '', noteAssetPrefix: '' };
const response = (document: CalendarDocument | null = active as CalendarDocument): SystemCalendarState => ({
  document, loading: false, error: '', last_received_at: 100, configured: true,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function setup() {
  const api = { getState: vi.fn(async () => response()), refresh: vi.fn(async () => response()) };
  const store = createSystemCalendarStore(api);
  return { api, store };
}
afterEach(() => vi.useRealTimers());

describe('system calendar independent async store', () => {
  it('rehydrates the native cache and distinguishes missing, withdrawn and failures', async () => {
    const { api, store } = setup();
    store.configure(settings);
    await store.rehydrate();
    expect(get(store).document).toEqual(active);
    api.refresh.mockResolvedValueOnce(response(withdrawn as CalendarDocument));
    await store.refresh();
    expect(get(store).document?.state).toBe('withdrawn');
    api.refresh.mockResolvedValueOnce(response(null));
    await store.refresh();
    expect(get(store)).toMatchObject({ document: null, error: '', hydrated: true });
    api.refresh.mockRejectedValueOnce(Error('secret endpoint'));
    await store.refresh();
    expect(get(store)).toMatchObject({ document: null, error: 'CALENDAR_REFRESH_FAILED', loading: false });
  });

  it('coalesces overlapping refreshes and retains cache on failure', async () => {
    const { api, store } = setup();
    store.configure(settings); await store.rehydrate();
    const job = deferred<SystemCalendarState>(); api.refresh.mockReturnValueOnce(job.promise);
    const first = store.refresh(); const second = store.refresh();
    expect(first).toBe(second);
    expect(api.refresh).toHaveBeenCalledTimes(1);
    job.reject(Error('network'));
    await first;
    expect(get(store)).toMatchObject({ document: active, loading: false, error: 'CALENDAR_REFRESH_FAILED' });
  });

  it('hides old data immediately and rejects in-flight cache/remote replies during saves', async () => {
    for (const kind of ['getState', 'refresh'] as const) {
      const { api, store } = setup();
      store.configure(settings); await store.rehydrate();
      const old = deferred<SystemCalendarState>(); api[kind].mockReturnValueOnce(old.promise);
      const pending = kind === 'getState' ? store.rehydrate() : store.refresh();
      store.beginSettingsUpdate();
      expect(get(store)).toMatchObject({ document: null, changingTarget: true, last_received_at: 0 });
      await store.rehydrate(); await store.refresh();
      store.configure({ ...settings, bucket: 'new-target' });
      old.resolve(response()); await pending;
      expect(get(store).document).toBeNull();
      api.getState.mockResolvedValueOnce(response(null));
      store.endSettingsUpdate(); await store.rehydrate();
      expect(get(store)).toMatchObject({ document: null, changingTarget: false, hydrated: true });
    }
  });

  it('guards queued rehydration when target changes again and serializes IPC across epochs', async () => {
    const { api, store } = setup();
    const old = deferred<SystemCalendarState>(); api.getState.mockReturnValueOnce(old.promise);
    store.configure(settings);
    store.configure({ ...settings, bucket: 'second' });
    store.configure({ ...settings, bucket: 'third' });
    api.getState.mockResolvedValue(response(null));
    old.resolve(response());
    await store.rehydrate();
    expect(api.getState).toHaveBeenCalledTimes(2);
    expect(get(store).document).toBeNull();
  });

  it('keeps overlapping settings changes blocked and safely rehydrates after failed saves', async () => {
    const { store } = setup(); store.configure(settings); await store.rehydrate();
    store.beginSettingsUpdate(); store.beginSettingsUpdate();
    store.endSettingsUpdate(); await store.rehydrate();
    expect(get(store).document).toBeNull();
    store.endSettingsUpdate(); await store.rehydrate();
    expect(get(store).document).toEqual(active);
  });

  it('handles credential replacement at the same target and disable without restoring old data', async () => {
    const { api, store } = setup(); store.configure(settings); await store.rehydrate();
    store.beginSettingsUpdate(); store.configure(settings);
    api.getState.mockResolvedValue(response(null));
    store.endSettingsUpdate(); await store.rehydrate();
    expect(get(store).document).toBeNull();
    store.configure({ ...settings, enabled: false }); await store.refresh();
    expect(get(store)).toMatchObject({ document: null, configured: false });
    expect(api.refresh).not.toHaveBeenCalled();
  });

  it('refreshes on show, foreground and five-minute timer but never while hidden', async () => {
    vi.useFakeTimers();
    const { api, store } = setup(); store.configure(settings); await store.rehydrate();
    store.panelShown(); await store.refresh();
    expect(api.refresh).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(CALENDAR_REFRESH_MS - 1);
    expect(api.refresh).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(api.refresh).toHaveBeenCalledTimes(2);
    store.setForeground(false); await vi.advanceTimersByTimeAsync(CALENDAR_REFRESH_MS * 2);
    expect(api.refresh).toHaveBeenCalledTimes(2);
    store.setForeground(true); await store.refresh();
    expect(api.refresh).toHaveBeenCalledTimes(3);
    store.setForeground(false);
  });

  it('defers calendar work until task synchronization completes without blocking its result', async () => {
    vi.useFakeTimers();
    const { api, store } = setup(); store.configure(settings); await store.rehydrate();
    store.setForeground(true);
    store.setTaskSyncing(true);
    store.afterTaskSync(); await store.refresh();
    expect(api.refresh).not.toHaveBeenCalled();
    const calendar = deferred<SystemCalendarState>(); api.refresh.mockReturnValueOnce(calendar.promise);
    store.setTaskSyncing(false);
    await Promise.resolve();
    expect(api.refresh).toHaveBeenCalledTimes(1);
    calendar.reject(Error('calendar only')); await store.refresh();
    expect(get(store).error).toBe('CALENDAR_REFRESH_FAILED');
    store.setForeground(false);
  });

  it('drops automatic work hidden before dispatch, task completion or cache completion', async () => {
    vi.useFakeTimers();
    const { api, store } = setup(); store.configure(settings); await store.rehydrate();
    store.panelShown(); store.setForeground(false);
    await Promise.resolve();
    expect(api.refresh).not.toHaveBeenCalled();
    store.setTaskSyncing(true); store.panelShown(); await Promise.resolve();
    store.setForeground(false); store.afterTaskSync(); store.setTaskSyncing(false);
    await Promise.resolve();
    expect(api.refresh).not.toHaveBeenCalled();
    const cache = deferred<SystemCalendarState>(); api.getState.mockReturnValueOnce(cache.promise);
    const pending = store.rehydrate();
    store.panelShown(); await Promise.resolve(); store.setForeground(false);
    cache.resolve(response()); await pending; await Promise.resolve();
    expect(api.refresh).not.toHaveBeenCalled();
    await store.refresh();
    expect(api.refresh).toHaveBeenCalledTimes(1);
  });

  it('gives same-turn task start priority over panel show/focus and coalesces automatic requests', async () => {
    const { api, store } = setup(); store.configure(settings); await store.rehydrate();
    store.panelShown(); store.setForeground(true); store.panelShown();
    expect(api.refresh).not.toHaveBeenCalled();
    store.setTaskSyncing(true);
    await Promise.resolve();
    expect(api.refresh).not.toHaveBeenCalled();
    store.afterTaskSync(); await Promise.resolve();
    expect(api.refresh).not.toHaveBeenCalled();
    store.setTaskSyncing(false); await Promise.resolve();
    expect(api.refresh).toHaveBeenCalledTimes(1);
    await store.refresh();
    store.setForeground(false);
  });

  it('keeps explicit refresh immediate but drops queued automatic work after a target change', async () => {
    const { api, store } = setup(); store.configure(settings); await store.rehydrate();
    const manual = store.refresh();
    expect(api.refresh).toHaveBeenCalledTimes(1);
    await manual;
    store.panelShown(); store.beginSettingsUpdate();
    await Promise.resolve();
    expect(api.refresh).toHaveBeenCalledTimes(1);
    store.configure({ ...settings, enabled: false }); store.endSettingsUpdate();
    store.setForeground(false);
  });
});
