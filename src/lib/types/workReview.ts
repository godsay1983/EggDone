export type ReviewGroupScope = 'all' | 'ungrouped' | 'group';
export interface ReviewQuery {
  start_at: number;
  end_at: number;
  group_scope: ReviewGroupScope;
  group_uuid: string | null;
  keyword: string;
}
export interface ReviewRow {
  record_uuid: string;
  task_uuid: string;
  task_title: string;
  group_uuid: string | null;
  group_name: string | null;
  group_color: string | null;
  completed: boolean;
  archived: boolean;
  body: string;
  created_at: number;
  updated_at: number;
}
export interface ReviewCursor {
  created_at: number;
  record_uuid: string;
  query_key: string;
  snapshot_token: string;
}
export interface ReviewSnapshot {
  rows: ReviewRow[];
  matching_entry_count: number;
  matching_task_count: number;
  snapshot_token: string;
}
export interface ReviewPage extends ReviewSnapshot { next_cursor: ReviewCursor | null }
