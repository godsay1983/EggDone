import type { RecurrenceRule } from '../types/recurrence';
export function recurrenceSummary(rule: RecurrenceRule, text: (key: string) => string): string {
  const s = rule.schedule;
  const frequency = s.frequency === 'weekly' ? 'summaryWeekly' : s.frequency === 'monthly' ? 'summaryMonthly' : 'summaryDaily';
  const parts: string[] = [text(frequency).replace('{n}', String(s.interval))];
  const days: string[] = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];
  if (s.frequency === 'weekly') parts.push(s.weekdays.map((n: number): string => text(days[n - 1])).join(', '));
  if (s.frequency === 'monthly') parts.push(s.month_day === 0 ? text('lastDay') : String(s.month_day));
  if (s.local_time_minutes === null) parts.push(text('allDay'));
  else parts.push(String(Math.floor(s.local_time_minutes / 60)).padStart(2, '0') + ':' +
    String(s.local_time_minutes % 60).padStart(2, '0') + ' ' + (rule.timezone_id ?? ''));
  if (s.end_type === 'date') parts.push(text('summaryUntil').replace('{date}', s.end_date ?? ''));
  if (s.end_type === 'count') parts.push(text('summaryCount').replace('{n}', String(s.max_occurrences)));
  if (rule.deleted_at !== null) parts.push(text('stopped'));
  else if (rule.exhausted) parts.push(text('exhausted'));
  return parts.join(' · ');
}
export function recurrenceErrorKey(error: string): string {
  if (/CONFLICT|OCCURRENCE_MISSING/.test(error)) return 'conflict';
  if (/TIMEZONE|RECURRENCE_TIME/.test(error)) return 'timezoneError';
  if (/INVALID_RECURRENCE/.test(error)) return 'invalid';
  return 'failed';
}
