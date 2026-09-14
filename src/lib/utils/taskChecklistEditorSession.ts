import type { ChecklistEditorDraft, ChecklistEditorRequest, ChecklistEditorResult, ChecklistEditorSnapshot } from '../types/taskChecklistEditor';

export interface ChecklistEditorGateway {
  read: (uuid: string) => Promise<ChecklistEditorSnapshot>;
  save: (request: ChecklistEditorRequest) => Promise<ChecklistEditorResult>;
}

// The UI owns the visible draft; this session owns immutable conflict/retry baselines.
export class TaskChecklistEditorSession {
  private api: ChecklistEditorGateway;
  private uuid: () => string;
  private baseline: ChecklistEditorSnapshot | null = null;
  private request: ChecklistEditorRequest | null = null;
  private fingerprint: string = '';
  private generation: number = 0;
  private busy: boolean = false;
  private committed: boolean = false;

  constructor(api: ChecklistEditorGateway, uuid: () => string) {
    this.api = api;
    this.uuid = uuid;
  }

  async load(todoUuid: string): Promise<ChecklistEditorSnapshot> {
    if (this.busy) throw new Error('CHECKLIST_EDITOR_BUSY');
    const ticket = ++this.generation;
    this.baseline = null;
    this.request = null;
    this.fingerprint = '';
    this.committed = false;
    const result = await this.api.read(todoUuid);
    if (ticket !== this.generation) throw new Error('CHECKLIST_EDITOR_LOAD_SUPERSEDED');
    if (result.task.todo_uuid !== todoUuid) throw new Error('CHECKLIST_PARENT_MISMATCH');
    this.baseline = JSON.parse(JSON.stringify(result)) as ChecklistEditorSnapshot;
    return result;
  }

  async save(draft: ChecklistEditorDraft): Promise<ChecklistEditorResult> {
    if (this.busy) throw new Error('CHECKLIST_EDITOR_BUSY');
    if (this.committed) throw new Error('CHECKLIST_EDITOR_RELOAD_REQUIRED');
    const baseline = this.baseline;
    if (baseline === null) throw new Error('CHECKLIST_EDITOR_NOT_LOADED');
    if (baseline.task.read_only) throw new Error('CHECKLIST_PARENT_READ_ONLY');
    if (baseline.completed && draft.mode !== 'keep') throw new Error('CHECKLIST_RULE_CONFLICT');
    const copy = JSON.parse(JSON.stringify(draft)) as ChecklistEditorDraft;
    copy.title = copy.title.trim();
    const fingerprint = JSON.stringify(copy);
    if (this.request === null || this.fingerprint !== fingerprint) {
      this.request = {
        task: { operation_uuid: this.uuid(), todo_uuid: baseline.task.todo_uuid, expected_updated_at: baseline.task.updated_at,
          expected_items: JSON.parse(JSON.stringify(baseline.task.items)), title: copy.title, note: copy.note, items: copy.items },
        fields: copy.fields, expected_rules: JSON.parse(JSON.stringify(baseline.rules)),
        mode: copy.mode, replaces_uuid: copy.replaces_uuid, replacement: copy.replacement, future_entries: copy.future_entries
      };
      this.fingerprint = fingerprint;
    }
    this.busy = true;
    try {
      const result = await this.api.save(JSON.parse(JSON.stringify(this.request)) as ChecklistEditorRequest);
      // Reload even after an idempotent replay; its receipt may describe an older commit.
      this.committed = true;
      return result;
    } finally {
      this.busy = false;
    }
  }
}
