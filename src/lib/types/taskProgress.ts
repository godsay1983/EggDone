export interface ProgressEntry {
  uuid: string;
  task_uuid: string;
  body: string;
  created_at: number;
  created_by: string;
  updated_at: number;
  updated_by: string;
  clock: number;
  deleted_at: number | null;
}
export interface ProgressCursor { created_at: number; uuid: string }
export interface ProgressView { record: ProgressEntry; token: string }
export interface ProgressPage {
  task_uuid: string;
  title: string;
  read_only: boolean;
  total: number;
  entries: ProgressView[];
  next_cursor: ProgressCursor | null;
  overwritten: boolean;
}
export interface ProgressCount { task_uuid: string; count: number }
export interface ProgressWrite {
  operation_uuid: string;
  task_uuid: string;
  record_uuid: string;
  action: 'create' | 'edit' | 'delete';
  body: string;
  expected_record: string | null;
}
