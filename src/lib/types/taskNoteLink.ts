export type LinkScope = "todo" | "note";
export type LinkEntityState = "missing" | "deleted" | "archived" | "completed" | "active";
export interface TaskNoteLink {
  uuid: string;
  todo_uuid: string;
  note_uuid: string;
  created_at: number;
  updated_at: number;
  updated_by: string;
  deleted_at: number | null;
}
export interface TaskNoteLinkView {
  link: TaskNoteLink;
  todo_title: string | null;
  note_title: string | null;
  todo_state: LinkEntityState;
  note_state: LinkEntityState;
  is_repeating: boolean;
}
export interface LinkedTodoDraft {
  todo_uuid: string;
  note_uuid: string;
  title: string;
  note: string;
  group_uuid: string | null;
  due_date: string | null;
  due_at: number | null;
  reminder_at: number | null;
  priority: number;
}
