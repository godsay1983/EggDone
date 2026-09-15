import { describe, it, expect } from 'vitest';
import { newChecklistDraft } from './taskChecklistCreation';
import { TaskChecklistEditorSession } from './taskChecklistEditorSession';
import type { ChecklistEditorDraft, ChecklistEditorRequest, TaskEditorFields } from '../types/taskChecklistEditor';

const fields: TaskEditorFields = { due_date: null, due_at: null, reminder_at: null, group_uuid: null, priority: 0, repeat_rule: null };
describe('checklist creation draft', () => {
  it('has no existing parent/rules and isolates the supplied settings', () => {
    const f = { ...fields, repeat_rule: 'daily', priority: 1 };
    const s = newChecklistDraft('parent', 'Title', f);
    expect(s.task.updated_at).toBe(0);
    expect(s.task.items.items).toEqual([]);
    expect(s.rules.rules).toEqual([]);
    expect(s.fields.repeat_rule).toBeNull();
    s.fields.priority = 0;
    expect(f.priority).toBe(1);
  });
  it('opening and cancelling perform no creation writes', async () => {
    let writes = 0;
    const s = newChecklistDraft('parent', '', fields);
    const session = new TaskChecklistEditorSession({ read: async () => s, save: async () => {
      writes++; return { updated_at: 1, rule_uuid: null, reminder_changed: false };
    } }, () => 'operation');
    await session.load('parent');
    expect(writes).toBe(0);
  });
  it('reuses the same complete request after a lost response and blocks double save', async () => {
    const s = newChecklistDraft('parent', 'Task', fields);
    const calls: ChecklistEditorRequest[] = [];
    const session = new TaskChecklistEditorSession({ read: async () => s, save: async r => {
      calls.push(r);
      if (calls.length === 1) throw Error('reply lost');
      return { updated_at: 1, rule_uuid: null, reminder_changed: false };
    } }, () => crypto.randomUUID());
    await session.load('parent');
    const draft: ChecklistEditorDraft = { title: 'Task', note: '', fields: { ...fields }, items: [],
      mode: 'keep', replaces_uuid: null, replacement: null, future_entries: [] };
    await expect(session.save(draft)).rejects.toThrow('reply lost');
    await session.save(draft);
    expect(calls[1]).toEqual(calls[0]);
    expect(calls[1].task.expected_updated_at).toBe(0);
    await expect(session.save(draft)).rejects.toThrow('RELOAD_REQUIRED');
    expect(calls.length).toBe(2);
  });
});
