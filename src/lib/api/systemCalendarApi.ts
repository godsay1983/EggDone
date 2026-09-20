import { invoke } from '@tauri-apps/api/core';
import type { SystemCalendarState } from '$lib/types/systemCalendar';

// Wire validation and target-bound caching belong to the native receiver.
export const systemCalendarApi = {
  getState: (): Promise<SystemCalendarState> => invoke('get_system_calendar_state'),
  refresh: (): Promise<SystemCalendarState> => invoke('refresh_system_calendar'),
};
