export interface ChecklistItem {
  uuid: string; todo_uuid: string; source_rule_uuid: string | null; source_entry_uuid: string | null;
  content: string; sort_order: number; completed: boolean;
  created_at: number; updated_at: number; updated_by: string; deleted_at: number | null;
}
export interface ItemsDocument { format_version: number; items: ChecklistItem[]; }
export interface ChecklistEdit { uuid: string; content: string; sort_order: number; completed: boolean; }
export interface ChecklistPanelSnapshot {
  todo_uuid: string; title: string; note: string; updated_at: number; read_only: boolean; items: ItemsDocument;
}
export interface ChecklistProgress { todo_uuid: string; total: number; completed: number; }
export interface ChecklistSave {
  operation_uuid: string; todo_uuid: string; expected_updated_at: number; expected_items: ItemsDocument;
  title: string; note: string; items: ChecklistEdit[];
}
