import type { CalendarCoverage, CalendarDocument, CalendarOccurrence } from '$lib/types/systemCalendar';

export const CALENDAR_REFRESH_MS = 5 * 60_000;
export const CALENDAR_SOURCE_STALE_MS = 24 * 60 * 60_000;

export function calendarDayBounds(date: string): [number, number] | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return null;
  const [year, month, day] = date.split('-').map(Number);
  const start = new Date(0);
  start.setFullYear(year, month - 1, day);
  start.setHours(0, 0, 0, 0);
  if (start.getFullYear() !== year || start.getMonth() !== month - 1 || start.getDate() !== day) return null;
  const end = new Date(start);
  // Calendar arithmetic, not 24-hour arithmetic: viewer days may cross DST.
  end.setDate(end.getDate() + 1);
  return [start.getTime(), end.getTime()];
}

export function occurrenceOnDate(item: CalendarOccurrence, date: string): boolean {
  const bounds = calendarDayBounds(date);
  if (!bounds) return false;
  if (item.isAllDay) return item.startDate <= date && date < item.endDateExclusive;
  const [start, end] = bounds;
  return item.startTime === item.endTime
    ? item.startTime >= start && item.startTime < end
    : item.startTime < end && item.endTime > start;
}

export function calendarOccurrencesOnDate(document: CalendarDocument | null, date: string) {
  if (document?.state !== 'active') return [];
  return document.occurrences.filter(item => occurrenceOnDate(item, date)).sort((a, b) =>
    Number(b.isAllDay) - Number(a.isAllDay) || a.startTime - b.startTime || a.id.localeCompare(b.id));
}

export function calendarCoverageOnDate(coverage: CalendarCoverage | null, date: string): 'full' | 'partial' | 'outside' {
  const bounds = calendarDayBounds(date);
  if (!coverage || !bounds) return 'outside';
  const [start, end] = bounds;
  const dateCovered = coverage.start <= date && date < coverage.end;
  const timeOverlaps = coverage.start_time < end && coverage.end_time > start;
  if (!dateCovered && !timeOverlaps) return 'outside';
  return dateCovered && coverage.start_time <= start && coverage.end_time >= end ? 'full' : 'partial';
}

export function calendarFreshness(document: CalendarDocument | null, receivedAt: number, now: number) {
  return {
    sourceStale: document?.state === 'active' && now - document.captured_at >= CALENDAR_SOURCE_STALE_MS,
    cacheStale: receivedAt <= 0 || now - receivedAt >= CALENDAR_REFRESH_MS,
  };
}

export function calendarOwnerLabel(owner: string): string {
  // Never render an arbitrary owner field or a full UUID as a device name.
  return /^[0-9a-f]{8}-[0-9a-f-]{27}$/i.test(owner) ? owner.slice(0, 8) : '';
}

export function calendarAllDayLastDate(endExclusive: string): string {
  // Subtract in UTC only for date-only values; the viewer timezone must not move an all-day span.
  if (!/^\d{4}-\d{2}-\d{2}$/.test(endExclusive)) return '';
  const end = new Date(`${endExclusive}T00:00:00Z`);
  if (!Number.isFinite(end.getTime()) || end.toISOString().slice(0, 10) !== endExclusive) return '';
  end.setUTCDate(end.getUTCDate() - 1);
  return end.toISOString().slice(0, 10);
}
