import type { RecurrenceRule, RecurrenceSchedule } from '../types/recurrence';

export interface RuleForm {
  start: string;
  frequency: string;
  interval: string;
  weekdays: number[];
  monthDay: string;
  end: string;
  until: string;
  count: string;
  allDay: boolean;
  time: string;
  zone: string;
}
function dateValue(text: string): Date {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(text)) throw new Error('INVALID_RECURRENCE');
  const value = new Date(text + 'T00:00:00Z');
  if (!Number.isFinite(value.getTime()) || value.toISOString().slice(0, 10) !== text ||
    Number(text.slice(0, 4)) < 1900) throw new Error('INVALID_RECURRENCE');
  return value;
}
function integer(text: string, min: number, max: number): number {
  if (!/^\d+$/.test(text)) throw new Error('INVALID_RECURRENCE');
  const n = Number(text);
  if (!Number.isInteger(n) || n < min || n > max) throw new Error('INVALID_RECURRENCE');
  return n;
}
export function initialRuleForm(date: string, rule: RecurrenceRule | null, zone: string): RuleForm {
  const start = rule?.current_date ?? date;
  const schedule = rule?.schedule;
  const minutes = schedule?.local_time_minutes ?? 540;
  return { start: start, frequency: schedule?.frequency ?? 'daily', interval: String(schedule?.interval ?? 1),
    weekdays: schedule?.weekdays.slice() ?? [((dateValue(start).getUTCDay() + 6) % 7) + 1],
    monthDay: String(schedule?.month_day ?? dateValue(start).getUTCDate()),
    end: schedule?.end_type ?? 'never', until: schedule?.end_date ?? start,
    count: String(schedule?.max_occurrences ?? 10), allDay: schedule?.local_time_minutes === null || rule === null,
    time: String(Math.floor(minutes / 60)).padStart(2, '0') + ':' + String(minutes % 60).padStart(2, '0'),
    zone: rule?.timezone_id ?? zone };
}
// Resolve the first matching day at/after the chosen start; both engines require a valid anchor.
export function formSchedule(form: RuleForm): RecurrenceSchedule {
  const date = dateValue(form.start);
  const interval = integer(form.interval, 1, 99);
  const weekdays: number[] = form.frequency === 'weekly' ? form.weekdays.slice().sort((a: number, b: number): number => a - b) : [];
  let day: number | null = null;
  if (form.frequency === 'weekly') {
    if (!weekdays.length || weekdays.some((d: number, i: number): boolean => !Number.isInteger(d) || d < 1 || d > 7 || weekdays.indexOf(d) !== i)) throw new Error('INVALID_RECURRENCE');
    while (!weekdays.includes((date.getUTCDay() + 6) % 7 + 1)) date.setUTCDate(date.getUTCDate() + 1);
  } else if (form.frequency === 'monthly') {
    day = integer(form.monthDay, 0, 31);
    const requested = day;
    const last = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 0)).getUTCDate();
    const candidate = requested === 0 ? last : Math.min(requested, last);
    if (candidate < date.getUTCDate()) {
      date.setUTCDate(1);
      date.setUTCMonth(date.getUTCMonth() + 1);
    }
    const limit = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 0)).getUTCDate();
    date.setUTCDate(requested === 0 ? limit : Math.min(requested, limit));
  } else if (form.frequency !== 'daily') throw new Error('INVALID_RECURRENCE');
  const anchor = date.toISOString().slice(0, 10);
  dateValue(anchor);
  let endDate: string | null = null;
  let count: number | null = null;
  if (form.end === 'date') {
    dateValue(form.until);
    if (form.until < anchor) throw new Error('INVALID_RECURRENCE');
    endDate = form.until;
  } else if (form.end === 'count') count = integer(form.count, 1, 100000);
  else if (form.end !== 'never') throw new Error('INVALID_RECURRENCE');
  let minutes: number | null = null;
  if (!form.allDay) {
    if (!/^\d{2}:\d{2}$/.test(form.time) || !form.zone.trim()) throw new Error('INVALID_RECURRENCE_TIME');
    const parts = form.time.split(':');
    minutes = integer(parts[0], 0, 23) * 60 + integer(parts[1], 0, 59);
  }
  return { anchor_date: anchor, frequency: form.frequency, interval: interval, weekdays: weekdays,
    month_day: day, end_type: form.end, end_date: endDate, max_occurrences: count, local_time_minutes: minutes };
}
export function associatedRule(rules: RecurrenceRule[], uuid: string, series: string | null): RecurrenceRule | null {
  const current = rules.filter((r: RecurrenceRule): boolean => r.current_todo_uuid === uuid && r.deleted_at === null && !r.exhausted);
  if (current.length === 1) return current[0];
  return rules.filter((r: RecurrenceRule): boolean => r.first_todo_uuid === (series ?? uuid) || r.current_todo_uuid === uuid)
    .sort((a: RecurrenceRule, b: RecurrenceRule): number => b.updated_at - a.updated_at)[0] ?? null;
}
export function ruleEditable(rule: RecurrenceRule | null, uuid: string, series: string | null, legacy: string | null, completed: boolean): boolean {
  if (completed || legacy !== null) return false;
  return rule === null ? series === null : rule.current_todo_uuid === uuid && rule.deleted_at === null && !rule.exhausted;
}

// Match Harmony's task badge policy without changing historical lookup for rule editing.
export function visibleRecurrenceRule(rules: RecurrenceRule[], uuid: string, series: string | null, legacy: string | null): RecurrenceRule | null {
  if (legacy !== null) return null;
  const rule = associatedRule(rules, uuid, series);
  if (rule !== null && series === null && (rule.deleted_at !== null || rule.exhausted)) return null;
  return rule;
}
