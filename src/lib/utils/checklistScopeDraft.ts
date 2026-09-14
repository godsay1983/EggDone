import type { ChecklistEditorDraft, ChecklistEditorSnapshot } from '../types/taskChecklistEditor';
import type { ChecklistEdit, DefinitionEntry } from '../types/taskChecklist';
import type { RecurrenceRule, RecurrenceSchedule } from '../types/recurrence';

export function checklistFutureRule(snapshot: ChecklistEditorSnapshot): RecurrenceRule | null {
  if (snapshot.task.read_only || snapshot.completed || snapshot.fields.repeat_rule !== null || snapshot.next_occurrence_date == null) return null;
  const active = snapshot.rules.rules.filter((rule: RecurrenceRule): boolean =>
    rule.current_todo_uuid === snapshot.task.todo_uuid && rule.deleted_at === null && !rule.exhausted);
  if (active.length !== 1) return null;
  const rule = active[0];
  if (snapshot.repeat_series_uuid !== null && snapshot.repeat_series_uuid !== rule.first_todo_uuid) return null;
  if (snapshot.repeat_series_uuid === null && rule.generated_count !== 1) return null;
  const count = rule.schedule.max_occurrences;
  if (count !== null && count - rule.generated_count < 1) return null;
  if (rule.schedule.end_date !== null && rule.schedule.end_date <= rule.current_date) return null;
  if (rule.schedule.local_time_minutes === null && snapshot.fields.due_date !== rule.current_date) return null;
  return rule;
}

// IDs live for the open draft, including failed submissions and toggling the scope back and forth.
export class ChecklistScopeDraft {
  private uuid: () => string;
  private ruleUuid: string = '';
  private entries: Map<string, string> = new Map<string, string>();
  constructor(uuid: () => string) { this.uuid = uuid; }

  build(snapshot: ChecklistEditorSnapshot, title: string, note: string,
    items: ChecklistEdit[], future: boolean): ChecklistEditorDraft {
    const draft: ChecklistEditorDraft = { title: title, note: note,
      items: JSON.parse(JSON.stringify(items)) as ChecklistEdit[],
      fields: JSON.parse(JSON.stringify(snapshot.fields)), mode: 'keep',
      replaces_uuid: null, replacement: null, future_entries: [] };
    if (!future) return draft;
    const rule = checklistFutureRule(snapshot);
    if (rule === null || items.length > 20) throw new Error('CHECKLIST_FUTURE_UNAVAILABLE');
    const schedule = JSON.parse(JSON.stringify(rule.schedule)) as RecurrenceSchedule;
    // Include the current occurrence in the new series; never restart the original total count.
    schedule.anchor_date = rule.current_date;
    if (schedule.max_occurrences !== null) schedule.max_occurrences -= rule.generated_count - 1;
    if (this.ruleUuid.length === 0) this.ruleUuid = this.uuid();
    draft.mode = 'replace';
    draft.replaces_uuid = rule.uuid;
    draft.replacement = { uuid: this.ruleUuid, schedule: schedule, timezone_id: rule.timezone_id };
    draft.future_entries = items.map((item: ChecklistEdit): DefinitionEntry => {
      let id = this.entries.get(item.uuid);
      if (id === undefined) { id = this.uuid(); this.entries.set(item.uuid, id); }
      return { uuid: id, content: item.content, sort_order: item.sort_order };
    });
    return draft;
  }
}
