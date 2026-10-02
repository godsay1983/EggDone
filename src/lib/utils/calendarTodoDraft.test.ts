import { describe, expect, it } from 'vitest';
import { calendarTodoDraft, calendarTodoIssue } from './calendarTodoDraft';
import type { CalendarOccurrence } from '$lib/types/systemCalendar';
import fixture from '../../../docs/fixtures/calendar-todo-v1.json';
const labels = { fallback: 'Calendar Todo', title: 'Event', time: 'Time', allDay: 'All day', zone: 'Display time zone', location: 'Location', calendar: 'Calendar' };
const occurrence: CalendarOccurrence = { id: 'private-event', calendarId: 'private-calendar', title: '明天 meeting <b>plain</b>', startTime: Date.UTC(2026,9,2,23), endTime: Date.UTC(2026,9,3,1), isAllDay: false, startDate: '2026-10-02', endDateExclusive: '2026-10-04', timeZone: 'unknown/untrusted', location: ' Room ' };
describe('calendar todo draft', () => {
  it.each(fixture.cases)('shared fixture: $id', testCase => {
    const draft = calendarTodoDraft(testCase.occurrence, testCase.calendar, fixture.labels['en-US'], fixture.draft_uuid, fixture.display_timezone);
    if (testCase.expected_title) expect(draft.title).toBe(testCase.expected_title);
    if (testCase.expected_note_en) expect(draft.note).toBe(testCase.expected_note_en);
    if (testCase.expected_note_zh) expect(calendarTodoDraft(testCase.occurrence, testCase.calendar, fixture.labels['zh-CN'], fixture.draft_uuid, fixture.display_timezone).note).toBe(testCase.expected_note_zh);
    expect(calendarTodoIssue(draft)).toBe(testCase.expected_issue ?? null);
    expect(draft.group_uuid).toBeNull();
  });
  it('copies full timestamps using display zone without parsing source zone or adding schedule', () => {
    const draft = calendarTodoDraft(occurrence, 'Work', labels, 'draft', 'UTC');
    expect(draft).toEqual({ uuid: 'draft', title: occurrence.title, group_uuid: null,
      note: 'Event: 明天 meeting <b>plain</b>\nTime: 2026-10-02 23:00 - 2026-10-03 01:00\nDisplay time zone: UTC\nLocation: Room\nCalendar: Work' });
    expect(draft.note).not.toContain('private');
  });
  it('preserves original all-day inclusive range independent of zone and selected date', () => {
    const draft = calendarTodoDraft({ ...occurrence, isAllDay: true }, '', labels, 'draft', 'America/New_York');
    expect(draft.note).toBe('Event: 明天 meeting <b>plain</b>\nAll day: 2026-10-02 - 2026-10-03\nLocation: Room');
  });
  it('uses localized fallback without inventing calendar or location', () => {
    const draft = calendarTodoDraft({ ...occurrence, title: ' \n ', location: ' ' }, '', { ...labels, fallback: '日程待办' }, 'draft', 'UTC');
    expect(draft.title).toBe('日程待办'); expect(draft.note).not.toContain('Calendar:');
  });
  it('never silently truncates and enforces UTF16 note boundary including emoji', () => {
    const draft = calendarTodoDraft({ ...occurrence, location: '😀'.repeat(510) }, '', labels, 'draft', 'UTC');
    expect(draft.note).toContain('😀'.repeat(510)); expect(calendarTodoIssue(draft)).toBe('note');
    expect(calendarTodoIssue({ ...draft, note: '😀'.repeat(500) })).toBeNull();
    expect(calendarTodoIssue({ ...draft, title: '', note: '' })).toBe('title');
  });
  it('rejects control characters and does not truncate long title', () => {
    const draft = calendarTodoDraft({ ...occurrence, title: 'x'.repeat(101) }, '', labels, 'draft', 'UTC');
    expect(draft.title.length).toBe(101); expect(calendarTodoIssue(draft)).toBe('title');
    expect(calendarTodoIssue({ ...draft, title: 'OK', note: '\u0000' })).toBe('note');
  });
});
