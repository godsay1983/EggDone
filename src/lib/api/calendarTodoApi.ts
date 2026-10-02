import { invoke } from '@tauri-apps/api/core';
import type { Todo } from '$lib/types';

export interface CalendarTodoDraft {
  uuid: string;
  title: string;
  note: string;
  group_uuid: string | null;
}
export interface CalendarTodoResult { todo: Todo; created: boolean }
export const calendarTodoApi = {
  create(draft: CalendarTodoDraft): Promise<CalendarTodoResult> {
    return invoke('create_calendar_todo', { draft });
  },
  resolve(uuid: string): Promise<CalendarTodoResult | null> {
    return invoke('resolve_calendar_todo', { uuid });
  },
};
