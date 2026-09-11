import { normalizeSmartView, type SmartViewId } from '../utils/smartViews';

export interface PinnedSmartViews {
  version: number;
  ids: SmartViewId[];
}

export interface PinnedSmartViewRepository {
  read(): Promise<string | null>;
  write(value: string): Promise<void>;
}

export function parsePinnedSmartViews(raw: string | null): SmartViewId[] {
  if (raw === null) return [];
  const value = JSON.parse(raw) as PinnedSmartViews;
  if (value === null || typeof value !== 'object' || value.version !== 1 ||
    !Array.isArray(value.ids) || value.ids.length > 2) throw new Error('Invalid pinned views');
  const ids: SmartViewId[] = [];
  for (const id of value.ids) {
    if (normalizeSmartView(id) === null || ids.includes(id)) throw new Error('Invalid pinned view');
    ids.push(id);
  }
  return ids;
}

export class PinnedSmartViewStore {
  ids: SmartViewId[] = [];
  ready: boolean = false;
  busy: boolean = false;
  failure: string = '';
  private pending: SmartViewId | null = null;
  private repository: PinnedSmartViewRepository;
  private changed: () => void;

  constructor(repository: PinnedSmartViewRepository, changed: () => void) {
    this.repository = repository;
    this.changed = changed;
  }

  async load(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    this.changed();
    try {
      this.ids = parsePinnedSmartViews(await this.repository.read());
      this.ready = true;
      if (this.failure !== 'save') this.failure = '';
    } catch (_) {
      this.ready = false;
      this.failure = 'load';
    } finally {
      this.busy = false;
      this.changed();
    }
  }

  async toggle(id: SmartViewId): Promise<void> {
    if (!this.ready || this.busy || normalizeSmartView(id) === null) return;
    this.busy = true;
    this.changed();
    try {
      // Re-read before patching so stale local state cannot replace unreadable data.
      const current = parsePinnedSmartViews(await this.repository.read());
      const exists = current.includes(id);
      if (!exists && current.length >= 2) {
        this.ids = current;
        this.failure = 'limit';
        this.pending = null;
        return;
      }
      const next = exists ? current.filter((item: SmartViewId): boolean => item !== id) : current.concat(id);
      const value: PinnedSmartViews = { version: 1, ids: next };
      await this.repository.write(JSON.stringify(value));
      this.ids = next;
      this.pending = null;
      this.failure = '';
    } catch (_) {
      this.pending = id;
      this.failure = 'save';
    } finally {
      this.busy = false;
      this.changed();
    }
  }

  async retry(): Promise<void> {
    if (this.busy) return;
    if (this.failure === 'save' && this.pending !== null) await this.toggle(this.pending);
    else await this.load();
  }
}

