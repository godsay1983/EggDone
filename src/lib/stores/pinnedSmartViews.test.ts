import { describe, it, expect } from 'vitest';
import { PinnedSmartViewStore, parsePinnedSmartViews } from './pinnedSmartViews';

function fixture(initial: string | null = null) {
  const disk = { value: initial, readFails: false, writeFails: false, writes: 0 };
  const repository = {
    async read() { if (disk.readFails) throw Error('read'); return disk.value; },
    async write(value: string) { if (disk.writeFails) throw Error('write'); disk.value = value; disk.writes++; },
  };
  return { disk, repository, store: new PinnedSmartViewStore(repository, () => {}) };
}

describe('pinned smart views', () => {
  it('validates the versioned contract without silently replacing unknown data', () => {
    expect(parsePinnedSmartViews(null)).toEqual([]);
    expect(parsePinnedSmartViews('{"version":1,"ids":["no_date","next7"]}')).toEqual(['no_date','next7']);
    for (const value of ['broken','null','[]','{}','{"version":2,"ids":[]}',
      '{"version":1,"ids":["unknown"]}','{"version":1,"ids":["no_date","no_date"]}',
      '{"version":1,"ids":["no_date","next7","important"]}','{"version":1,"ids":[7]}']) {
      expect(() => parsePinnedSmartViews(value)).toThrow();
    }
  });
  it('limits to two, preserves order on restart and does not select a filter', async () => {
    const { store, disk, repository } = fixture();
    await store.load(); await store.toggle('next7'); await store.toggle('no_date');
    await store.toggle('important'); expect(store.failure).toBe('limit'); expect(disk.writes).toBe(2);
    await store.toggle('next7'); await store.toggle('important');
    const restarted = new PinnedSmartViewStore(repository, () => {}); await restarted.load();
    expect(restarted.ids).toEqual(['no_date','important']);
    expect(disk.value).toBe('{"version":1,"ids":["no_date","important"]}');
  });
  it('blocks writes after unreadable load and recovers with retry', async () => {
    const { store, disk } = fixture('{"version":1,"ids":["overdue"]}'); disk.readFails = true;
    await store.load(); await store.toggle('next7'); expect(disk.writes).toBe(0); expect(store.failure).toBe('load');
    disk.readFails = false; await store.retry(); expect(store.ids).toEqual(['overdue']);
  });
  it('preserves confirmed pins on failed writes and retries the intended action', async () => {
    const { store, disk } = fixture(); await store.load(); disk.writeFails = true;
    await store.toggle('next7'); expect(store.ids).toEqual([]); expect(store.failure).toBe('save');
    disk.writeFails = false; await store.retry(); expect(store.ids).toEqual(['next7']);
    disk.writeFails = true; await store.toggle('next7'); expect(store.ids).toEqual(['next7']);
    disk.writeFails = false; await store.retry(); expect(store.ids).toEqual([]);
  });
  it('rechecks persistence and preserves external pins and corrupted data', async () => {
    const { store, disk } = fixture(); await store.load();
    disk.value = '{"version":1,"ids":["important"]}'; await store.toggle('next7');
    expect(store.ids).toEqual(['important','next7']);
    disk.value = 'broken'; await store.toggle('important'); expect(disk.value).toBe('broken');
    expect(store.ids).toEqual(['important','next7']); expect(store.failure).toBe('save');
  });
  it('suppresses double clicks and reload while a write is pending', async () => {
    let finish: () => void = () => {};
    const gate = new Promise<void>(resolve => { finish = resolve; });
    const { store, repository } = fixture(); await store.load();
    const write = repository.write; repository.write = async value => { await gate; await write(value); };
    const pending = store.toggle('next7'); await store.toggle('no_date'); await store.load();
    expect(store.busy).toBe(true); finish(); await pending; expect(store.ids).toEqual(['next7']);
  });
});
