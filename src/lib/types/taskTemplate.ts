export interface TemplateContent {
  name: string; title: string; note: string; group_uuid: string | null; checklist: string[];
}
export interface TaskTemplate {
  uuid: string; content: TemplateContent; created_at: number; updated_at: number;
  updated_by: string; deleted_at: number | null;
}
export interface TemplatesDocument { format_version: number; templates: TaskTemplate[]; }
export interface TemplateWrite {
  operation_uuid: string; uuid: string; expected: TaskTemplate | null; content: TemplateContent; deleted: boolean;
}
