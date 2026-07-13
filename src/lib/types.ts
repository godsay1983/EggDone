export interface Todo {
  id: number;
  uuid: string;
  title: string;
  note: string | null;
  group_uuid: string | null;
  completed: boolean;
  pinned: boolean;
  priority: number;
  sort_order: number;
  created_at: number;
  updated_at: number;
  completed_at: number | null;
  deleted_at: number | null;
  archived_at: number | null;
  due_date: string | null;
  due_at: number | null;
  reminder_at: number | null;
  repeat_rule: RepeatRule | null;
  repeat_next_due_date: string | null;
  repeat_series_uuid: string | null;
}

export type RepeatRule = "daily" | "weekly" | "monthly" | "weekdays";
export type RepeatDeleteScope = "single" | "series";
export type RepeatEditScope = "single" | "future";

export interface TodoGroup {
  id: number;
  uuid: string;
  name: string;
  color: string;
  sort_order: number;
  created_at: number;
  updated_at: number;
  deleted_at: number | null;
}

export type NoteColor = "default" | "yellow" | "pink" | "green" | "blue";

export interface Note {
  id: number;
  uuid: string;
  title: string;
  content: string;
  color: NoteColor;
  pinned: boolean;
  created_at: number;
  updated_at: number;
  deleted_at: number | null;
  updated_by: string;
}
