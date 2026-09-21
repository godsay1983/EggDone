export type CalendarViewMode = 'week' | 'month';

export interface CalendarDateCell {
  dateAt: number;
  dateKey: string;
  day: number;
  inMonth: boolean;
}

export function calendarDateKey(value: number): string {
  const date = new Date(value);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

export function calendarPeriodCells(anchorAt: number, mode: CalendarViewMode): CalendarDateCell[] {
  const anchor = new Date(anchorAt);
  const start = new Date(anchor.getFullYear(), anchor.getMonth(), mode === 'month' ? 1 : anchor.getDate());
  start.setDate(start.getDate() - start.getDay());
  const cells: CalendarDateCell[] = [];
  for (let index = 0; index < (mode === 'month' ? 42 : 7); index++) {
    // Calendar arithmetic keeps local dates stable across DST transitions.
    const date = new Date(start.getFullYear(), start.getMonth(), start.getDate() + index);
    cells.push({
      dateAt: date.getTime(),
      dateKey: calendarDateKey(date.getTime()),
      day: date.getDate(),
      inMonth: mode === 'week' || (date.getFullYear() === anchor.getFullYear() && date.getMonth() === anchor.getMonth())
    });
  }
  return cells;
}

export function shiftCalendarPeriod(anchorAt: number, mode: CalendarViewMode, offset: number): number {
  const anchor = new Date(anchorAt);
  if (mode === 'week') {
    return new Date(anchor.getFullYear(), anchor.getMonth(), anchor.getDate() + offset * 7).getTime();
  }
  const target = new Date(anchor.getFullYear(), anchor.getMonth() + offset, 1);
  const lastDay = new Date(target.getFullYear(), target.getMonth() + 1, 0).getDate();
  target.setDate(Math.min(anchor.getDate(), lastDay));
  return target.getTime();
}

