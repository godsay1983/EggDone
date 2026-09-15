import type { ChecklistEditorSnapshot } from '../types/taskChecklistEditor';
import type { ChecklistEdit, ChecklistItem } from '../types/taskChecklist';
import { copyTaskForCreation, validateTaskCreationDraft, type TaskCopyItem, type ChecklistDraftItem } from './taskComposition';
import { newChecklistDraft } from './taskChecklistCreation';

export interface TaskCopyDraft {
  creation: ChecklistEditorSnapshot;
  items: ChecklistEdit[];
}

// Initial rows belong to the UI draft, never to the empty creation conflict baseline.
export function taskCopyDraft(source: ChecklistEditorSnapshot, groupIds: string[], uuid: () => string): TaskCopyDraft {
  const id = uuid();
  const active = source.task.items.items.filter((item: ChecklistItem): boolean => item.deleted_at === null)
    .sort((a: ChecklistItem, b: ChecklistItem): number => a.sort_order - b.sort_order || (a.uuid < b.uuid ? -1 : a.uuid > b.uuid ? 1 : 0));
  const draft = copyTaskForCreation({
    title: source.task.title, note: source.task.note, groupUuid: source.fields.group_uuid,
    checklist: active.map((item: ChecklistItem): TaskCopyItem => ({ content: item.content, completed: item.completed }))
  }, id);
  const issues = validateTaskCreationDraft(draft);
  if (issues.length > 0) throw new Error(issues[0].code);
  const creation = newChecklistDraft(id, draft.title, {
    due_date: null, due_at: null, reminder_at: null, priority: 0, repeat_rule: null,
    group_uuid: draft.groupUuid !== null && groupIds.includes(draft.groupUuid) ? draft.groupUuid : null
  });
  creation.task.note = draft.note;
  return { creation: creation, items: draft.checklist.map((item: ChecklistDraftItem, index: number): ChecklistEdit =>
    ({ uuid: uuid(), content: item.content, completed: false, sort_order: (index + 1) * 1000 })) };
}
