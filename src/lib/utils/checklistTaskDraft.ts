import type { ChecklistEditorSnapshot, ChecklistEditorDraft, TaskEditorFields } from '../types/taskChecklistEditor';
import type { ChecklistEdit, DefinitionEntry } from '../types/taskChecklist';
import type { RecurrenceRule, RecurrenceSchedule } from '../types/recurrence';
import { initialRuleForm, formSchedule, type RuleForm } from './recurrenceForm';
import { ChecklistScopeDraft } from './checklistScopeDraft';

export function editableChecklistRule(s: ChecklistEditorSnapshot): RecurrenceRule | null {
  const rules = s.rules.rules.filter((r: RecurrenceRule): boolean =>
    r.current_todo_uuid === s.task.todo_uuid && r.deleted_at === null && !r.exhausted);
  if (rules.length !== 1 || s.completed || s.task.read_only) return null;
  const r = rules[0];
  return (s.repeat_series_uuid === r.first_todo_uuid || (s.repeat_series_uuid === null && r.generated_count === 1)) ? r : null;
}
export function checklistCanChangeRepeat(s: ChecklistEditorSnapshot): boolean {
  if (s.fields.repeat_rule !== null && s.rules.rules.some((r: RecurrenceRule): boolean =>
    r.current_todo_uuid === s.task.todo_uuid && r.deleted_at === null && !r.exhausted)) return false;
  return !s.completed && !s.task.read_only && (editableChecklistRule(s) !== null ||
    (s.rules.rules.every((r: RecurrenceRule): boolean => r.current_todo_uuid !== s.task.todo_uuid || r.deleted_at !== null || r.exhausted) &&
      (s.repeat_series_uuid === null || s.fields.repeat_rule !== null)));
}
export function checklistLegacyFuture(s: ChecklistEditorSnapshot): boolean {
  return checklistCanChangeRepeat(s) && s.fields.repeat_rule !== null && (s.fields.due_at !== null || s.fields.due_date !== null);
}
export function checklistLocalDay(timestamp: number): string {
  const d = new Date(timestamp);
  return String(d.getFullYear()).padStart(4, '0') + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
}
export function checklistRuleForm(s: ChecklistEditorSnapshot, fields: TaskEditorFields, choice: string, zone: string): RuleForm {
  const r = choice === 'custom' ? editableChecklistRule(s) : null;
  const day = fields.due_date ?? checklistLocalDay(fields.due_at ?? Date.now());
  const form = initialRuleForm(day, r, zone);
  if (r !== null && r.schedule.max_occurrences !== null) form.count = String(r.schedule.max_occurrences - r.generated_count + 1);
  if (r === null) {
    form.start = day;
    form.frequency = choice === 'weekdays' ? 'weekly' : choice === 'custom' ? 'daily' : choice;
    if (choice === 'weekdays') form.weekdays = [1, 2, 3, 4, 5];
    form.allDay = fields.due_at === null;
    if (fields.due_at !== null) {
      const date = new Date(fields.due_at);
      form.time = String(date.getHours()).padStart(2, '0') + ':' + String(date.getMinutes()).padStart(2, '0');
    }
  }
  return form;
}
// The outer editor alone commits. Date resolution is delegated to each platform's existing recurrence engine.
export class ChecklistTaskDraft {
  private scope: ChecklistScopeDraft;
  private ruleUuid: string = '';
  private entryIds: Map<string, string> = new Map<string, string>();
  private uuid: () => string;
  constructor(uuid: () => string) { this.uuid = uuid; this.scope = new ChecklistScopeDraft(uuid); }
  async build(s: ChecklistEditorSnapshot, title: string, note: string, items: ChecklistEdit[],
    fields: TaskEditorFields, choice: string, form: RuleForm | null, future: boolean, zone: string,
    resolve: (schedule: RecurrenceSchedule, zone: string | null) => Promise<number | null>): Promise<ChecklistEditorDraft> {
    const f = JSON.parse(JSON.stringify(fields)) as TaskEditorFields;
    if (choice === 'keep' && f.repeat_rule !== null && f.due_at === null && f.due_date === null) throw new Error('CHECKLIST_DATE_REQUIRED');
    if (f.reminder_at !== null && f.reminder_at !== s.fields.reminder_at && f.reminder_at <= Date.now()) throw new Error('CHECKLIST_REMINDER_PAST');
    const legacyFuture = choice === 'keep' && future && checklistLegacyFuture(s);
    const draft = this.scope.build(s, title, note, items, choice === 'keep' && future && !legacyFuture);
    draft.fields = f;
    if (choice === 'keep' && !legacyFuture) return draft;
    if (!checklistCanChangeRepeat(s)) throw new Error('CHECKLIST_RULE_CONFLICT');
    const old = editableChecklistRule(s);
    f.repeat_rule = null;
    if (choice === 'none') {
      draft.mode = old !== null || s.fields.repeat_rule !== null ? 'stop' : 'keep';
      draft.replaces_uuid = old?.uuid ?? null;
      return draft;
    }
    if (items.length > 20) throw new Error('INVALID_CHECKLIST_EDITOR');
    const selection = legacyFuture ? s.fields.repeat_rule! : choice;
    if (selection !== 'custom' && f.due_at === null && f.due_date === null) throw new Error('CHECKLIST_DATE_REQUIRED');
    if (selection === 'custom' && form === null) throw new Error('CHECKLIST_CONFIGURE_RULE');
    const schedule = formSchedule(selection === 'custom' ? form! : checklistRuleForm(s, f, selection, zone));
    const timezone = schedule.local_time_minutes === null ? null : selection === 'custom' ? form!.zone.trim() : zone;
    f.due_at = await resolve(schedule, timezone);
    f.due_date = f.due_at === null ? schedule.anchor_date : null;
    if (this.ruleUuid.length === 0) this.ruleUuid = this.uuid();
    draft.mode = 'replace';
    draft.replaces_uuid = old?.uuid ?? null;
    draft.replacement = { uuid: this.ruleUuid, schedule: schedule, timezone_id: timezone };
    draft.future_entries = items.map((i: ChecklistEdit): DefinitionEntry => {
      let id = this.entryIds.get(i.uuid);
      if (id === undefined) { id = this.uuid(); this.entryIds.set(i.uuid, id); }
      return { uuid: id, content: i.content, sort_order: i.sort_order };
    });
    return draft;
  }
}
