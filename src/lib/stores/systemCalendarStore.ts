import { writable } from 'svelte/store';
import { systemCalendarApi } from '$lib/api/systemCalendarApi';
import type { SyncSettings } from '$lib/api/syncApi';
import type { SystemCalendarState } from '$lib/types/systemCalendar';
import { CALENDAR_REFRESH_MS } from '$lib/utils/systemCalendarDates';
import { calendarErrorCode } from '$lib/utils/systemCalendarErrors';

export interface CalendarViewState extends SystemCalendarState {
  hydrated: boolean;
  changingTarget: boolean;
}

const empty = (): CalendarViewState => ({ document: null, loading: false, error: '',
  last_received_at: 0, configured: false, hydrated: false, changingTarget: false });

export function createSystemCalendarStore(api = systemCalendarApi) {
  const state = writable<CalendarViewState>(empty());
  let epoch = 0;
  let target: string | null = null;
  let configured = false;
  let settingsUpdates = 0;
  let foreground = false;
  let taskSyncing = false;
  let refreshPending = false;
  let scheduledEpoch: number | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;
  let running: { epoch: number; kind: 'cache' | 'remote'; promise: Promise<void> } | null = null;

  function invalidate() {
    epoch++;
    refreshPending = false;
    scheduledEpoch = null;
    state.set({ ...empty(), configured, changingTarget: settingsUpdates > 0 });
  }

  function request(kind: 'cache' | 'remote', version = epoch, automatic = false): Promise<void> {
    if (version !== epoch || settingsUpdates || !configured) return Promise.resolve();
    if (automatic && !foreground) return Promise.resolve();
    if (kind === 'remote' && taskSyncing) { refreshPending = true; return Promise.resolve(); }
    if (running) {
      if (running.epoch === version && (running.kind === 'remote' || kind === 'cache')) return running.promise;
      // A remote request must follow a cache read, and a new target must wait out old IPC.
      return running.promise.then(() => request(kind, version, automatic));
    }
    state.update(s => ({ ...s, loading: true }));
    const job = { epoch: version, kind, promise: Promise.resolve() };
    running = job;
    job.promise = (async () => {
      try {
        const result = await (kind === 'cache' ? api.getState() : api.refresh());
        if (version !== epoch || settingsUpdates) return;
        state.set({ ...result, document: result.configured ? result.document : null,
          hydrated: true, changingTarget: false });
      } catch (reason) {
        // Do not expose transport diagnostics: they may contain endpoint/credential details.
        if (version === epoch && !settingsUpdates) state.update(s => ({ ...s,
          loading: false, hydrated: true, error: calendarErrorCode(reason) }));
      } finally {
        if (running === job) running = null;
      }
    })();
    return job.promise;
  }

  function rehydrate() {
    const version = epoch;
    return request('cache', version).then(() => {
      if (foreground && version === epoch) scheduleRefresh();
    });
  }

  function scheduleRefresh() {
    if (!foreground) return;
    const version = epoch;
    if (scheduledEpoch === version) return;
    scheduledEpoch = version;
    // Native show/focus schedules task sync in this turn. Let it acquire priority first.
    void Promise.resolve().then(() => {
      if (scheduledEpoch !== version) return;
      scheduledEpoch = null;
      return request('remote', version, true);
    });
  }

  return {
    subscribe: state.subscribe,
    refresh: () => request('remote'),
    rehydrate,
    configure(settings: SyncSettings) {
      const identity = JSON.stringify([settings.endpoint, settings.bucket, settings.region, settings.pathStyle,
        settings.objectKey, settings.allowHttp, settings.enabled, settings.credentialsConfigured]);
      configured = settings.enabled && settings.credentialsConfigured;
      if (identity === target) return;
      target = identity;
      invalidate();
      void rehydrate();
    },
    beginSettingsUpdate() {
      settingsUpdates++;
      invalidate();
    },
    endSettingsUpdate() {
      settingsUpdates = Math.max(0, settingsUpdates - 1);
      if (settingsUpdates) return;
      state.update(s => ({ ...s, changingTarget: false }));
      void rehydrate();
    },
    setForeground(value: boolean) {
      const changed = foreground !== value;
      foreground = value;
      if (!value && timer) { clearInterval(timer); timer = null; }
      if (value && !timer) timer = setInterval(scheduleRefresh, CALENDAR_REFRESH_MS);
      if (value && changed) scheduleRefresh();
    },
    panelShown() {
      this.setForeground(true);
      scheduleRefresh();
    },
    setTaskSyncing(value: boolean) {
      taskSyncing = value;
      if (!value && refreshPending) { refreshPending = false; scheduleRefresh(); }
    },
    afterTaskSync() { scheduleRefresh(); },
  };
}

export const systemCalendar = createSystemCalendarStore();
