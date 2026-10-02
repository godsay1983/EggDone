import { translate, type ResolvedLocale } from '$lib/i18n';
import type { ReviewQuery, ReviewRow, ReviewSnapshot } from '$lib/types/workReview';

export type ReviewPeriod = 'today' | 'week' | 'previousWeek' | 'custom';
export interface ReviewDates { start: string; end: string }
export const REVIEW_TEXT_LIMIT = 100000;

export function reviewDateKey(value: number | Date): string {
  const date = value instanceof Date ? value : new Date(value);
  return `${String(date.getFullYear()).padStart(4, '0')}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

export function reviewPeriodDates(period: Exclude<ReviewPeriod, 'custom'>, now = new Date()): ReviewDates {
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  if (period !== 'today') start.setDate(start.getDate() - (start.getDay() + 6) % 7 - (period === 'previousWeek' ? 7 : 0));
  const end = new Date(start);
  if (period !== 'today') end.setDate(end.getDate() + 6);
  return { start: reviewDateKey(start), end: reviewDateKey(end) };
}

function parseDay(value: string): Date | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return null;
  const [year, month, day] = value.split('-').map(Number);
  if (year < 100 || year > 9999) return null;
  const result = new Date(year, month - 1, day);
  return reviewDateKey(result) === value ? result : null;
}

export function reviewDateRange(dates: ReviewDates): { start_at: number; end_at: number } | null {
  const start = parseDay(dates.start), end = parseDay(dates.end);
  if (!start || !end || start > end || start.getTime() < 0) return null;
  // Advancing the local calendar preserves 23/25-hour DST days.
  end.setDate(end.getDate() + 1);
  return { start_at: start.getTime(), end_at: end.getTime() };
}

export function normalizeReviewQuery(query: ReviewQuery): ReviewQuery {
  return { ...query, group_uuid: query.group_scope === 'group' ? query.group_uuid : null,
    keyword: query.keyword.replace(/^[\s\u0085]+|[\s\u0085]+$/g, '').replace(/[A-Z]/g, character => character.toLowerCase()) };
}

export function compareReviewRows(a: ReviewRow, b: ReviewRow): number {
  return b.created_at - a.created_at || (a.record_uuid < b.record_uuid ? 1 : a.record_uuid > b.record_uuid ? -1 : 0);
}

export interface ReviewSummaryOptions { dates: ReviewDates; groupName: string; locale: ResolvedLocale; timeZone?: string }
export function formatReviewSummary(snapshot: ReviewSnapshot, options: ReviewSummaryOptions): string {
  const t = (key: Parameters<typeof translate>[1], params = {}) => translate(options.locale, key, params);
  const lines = [t('workReview.summaryTitle', options.dates), t('workReview.summaryGroup', { group: options.groupName }),
    t('workReview.summaryCounts', { tasks: snapshot.matching_task_count, entries: snapshot.matching_entry_count })];
  if (snapshot.rows.length && options.dates.start > options.dates.end) throw Error('REVIEW_INVALID');
  const days = new Map<string, Map<string, ReviewRow[]>>();
  const calendar = new Intl.DateTimeFormat('en-US', { timeZone: options.timeZone, year: 'numeric', month: '2-digit', day: '2-digit' });
  for (const row of [...snapshot.rows].sort(compareReviewRows)) {
    const parts = calendar.formatToParts(row.created_at);
    const part = (name: string) => parts.find(item => item.type === name)!.value;
    const key = `${part('year')}-${part('month')}-${part('day')}`;
    if (!days.has(key)) days.set(key, new Map());
    const tasks = days.get(key)!;
    if (!tasks.has(row.task_uuid)) tasks.set(row.task_uuid, []);
    tasks.get(row.task_uuid)!.push(row);
  }
  const clock = new Intl.DateTimeFormat(options.locale, { timeZone: options.timeZone, hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });
  for (const day of [...days.keys()].sort().reverse()) {
    lines.push('', day);
    const tasks = [...days.get(day)!.values()].sort((a, b) => compareReviewRows(a[0], b[0]));
    tasks.forEach((rows, index) => {
      if (index) lines.push('');
      lines.push(rows[0].task_title);
      for (const row of rows) lines.push(`- ${clock.format(row.created_at)} ${row.body.replace(/\r\n?/g, '\n').replace(/\n/g, '\n  ')}`);
    });
  }
  const text = lines.join('\n');
  if (text.length > REVIEW_TEXT_LIMIT) throw Error('REVIEW_LIMIT');
  return text;
}
