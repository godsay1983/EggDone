import type { ChecklistEditorSnapshot, TaskEditorFields } from '../types/taskChecklistEditor';

// No persistence: the provisional identity remains stable across retries in this creation session.
export function newChecklistDraft(uuid: string, title: string, fields: TaskEditorFields): ChecklistEditorSnapshot {
  const copy = JSON.parse(JSON.stringify(fields)) as TaskEditorFields;
  copy.repeat_rule = null;
  return {
    task: { todo_uuid: uuid, title: title, note: '', updated_at: 0, read_only: false,
      items: { format_version: 1, items: [] } },
    fields: copy, completed: false, repeat_series_uuid: null,
    rules: { format_version: 1, rules: [] }, definitions: { format_version: 1, definitions: [] },
    next_occurrence_date: null
  };
}
