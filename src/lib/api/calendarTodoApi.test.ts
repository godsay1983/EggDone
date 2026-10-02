import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { calendarTodoApi } from './calendarTodoApi';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
describe('calendar task scoped API', () => {
  it('uses the agreed draft shape and stable UUID result lookup', async () => {
    const draft = { uuid: 'uuid', title: 'title', note: 'note', group_uuid: null };
    await calendarTodoApi.create(draft); await calendarTodoApi.resolve(draft.uuid);
    expect(invoke).toHaveBeenCalledWith('create_calendar_todo', { draft });
    expect(invoke).toHaveBeenCalledWith('resolve_calendar_todo', { uuid: 'uuid' });
  });
});
