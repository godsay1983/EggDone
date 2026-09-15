import type { ChecklistPanelSnapshot, ChecklistSave, ChecklistItem, ChecklistEdit, ItemsDocument } from '../types/taskChecklist';

export interface ChecklistDetailGateway {
  read: (uuid: string) => Promise<ChecklistPanelSnapshot>;
  save: (request: ChecklistSave) => Promise<number>;
}

// Only persisted values enter this request; editor drafts never share this session.
export class TaskChecklistDetailSession {
  private api: ChecklistDetailGateway;
  private uuid: () => string;
  private baseline: ChecklistPanelSnapshot | null = null;
  private request: ChecklistSave | null = null;
  private busy: boolean = false;
  private committed: boolean = false;
  private generation: number = 0;

  constructor(api: ChecklistDetailGateway, uuid: () => string) {
    this.api = api; this.uuid = uuid;
  }
  hasPending(): boolean { return this.request !== null && !this.committed; }
  canWrite(): boolean {
    return this.baseline !== null && !this.baseline.read_only && !this.busy && !this.committed && this.request === null;
  }
  async load(todoUuid: string): Promise<ChecklistPanelSnapshot> {
    if (this.busy) throw new Error('CHECKLIST_DETAIL_BUSY');
    const ticket = ++this.generation;
    this.baseline = null; this.request = null; this.committed = false;
    const result = await this.api.read(todoUuid);
    if (ticket !== this.generation) throw new Error('CHECKLIST_DETAIL_LOAD_SUPERSEDED');
    if (result.todo_uuid !== todoUuid) throw new Error('CHECKLIST_PARENT_MISMATCH');
    this.baseline = JSON.parse(JSON.stringify(result)) as ChecklistPanelSnapshot;
    return result;
  }
  async setCompleted(itemUuid: string, completed: boolean): Promise<void> {
    if (!this.canWrite()) throw new Error('CHECKLIST_DETAIL_RELOAD_REQUIRED');
    const s = this.baseline!;
    const item = s.items.items.find((i: ChecklistItem): boolean => i.uuid === itemUuid && i.deleted_at === null);
    if (item === undefined || typeof completed !== 'boolean') throw new Error('CHECKLIST_ITEM_MISSING');
    if (item.completed === completed) return;
    this.request = {
      operation_uuid: this.uuid(), todo_uuid: s.todo_uuid, expected_updated_at: s.updated_at,
      expected_items: JSON.parse(JSON.stringify(s.items)) as ItemsDocument, title: s.title, note: s.note,
      items: s.items.items.filter((i: ChecklistItem): boolean => i.deleted_at === null)
        .map((i: ChecklistItem): ChecklistEdit => ({ uuid: i.uuid, content: i.content, sort_order: i.sort_order,
          completed: i.uuid === itemUuid ? completed : i.completed }))
    };
    await this.retry();
  }
  async retry(): Promise<void> {
    if (this.busy) throw new Error('CHECKLIST_DETAIL_BUSY');
    if (!this.hasPending()) throw new Error('CHECKLIST_DETAIL_RELOAD_REQUIRED');
    this.busy = true;
    try {
      await this.api.save(JSON.parse(JSON.stringify(this.request)) as ChecklistSave);
      // A receipt can describe an older commit: always reread before the next action.
      this.committed = true; this.request = null;
    } finally { this.busy = false; }
  }
}
