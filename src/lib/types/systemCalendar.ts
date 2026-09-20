export interface CalendarCoverage {
  start: string;
  end: string;
  start_time: number;
  end_time: number;
}

export interface CalendarSource { id: string; title: string }

export interface CalendarOccurrence {
  id: string;
  calendarId: string;
  title: string;
  startTime: number;
  endTime: number;
  isAllDay: boolean;
  startDate: string;
  endDateExclusive: string;
  timeZone: string;
  location: string;
}

export interface CalendarDocument {
  format_version: 1;
  owner_id: string;
  owner_generation: string;
  revision: number;
  operation_id: string;
  state: 'active' | 'withdrawn';
  captured_at: number;
  source_timezone: string;
  coverage: CalendarCoverage | null;
  calendars: CalendarSource[];
  occurrences: CalendarOccurrence[];
}

export interface SystemCalendarState {
  document: CalendarDocument | null;
  loading: boolean;
  error: string;
  last_received_at: number;
  configured: boolean;
}
