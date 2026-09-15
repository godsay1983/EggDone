import type { TemplateContent, TaskTemplate, TemplatesDocument, TemplateWrite } from '../types/taskTemplate';
import type { ChecklistEditorSnapshot } from '../types/taskChecklistEditor';
import type { ChecklistEdit, ChecklistItem } from '../types/taskChecklist';
import type { TaskCopyDraft } from './taskCopyDraft';
import { taskTemplateToDraft, validateTaskCreationDraft, type ChecklistDraftItem } from './taskComposition';
import { newChecklistDraft } from './taskChecklistCreation';

export interface TemplateLibraryApi {
  list: () => Promise<TemplatesDocument>;
  save: (request: TemplateWrite) => Promise<TaskTemplate>;
}
export function templateFromTask(source: ChecklistEditorSnapshot): TemplateContent {
  return {
    name: source.task.title.slice(0, 60).replace(/[\uD800-\uDBFF]$/, '').trim(), title: source.task.title, note: source.task.note,
    group_uuid: source.fields.group_uuid,
    checklist: source.task.items.items.filter((item: ChecklistItem): boolean => item.deleted_at === null)
      .sort((a: ChecklistItem, b: ChecklistItem): number => a.sort_order - b.sort_order || (a.uuid < b.uuid ? -1 : 1))
      .map((item: ChecklistItem): string => item.content)
  };
}
export function templateCreation(content: TemplateContent, groupIds: string[], uuid: () => string): TaskCopyDraft {
  if (content.group_uuid !== null && !groupIds.includes(content.group_uuid)) throw new Error('TEMPLATE_GROUP_MISSING');
  const draft = taskTemplateToDraft({ name: content.name, title: content.title, note: content.note,
    groupUuid: content.group_uuid, checklist: content.checklist }, uuid());
  const issues = validateTaskCreationDraft(draft);
  if (issues.length > 0) throw new Error(issues[0].code);
  const creation = newChecklistDraft(draft.draftKey, draft.title, { due_date: null, due_at: null, reminder_at: null,
    group_uuid: draft.groupUuid, priority: 0, repeat_rule: null });
  creation.task.note = draft.note;
  return { creation: creation, items: draft.checklist.map((item: ChecklistDraftItem, index: number): ChecklistEdit =>
    ({ uuid: uuid(), content: item.content, completed: false, sort_order: (index + 1) * 1000 })) };
}
export function templateError(error: string): string {
  if (/STALE|OPERATION_REUSED/.test(error)) return 'conflict';
  if (/GROUP/.test(error)) return 'missingGroup';
  if (/LIMIT|TOO_MANY/.test(error)) return 'limit';
  if (/INVALID|EMPTY|TOO_LONG/.test(error)) return 'invalid';
  return 'failed';
}
// Each editor owns its baseline and identity. Uncertain retries reuse the full request.
export class TemplateLibrarySession {
  private api: TemplateLibraryApi;
  private uuid: () => string;
  private expected: TaskTemplate | null = null;
  private target: string = '';
  private request: TemplateWrite | null = null;
  private key: string = '';
  private busy: boolean = false;
  constructor(api: TemplateLibraryApi, uuid: () => string) { this.api = api; this.uuid = uuid; }
  async list(): Promise<TaskTemplate[]> {
    const doc = await this.api.list();
    return doc.templates.filter((row: TaskTemplate): boolean => row.deleted_at === null)
      .sort((a: TaskTemplate, b: TaskTemplate): number => b.updated_at - a.updated_at || (a.uuid < b.uuid ? -1 : 1));
  }
  select(row: TaskTemplate | null): void {
    if (this.busy) throw new Error('TEMPLATE_BUSY');
    this.expected = row === null ? null : JSON.parse(JSON.stringify(row)) as TaskTemplate;
    this.target = row === null ? this.uuid() : row.uuid; this.request = null; this.key = '';
  }
  async save(content: TemplateContent, deleted: boolean): Promise<TaskTemplate> {
    if (this.busy || this.target.length === 0) throw new Error('TEMPLATE_BUSY');
    const clean: TemplateContent = { name: content.name.trim(), title: content.title.trim(), note: content.note,
      group_uuid: content.group_uuid, checklist: content.checklist.map((item: string): string => item.trim()) };
    const key = JSON.stringify([clean, deleted]);
    if (this.request === null || this.key !== key) {
      this.key = key;
      this.request = { operation_uuid: this.uuid(), uuid: this.target, expected: this.expected, content: clean, deleted: deleted };
    }
    this.busy = true;
    try {
      const result = await this.api.save(JSON.parse(JSON.stringify(this.request)) as TemplateWrite);
      this.expected = JSON.parse(JSON.stringify(result)) as TaskTemplate;
      this.request = null; this.key = '';
      return result;
    } finally { this.busy = false; }
  }
}
