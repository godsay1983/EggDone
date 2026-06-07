export interface Todo {
  id: number;
  uuid: string;
  title: string;
  completed: boolean;
  sort_order: number;
  created_at: number;
  updated_at: number;
  completed_at: number | null;
  deleted_at: number | null;
}
