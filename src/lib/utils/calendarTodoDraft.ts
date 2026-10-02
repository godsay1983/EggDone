import type { CalendarTodoDraft } from '$lib/api/calendarTodoApi';
import type { CalendarOccurrence } from '$lib/types/systemCalendar';
import { calendarAllDayLastDate } from './systemCalendarDates';
import { titleIssue, validateTaskCreationDraft } from './taskComposition';

export interface CalendarTodoLabels {
  fallback: string; title: string; time: string; allDay: string;
  zone: string; location: string; calendar: string;
}
export function calendarTodoDraft(
  occurrence: CalendarOccurrence, calendar: string, labels: CalendarTodoLabels,
  uuid: string = crypto.randomUUID(), zone = Intl.DateTimeFormat().resolvedOptions().timeZone,
): CalendarTodoDraft {
  const title = occurrence.title.trim() || labels.fallback;
  const lines = [`${labels.title}: ${title}`];
  if (occurrence.isAllDay) {
    const last = calendarAllDayLastDate(occurrence.endDateExclusive);
    lines.push(`${labels.allDay}: ${occurrence.startDate}${last !== occurrence.startDate ? ' - ' + last : ''}`);
  } else {
    // Timestamps are authoritative; the source's untrusted zone text is never parsed.
    const formatter = new Intl.DateTimeFormat('sv-SE', {
      timeZone: zone, year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
    });
    lines.push(`${labels.time}: ${formatter.format(occurrence.startTime)} - ${formatter.format(occurrence.endTime)}`);
    lines.push(`${labels.zone}: ${zone}`);
  }
  if (occurrence.location.trim()) lines.push(`${labels.location}: ${occurrence.location.trim()}`);
  if (calendar.trim()) lines.push(`${labels.calendar}: ${calendar.trim()}`);
  return { uuid, title, note: lines.join('\n'), group_uuid: null };
}

export function calendarTodoIssue(draft: CalendarTodoDraft): 'title' | 'note' | null {
  if (titleIssue(draft.title)) return 'title';
  const issues = validateTaskCreationDraft({
    draftKey: draft.uuid, title: draft.title, note: draft.note, groupUuid: draft.group_uuid,
    checklist: [], completed: false, isPinned: false, priority: 0, dueAt: null,
    reminderAt: null, repeatRule: null,
  });
  return issues.length ? 'note' : null;
}
