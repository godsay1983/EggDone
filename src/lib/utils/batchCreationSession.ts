import type { BatchRequest, BatchResult, BatchItem } from '../types/taskBatch';
import { type BatchTaskPreview, type BatchTaskRow, type CompositionIssueCode, parseBatchTaskText, batchPreviewToDrafts, titleIssue } from './taskComposition';

export interface BatchApi { create: (request: BatchRequest) => Promise<BatchResult>; }
export class BatchCreationSession {
  text: string = '';
  preview: BatchTaskPreview = { rows: [], error: null };
  group: string = '';
  reviewing: boolean = false;
  busy: boolean = false;
  done: boolean = false;
  private api: BatchApi;
  private uuid: () => string;
  private pending: BatchRequest | null = null;
  private parsedText: string | null = null;
  constructor(api: BatchApi, uuid: () => string) { this.api = api; this.uuid = uuid; }
  locked(): boolean { return this.busy || this.pending !== null || this.done; }
  private editable(): void { if (this.locked()) throw new Error('BATCH_LOCKED'); }
  input(text: string): void { this.editable(); this.text = text; this.reviewing = false; }
  review(): void {
    this.editable();
    if (this.parsedText === this.text && this.preview.rows.length > 0) { this.reviewing = true; return; }
    this.preview = parseBatchTaskText(this.text);
    if (this.preview.error !== null) throw new Error(this.preview.error);
    if (this.preview.rows.length === 0) throw new Error('NO_TASKS_SELECTED');
    this.parsedText = this.text; this.reviewing = true;
  }
  back(): void { this.editable(); this.reviewing = false; }
  selectGroup(group: string): void { this.editable(); this.group = group; }
  change(line: number, title: string, selected: boolean): void {
    this.editable();
    this.preview.rows = this.preview.rows.map((row: BatchTaskRow): BatchTaskRow => ({
      lineNumber: row.lineNumber, title: row.lineNumber === line ? title : row.title,
      selected: row.lineNumber === line ? selected : row.selected, duplicate: false,
      issue: titleIssue(row.lineNumber === line ? title : row.title)
    }));
    for (const row of this.preview.rows) {
      row.duplicate = row.title.trim().length > 0 && this.preview.rows.some((other: BatchTaskRow): boolean =>
        row.lineNumber !== other.lineNumber && row.title.trim() === other.title.trim());
    }
  }
  selectAll(selected: boolean): void {
    this.editable();
    this.preview.rows = this.preview.rows.map((row: BatchTaskRow): BatchTaskRow => ({
      lineNumber: row.lineNumber, title: row.title, selected: selected, duplicate: row.duplicate, issue: row.issue
    }));
  }
  count(): number { return this.preview.rows.filter((row: BatchTaskRow): boolean => row.selected).length; }
  issue(): CompositionIssueCode | null {
    if (this.preview.error !== null) return this.preview.error;
    if (this.count() === 0) return 'NO_TASKS_SELECTED';
    const bad = this.preview.rows.find((row: BatchTaskRow): boolean => row.selected && titleIssue(row.title) !== null);
    return bad === undefined ? null : titleIssue(bad.title);
  }
  async submit(groupIds: string[]): Promise<BatchResult> {
    if (this.busy || this.done || !this.reviewing) throw new Error('BATCH_LOCKED');
    if (this.pending === null) {
      if (this.group.length > 0 && !groupIds.includes(this.group)) throw new Error('BATCH_GROUP_MISSING');
      const operation = this.uuid();
      const drafts = batchPreviewToDrafts(this.preview, operation, this.group || null);
      const items: BatchItem[] = drafts.map((draft): BatchItem => ({ uuid: this.uuid(), title: draft.title }));
      this.pending = { operation_uuid: operation, group_uuid: this.group || null, items: items };
    }
    this.busy = true;
    try {
      const request = JSON.parse(JSON.stringify(this.pending)) as BatchRequest;
      const result = await this.api.create(request);
      if (result.operation_uuid !== request.operation_uuid || !Array.isArray(result.task_uuids) ||
        JSON.stringify(result.task_uuids) !== JSON.stringify(request.items.map((item: BatchItem): string => item.uuid)) ||
        !Number.isSafeInteger(result.created_at) || result.created_at < 0) throw new Error('BATCH_INVALID_RESPONSE');
      this.done = true; return result;
    } catch (error) {
      // These rejections happen before inserts; all other failures keep the exact request for retry.
      if (/BATCH_GROUP_MISSING|INVALID_BATCH_REQUEST|BATCH_ORDER_OVERFLOW/.test(String(error))) this.pending = null;
      throw error as Error;
    } finally { this.busy = false; }
  }
}
export function batchError(error: string): string {
  for (const code of ['INPUT_TOO_LONG', 'TOO_MANY_TASKS', 'NO_TASKS_SELECTED', 'EMPTY_TITLE', 'TITLE_TOO_LONG', 'INVALID_TITLE']) {
    if (error.includes(code)) return code;
  }
  if (error.includes('GROUP_MISSING')) return 'missingGroup';
  if (/STALE|IDENTITY_EXISTS|OPERATION_REUSED|INVALID_RECEIPT/.test(error)) return 'conflict';
  if (error.includes('ORDER_OVERFLOW')) return 'orderError';
  return 'failed';
}
