import { expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { systemCalendarApi } from './systemCalendarApi';
import active from '../../../tests/fixtures/system-calendar-v1-active.json';
import withdrawn from '../../../tests/fixtures/system-calendar-v1-withdrawn.json';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

it('uses only the two receive-only no-argument native commands and preserves wire fields', async () => {
  for (const document of [active, withdrawn]) {
    const state = { document, configured: true, last_received_at: 42, loading: false, error: '' };
    vi.mocked(invoke).mockResolvedValue(state);
    expect(await systemCalendarApi.getState()).toBe(state);
    expect(invoke).toHaveBeenLastCalledWith('get_system_calendar_state');
    expect(await systemCalendarApi.refresh()).toBe(state);
    expect(invoke).toHaveBeenLastCalledWith('refresh_system_calendar');
  }
});
