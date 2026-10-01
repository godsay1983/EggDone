import { get } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import { createProgressCounts } from './taskProgressCounts';
import type { ProgressCount } from '$lib/types/taskProgress';
vi.mock('$lib/sync/autoSync', () => ({ scheduleAutoSync: vi.fn() }));
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };
describe('shared visible progress counts', () => {
  it('coalesces 100 rows into one grouped query without N+1', async () => {
    const counts = vi.fn(async () => [{ task_uuid: 'a', count: 2 }]);
    const store = createProgressCounts({ list: vi.fn(), write: vi.fn(), dismissNotice: vi.fn(), counts });
    const rows = Array.from({ length: 100 }, (_, index) => store.register([index % 2 ? 'a' : 'b']));
    await settle(); expect(counts).toHaveBeenCalledExactlyOnceWith(['a', 'b']); expect(get(store)).toEqual({ a: 2, b: 0 });
    rows.forEach(row => row.dispose()); await settle(); expect(get(store)).toEqual({}); expect(counts).toHaveBeenCalledOnce();
  });
  it('rejects stale count replies and purges removed task caches', async () => {
    let resolve!: (rows: ProgressCount[]) => void;
    const counts = vi.fn().mockReturnValueOnce(new Promise<ProgressCount[]>(yes => resolve = yes)).mockResolvedValue([{ task_uuid: 'b', count: 4 }]);
    const store = createProgressCounts({ list: vi.fn(), write: vi.fn(), dismissNotice: vi.fn(), counts });
    const batch = store.register(['a']); await settle(); batch.update(['b']); await settle(); resolve([{ task_uuid: 'a', count: 7 }]); await settle();
    expect(get(store)).toEqual({ b: 4 }); batch.dispose(); await settle(); expect(get(store)).toEqual({});
  });
  it('coalesces refresh notifications and exposes no stale counts on read failure', async () => {
    const counts = vi.fn().mockResolvedValue([{ task_uuid: 'a', count: 3 }]);
    const store = createProgressCounts({ list: vi.fn(), write: vi.fn(), dismissNotice: vi.fn(), counts });
    const batch = store.register(['a']); await settle(); const refreshPanel = vi.fn(); const stop = store.onChanged(refreshPanel);
    counts.mockRejectedValueOnce('offline'); store.notify(); store.notify(); await settle();
    expect(counts).toHaveBeenCalledTimes(2); expect(get(store)).toEqual({}); expect(refreshPanel).toHaveBeenCalledTimes(2);
    stop(); batch.dispose(); await settle();
  });
});
