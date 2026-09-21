import { describe, expect, it } from 'vitest';
import { calendarDateKey, calendarPeriodCells, shiftCalendarPeriod } from './calendarView';

describe('calendar week/month navigation', () => {
  it.each([[2024, 1, 29], [2025, 1, 28], [2026, 3, 30], [2026, 0, 31]])(
    'renders all dates of %i/%i with six complete Sunday-based rows', (year, month, days) => {
      const cells = calendarPeriodCells(new Date(year, month, 15).getTime(), 'month');
      expect(cells).toHaveLength(42);
      expect(new Date(cells[0].dateAt).getDay()).toBe(0);
      expect(new Set(cells.map(c => c.dateKey)).size).toBe(42);
      expect(cells.filter(c => c.inMonth)).toHaveLength(days);
      expect(cells.filter(c => c.inMonth)[0].day).toBe(1);
      expect(cells.filter(c => c.inMonth).at(-1)?.day).toBe(days);
    });

  it('keeps the selected date in both modes, including a cross-year week', () => {
    const selected = new Date(2026, 11, 31).getTime();
    const week = calendarPeriodCells(selected, 'week');
    expect(week.map(c => c.dateKey)).toEqual([
      '2026-12-27', '2026-12-28', '2026-12-29', '2026-12-30', '2026-12-31', '2027-01-01', '2027-01-02'
    ]);
    expect(week.every(c => c.inMonth)).toBe(true);
    for (const mode of ['week', 'month'] as const) {
      expect(calendarPeriodCells(selected, mode).some(c => c.dateKey === '2026-12-31')).toBe(true);
    }
  });

  it('clamps month-end, handles leap years and crosses years', () => {
    expect(calendarDateKey(shiftCalendarPeriod(new Date(2024, 0, 31).getTime(), 'month', 1))).toBe('2024-02-29');
    expect(calendarDateKey(shiftCalendarPeriod(new Date(2025, 2, 31).getTime(), 'month', -1))).toBe('2025-02-28');
    expect(calendarDateKey(shiftCalendarPeriod(new Date(2026, 11, 31).getTime(), 'month', 1))).toBe('2027-01-31');
    expect(calendarDateKey(shiftCalendarPeriod(new Date(2026, 0, 1).getTime(), 'week', -1))).toBe('2025-12-25');
  });

  it('uses local calendar days through DST weeks', () => {
    for (const anchor of [new Date(2026, 2, 8), new Date(2026, 10, 1)]) {
      const cells = calendarPeriodCells(anchor.getTime(), 'week');
      for (let i = 0; i < cells.length; i++) {
        expect(new Date(cells[i].dateAt).getHours()).toBe(0);
        expect(new Date(cells[i].dateAt).getDate()).toBe(anchor.getDate() + i);
      }
      expect(new Date(shiftCalendarPeriod(anchor.getTime(), 'week', 1)).getDate()).toBe(anchor.getDate() + 7);
    }
  });
});
