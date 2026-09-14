import type { ChecklistSave, ChecklistEdit, DefinitionEntry, DefinitionsDocument, ChecklistPanelSnapshot } from './taskChecklist';
import type { RecurrenceSchedule, RecurrenceRule } from './recurrence';

export interface RecurrenceDocument { format_version: number; rules: RecurrenceRule[]; }
export interface TaskEditorFields {
  due_date: string | null;
  due_at: number | null;
  reminder_at: number | null;
  group_uuid: string | null;
  priority: number;
  repeat_rule: string | null;
}
export interface ChecklistRuleSeed { uuid: string; schedule: RecurrenceSchedule; timezone_id: string | null; }
export interface ChecklistEditorRequest {
  task: ChecklistSave;
  fields: TaskEditorFields;
  expected_rules: RecurrenceDocument;
  mode: string;
  replaces_uuid: string | null;
  replacement: ChecklistRuleSeed | null;
  future_entries: DefinitionEntry[];
}
export interface ChecklistEditorResult { updated_at: number; rule_uuid: string | null; reminder_changed: boolean; }
export interface ChecklistEditorSnapshot {
  task: ChecklistPanelSnapshot;
  fields: TaskEditorFields;
  completed: boolean;
  repeat_series_uuid: string | null;
  rules: RecurrenceDocument;
  definitions: DefinitionsDocument;
}
export interface ChecklistEditorDraft {
  title: string;
  note: string;
  items: ChecklistEdit[];
  fields: TaskEditorFields;
  mode: 'keep' | 'replace' | 'stop';
  replaces_uuid: string | null;
  replacement: ChecklistRuleSeed | null;
  future_entries: DefinitionEntry[];
}
