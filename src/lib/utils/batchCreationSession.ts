import type { BatchRequest, BatchResult, BatchItem } from '../types/taskBatch';
import { type BatchTaskPreview, type BatchTaskRow, type CompositionIssueCode, parseBatchTaskText, batchPreviewToDrafts, titleIssue } from './taskComposition';

export interface BatchApi {
  create: (request: BatchRequest) => Promise<BatchResult>;
  load?: () => Promise<BatchRequest | null>;
  forget?: (request: BatchRequest) => Promise<void>;
}
export class BatchCreationSession {
  text: string = '';
  preview: BatchTaskPreview = { rows: [], error: null };
  group: string = '';
  reviewing: boolean = false;
  busy: boolean = false;
  done: boolean = false;
  ready: boolean = false;
  recovered: boolean = false;
  private confirmed: BatchResult | null = null;
  private api: BatchApi;
  private uuid: () => string;
  private pending: BatchRequest | null = null;
  private parsedText: string | null = null;
  constructor(api: BatchApi, uuid: () => string) { this.api = api; this.uuid = uuid; this.ready = api.load === undefined; }
  locked(): boolean { return !this.ready || this.busy || this.pending !== null || this.done; }
  creationConfirmed(): boolean { return this.confirmed !== null; }
  async initialize(): Promise<void> {
    if (this.ready || this.busy) return;
    this.busy = true;
    try {
      const request = this.api.load === undefined ? null : await this.api.load();
      if (request !== null) {
        if (!Array.isArray(request.items) || request.items.length === 0 || request.items.length > 50 ||
          typeof request.operation_uuid !== 'string' || !request.operation_uuid ||
          request.items.some((item: BatchItem): boolean => typeof item.title !== 'string' || titleIssue(item.title) !== null)) {
          throw new Error('BATCH_RECOVERY_INVALID');
        }
        this.pending = JSON.parse(JSON.stringify(request)) as BatchRequest;
        this.group = request.group_uuid ?? '';
        this.preview = { error: null, rows: request.items.map((item: BatchItem, i: number): BatchTaskRow => ({
          lineNumber: i + 1, title: item.title, selected: true, duplicate: false, issue: null
        })) };
        this.reviewing = true; this.recovered = true;
      }
      this.ready = true;
    } finally { this.busy = false; }
  }
  async discard(): Promise<void> {
    if (!this.ready || this.busy || this.done || this.pending === null) throw new Error('BATCH_LOCKED');
    this.busy = true;
    try {
      if (this.api.forget !== undefined) await this.api.forget(JSON.parse(JSON.stringify(this.pending)) as BatchRequest);
      this.pending = null; this.confirmed = null; this.done = true;
    } finally { this.busy = false; }
  }
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
    if (!this.ready || this.busy || this.done || !this.reviewing) throw new Error('BATCH_LOCKED');
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
      const result = this.confirmed ?? await this.api.create(request);
      if (result.operation_uuid !== request.operation_uuid || !Array.isArray(result.task_uuids) ||
        JSON.stringify(result.task_uuids) !== JSON.stringify(request.items.map((item: BatchItem): string => item.uuid)) ||
        !Number.isSafeInteger(result.created_at) || result.created_at < 0) throw new Error('BATCH_INVALID_RESPONSE');
      this.confirmed = result;
      if (this.api.forget !== undefined) await this.api.forget(request);
      this.done = true; return result;
    } catch (error) {
      if (this.confirmed !== null) throw new Error('BATCH_RECOVERY_CLEANUP');
      // These rejections happen before inserts; all other failures keep the exact request for retry.
      if (this.confirmed === null && /BATCH_GROUP_MISSING|INVALID_BATCH_REQUEST|BATCH_ORDER_OVERFLOW/.test(String(error))) {
        if (this.api.forget !== undefined) await this.api.forget(JSON.parse(JSON.stringify(this.pending)) as BatchRequest);
        this.pending = null; this.recovered = false;
      }
      throw error as Error;
    } finally { this.busy = false; }
  }
}
export function batchError(error: string): string {
  if (error.includes('RECOVERY_CLEANUP')) return 'cleanupFailed';
  if (error.includes('RECOVERY_')) return 'recoveryFailed';
  for (const code of ['INPUT_TOO_LONG', 'TOO_MANY_TASKS', 'NO_TASKS_SELECTED', 'EMPTY_TITLE', 'TITLE_TOO_LONG', 'INVALID_TITLE']) {
    if (error.includes(code)) return code;
  }
  if (error.includes('GROUP_MISSING')) return 'missingGroup';
  if (/STALE|IDENTITY_EXISTS|OPERATION_REUSED|INVALID_RECEIPT/.test(error)) return 'conflict';
  if (error.includes('ORDER_OVERFLOW')) return 'orderError';
  return 'failed';
}
