export interface Todo {
  id: number;
  uuid: string;
  title: string;
  group_uuid: string | null;
  completed: boolean;
  pinned: boolean;
  sort_order: number;
  created_at: number;
  updated_at: number;
  completed_at: number | null;
  deleted_at: number | null;
  due_date: string | null;
  due_at: number | null;
  reminder_at: number | null;
}

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
