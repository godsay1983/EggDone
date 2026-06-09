export interface Todo {
  id: number;
  uuid: string;
  title: string;
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
