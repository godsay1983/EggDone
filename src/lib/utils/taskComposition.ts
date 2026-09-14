export const TASK_TITLE_LIMIT: number = 100;
export const TASK_NOTE_LIMIT: number = 1000;
export const CHECKLIST_ITEM_LIMIT: number = 200;
export const CHECKLIST_LOCAL_LIMIT: number = 20;
export const TEMPLATE_NAME_LIMIT: number = 60;
export const BATCH_TASK_LIMIT: number = 50;
export const BATCH_TEXT_LIMIT: number = 20000;

export type CompositionIssueCode = 'EMPTY_TITLE' | 'TITLE_TOO_LONG' | 'INVALID_TITLE' |
  'NOTE_TOO_LONG' | 'INVALID_NOTE' | 'TOO_MANY_ITEMS' | 'EMPTY_ITEM' | 'ITEM_TOO_LONG' |
  'INVALID_ITEM' | 'INVALID_ITEM_KEY' | 'EMPTY_TEMPLATE_NAME' | 'TEMPLATE_NAME_TOO_LONG' |
  'INVALID_TEMPLATE_NAME' | 'INPUT_TOO_LONG' | 'TOO_MANY_TASKS' | 'NO_TASKS_SELECTED';

export interface CompositionIssue {
  code: CompositionIssueCode;
  index: number;
}

export interface ChecklistDraftItem {
  rowKey: string;
  content: string;
  completed: boolean;
}

export interface TaskCopyItem {
  content: string;
  completed: boolean;
}

export interface TaskCopySource {
  title: string;
  note: string;
  groupUuid: string | null;
  checklist: TaskCopyItem[];
}

export interface TaskCreationDraft {
  draftKey: string;
  title: string;
  note: string;
  groupUuid: string | null;
  checklist: ChecklistDraftItem[];
  completed: boolean;
  isPinned: boolean;
  priority: number;
  dueAt: number | null;
  reminderAt: number | null;
  repeatRule: string | null;
}

export interface TaskTemplateSnapshot {
  name: string;
  title: string;
  note: string;
  groupUuid: string | null;
  checklist: string[];
}

export interface BatchTaskRow {
  lineNumber: number;
  title: string;
  selected: boolean;
  duplicate: boolean;
  issue: CompositionIssueCode | null;
}

export interface BatchTaskPreview {
  rows: BatchTaskRow[];
  error: CompositionIssueCode | null;
}

function validKey(value: string): boolean {
  return /^[A-Za-z0-9:_-]{1,120}$/.test(value);
}

function validText(value: string, multiline: boolean): boolean {
  if (/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u202A-\u202E\u2066-\u2069]/.test(value)) return false;
  if (!multiline && /[\t\r\n\u2028\u2029]/.test(value)) return false;
  for (let i: number = 0; i < value.length; i++) {
    const unit: number = value.charCodeAt(i);
    if (unit >= 0xD800 && unit <= 0xDBFF) {
      if (i + 1 === value.length) return false;
      const next: number = value.charCodeAt(++i);
      if (next < 0xDC00 || next > 0xDFFF) return false;
    } else if (unit >= 0xDC00 && unit <= 0xDFFF) {
      return false;
    }
  }
  return true;
}

function titleIssue(title: string): CompositionIssueCode | null {
  if (title.trim().length === 0) return 'EMPTY_TITLE';
  if (title.trim().length > TASK_TITLE_LIMIT) return 'TITLE_TOO_LONG';
  if (!validText(title, false)) return 'INVALID_TITLE';
  return null;
}

export function validateTaskCreationDraft(draft: TaskCreationDraft): CompositionIssue[] {
  const issues: CompositionIssue[] = [];
  const titleError: CompositionIssueCode | null = titleIssue(draft.title);
  if (titleError !== null) issues.push({ code: titleError, index: -1 });
  if (draft.note.length > TASK_NOTE_LIMIT) issues.push({ code: 'NOTE_TOO_LONG', index: -1 });
  if (!validText(draft.note, true)) issues.push({ code: 'INVALID_NOTE', index: -1 });
  if (draft.checklist.length > CHECKLIST_LOCAL_LIMIT) issues.push({ code: 'TOO_MANY_ITEMS', index: -1 });
  const keys: Set<string> = new Set<string>();
  for (let i: number = 0; i < draft.checklist.length; i++) {
    const item: ChecklistDraftItem = draft.checklist[i];
    if (!validKey(item.rowKey) || keys.has(item.rowKey)) issues.push({ code: 'INVALID_ITEM_KEY', index: i });
    keys.add(item.rowKey);
    if (item.content.trim().length === 0) issues.push({ code: 'EMPTY_ITEM', index: i });
    if (item.content.trim().length > CHECKLIST_ITEM_LIMIT) issues.push({ code: 'ITEM_TOO_LONG', index: i });
    if (!validText(item.content, false)) issues.push({ code: 'INVALID_ITEM', index: i });
  }
  return issues;
}

