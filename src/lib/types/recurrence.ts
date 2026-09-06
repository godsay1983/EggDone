export interface RecurrenceSchedule {
  anchor_date: string;
  frequency: string;
  interval: number;
  weekdays: number[];
  month_day: number | null;
  end_type: string;
  end_date: string | null;
  max_occurrences: number | null;
  local_time_minutes: number | null;
}
export interface RecurrenceRule {
  uuid: string;
  first_todo_uuid: string;
  schedule: RecurrenceSchedule;
  timezone_id: string | null;
  current_todo_uuid: string;
  current_date: string;
  generated_count: number;
  exhausted: boolean;
  updated_at: number;
  updated_by: string;
  deleted_at: number | null;
}
export interface RuleEditRequest {
  rule: RecurrenceRule;
  expected_todo_updated_at: number;
  replaces: RecurrenceRule | null;
}
