import { afterEach, describe, expect, it, vi } from 'vitest';
import active from '../../../tests/fixtures/system-calendar-v1-active.json';
import withdrawn from '../../../tests/fixtures/system-calendar-v1-withdrawn.json';
import type { CalendarDocument, CalendarOccurrence } from '$lib/types/systemCalendar';
import { calendarDayBounds, calendarOccurrencesOnDate, calendarCoverageOnDate, occurrenceOnDate,
  calendarFreshness, calendarOwnerLabel, calendarAllDayLastDate, CALENDAR_REFRESH_MS, CALENDAR_SOURCE_STALE_MS } from './systemCalendarDates';

const document = active as CalendarDocument;
const timed = (start: number, end = start): CalendarOccurrence => ({ ...document.occurrences[0], startTime: start, endTime: end });
afterEach(() => vi.unstubAllEnvs());

describe('system calendar production date projection', () => {
  it('converts an exclusive all-day end to the last included date across month/year/leap boundaries', () => {
    expect(calendarAllDayLastDate(document.occurrences[1].endDateExclusive)).toBe('2026-09-21');
    expect(calendarAllDayLastDate('2027-01-01')).toBe('2026-12-31');
    expect(calendarAllDayLastDate('2024-03-01')).toBe('2024-02-29');
    expect(calendarAllDayLastDate('2026-02-30')).toBe('');
  });
  it('consumes the shared golden active/withdrawn documents without changing date-only days', () => {
    const allDay = document.occurrences[1];
    for (const zone of ['UTC', 'America/New_York', 'Asia/Shanghai']) {
      vi.stubEnv('TZ', zone);
      expect(occurrenceOnDate(allDay, '2026-09-19')).toBe(false);
      expect(occurrenceOnDate(allDay, '2026-09-20')).toBe(true);
      expect(occurrenceOnDate(allDay, '2026-09-21')).toBe(true);
      expect(occurrenceOnDate(allDay, '2026-09-22')).toBe(false);
    }
    expect(calendarOccurrencesOnDate(withdrawn as CalendarDocument, '2026-09-20')).toEqual([]);
  });

  it('uses viewer time, exclusive ends, multi-day intersections and zero-duration events', () => {
    const [start, end] = calendarDayBounds('2026-09-20')!;
    expect(occurrenceOnDate(timed(start), '2026-09-20')).toBe(true);
    expect(occurrenceOnDate(timed(end), '2026-09-20')).toBe(false);
    expect(occurrenceOnDate(timed(start - 1000, start), '2026-09-20')).toBe(false);
    expect(occurrenceOnDate(timed(end, end + 1000), '2026-09-20')).toBe(false);
    expect(occurrenceOnDate(timed(start - 1000, end + 1000), '2026-09-20')).toBe(true);
    expect(occurrenceOnDate(timed(start - 1000, start + 1000), '2026-09-20')).toBe(true);
  });

  it('uses local midnight arithmetic across DST and rejects malformed selected dates', () => {
    for (const date of ['2026-03-08', '2026-11-01']) {
      const [start, end] = calendarDayBounds(date)!;
      const expected = new Date(start);
      expected.setDate(expected.getDate() + 1);
      expect(end).toBe(expected.getTime());
      expect(new Date(end).getHours()).toBe(0);
    }
    for (const date of ['', '2026-02-30', '2026-13-01', '2026-9-1']) expect(calendarDayBounds(date)).toBeNull();
  });

  it('distinguishes full, partial and absent coverage in the viewer timezone', () => {
    const [start, end] = calendarDayBounds('2026-09-20')!;
    const coverage = { start: '2026-09-20', end: '2026-09-21', start_time: start, end_time: end };
    expect(calendarCoverageOnDate(coverage, '2026-09-20')).toBe('full');
    expect(calendarCoverageOnDate({ ...coverage, start_time: start + 1 }, '2026-09-20')).toBe('partial');
    expect(calendarCoverageOnDate(coverage, '2026-09-21')).toBe('outside');
    expect(calendarCoverageOnDate(null, '2026-09-20')).toBe('outside');
  });

  it('separates source age from cache receive age and only abbreviates valid owners', () => {
    const now = document.captured_at + CALENDAR_SOURCE_STALE_MS;
    expect(calendarFreshness(document, now, now)).toEqual({ sourceStale: true, cacheStale: false });
    expect(calendarFreshness(document, now - CALENDAR_REFRESH_MS, now).cacheStale).toBe(true);
    expect(calendarOwnerLabel(document.owner_id)).toBe('11111111');
    expect(calendarOwnerLabel('<script>')).toBe('');
  });
});