// Copy only reusable content. Persistence IDs, scheduling, links and receipts never enter this draft.
export function copyTaskForCreation(source: TaskCopySource, draftKey: string): TaskCreationDraft {
  if (!validKey(draftKey) || draftKey.length > 100) throw new Error('INVALID_DRAFT_KEY');
  const items: ChecklistDraftItem[] = source.checklist.map((item: TaskCopyItem, index: number): ChecklistDraftItem => {
    return { rowKey: draftKey + ':item:' + index.toString(), content: item.content.trim(), completed: false };
  });
  return {
    draftKey: draftKey, title: source.title.trim(), note: source.note.replace(/\r\n?/g, '\n'),
    groupUuid: source.groupUuid, checklist: items,
    completed: false, isPinned: false, priority: 0, dueAt: null, reminderAt: null, repeatRule: null
  };
}

function templateNameIssue(name: string): CompositionIssueCode | null {
  if (name.trim().length === 0) return 'EMPTY_TEMPLATE_NAME';
  if (name.trim().length > TEMPLATE_NAME_LIMIT) return 'TEMPLATE_NAME_TOO_LONG';
  if (!validText(name, false)) return 'INVALID_TEMPLATE_NAME';
  return null;
}

export function makeTaskTemplateSnapshot(source: TaskCopySource, name: string): TaskTemplateSnapshot {
  const nameIssue: CompositionIssueCode | null = templateNameIssue(name);
  if (nameIssue !== null) throw new Error(nameIssue);
  const draft: TaskCreationDraft = copyTaskForCreation(source, 'template-preview');
  const issues: CompositionIssue[] = validateTaskCreationDraft(draft);
  if (issues.length > 0) throw new Error(issues[0].code);
  return {
    name: name.trim(), title: draft.title, note: draft.note, groupUuid: draft.groupUuid,
    checklist: draft.checklist.map((item: ChecklistDraftItem): string => item.content)
  };
}

export function taskTemplateToDraft(snapshot: TaskTemplateSnapshot, draftKey: string): TaskCreationDraft {
  const nameIssue: CompositionIssueCode | null = templateNameIssue(snapshot.name);
  if (nameIssue !== null) throw new Error(nameIssue);
  const source: TaskCopySource = {
    title: snapshot.title, note: snapshot.note, groupUuid: snapshot.groupUuid,
    checklist: snapshot.checklist.map((content: string): TaskCopyItem => {
      return { content: content, completed: false };
    })
  };
  return copyTaskForCreation(source, draftKey);
}

function stripListPrefix(line: string): string {
  return line.replace(/^(?:[-*+]\s+(?:\[[ xX]\]\s*)?|\d{1,3}[.)\u3001]\s+)/, '').trim();
}

export function parseBatchTaskText(text: string): BatchTaskPreview {
  if (text.length > BATCH_TEXT_LIMIT) return { rows: [], error: 'INPUT_TOO_LONG' };
  const lines: string[] = text.replace(/\r\n?|\u2028|\u2029/g, '\n').split('\n');
  const rows: BatchTaskRow[] = [];
  for (let i: number = 0; i < lines.length; i++) {
    if (lines[i].trim().length === 0) continue;
    if (rows.length === BATCH_TASK_LIMIT) return { rows: [], error: 'TOO_MANY_TASKS' };
    const title: string = stripListPrefix(lines[i].trimStart());
    rows.push({ lineNumber: i + 1, title: title, selected: true, duplicate: false, issue: titleIssue(title) });
  }
  for (const row of rows) {
    row.duplicate = row.title.length > 0 && rows.some((other: BatchTaskRow): boolean =>
      other.lineNumber !== row.lineNumber && other.title === row.title);
  }
  return { rows: rows, error: null };
}

// The commit layer must bind these stable draft keys to persisted UUIDs and an atomic operation receipt.
export function batchPreviewToDrafts(preview: BatchTaskPreview, requestKey: string,
  groupUuid: string | null): TaskCreationDraft[] {
  if (!validKey(requestKey) || requestKey.length > 80) throw new Error('INVALID_DRAFT_KEY');
  if (preview.error !== null) throw new Error(preview.error);
  if (preview.rows.length > BATCH_TASK_LIMIT) throw new Error('TOO_MANY_TASKS');
  const drafts: TaskCreationDraft[] = [];
  const lines: Set<number> = new Set<number>();
  for (const row of preview.rows) {
    if (!Number.isSafeInteger(row.lineNumber) || row.lineNumber < 1 || lines.has(row.lineNumber)) {
      throw new Error('INVALID_BATCH_ROWS');
    }
    lines.add(row.lineNumber);
    if (!row.selected) continue;
    const issue: CompositionIssueCode | null = titleIssue(row.title);
    if (issue !== null) throw new Error(issue);
    const source: TaskCopySource = { title: row.title, note: '', groupUuid: groupUuid, checklist: [] };
    drafts.push(copyTaskForCreation(source, requestKey + ':' + row.lineNumber.toString()));
  }
  if (drafts.length === 0) throw new Error('NO_TASKS_SELECTED');
  return drafts;
}
