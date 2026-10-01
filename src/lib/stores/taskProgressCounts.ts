import { writable } from 'svelte/store';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { taskProgressApi } from '$lib/api/taskProgressApi';
import { scheduleAutoSync } from '$lib/sync/autoSync';
import type { ProgressPage } from '$lib/types/taskProgress';

export function createProgressCounts(api = taskProgressApi) {
  const counts = writable<Record<string, number>>({});
  const batches = new Map<symbol, string[]>();
  const subscribers = new Set<() => void>();
  let queued = false, generation = 0;
  function ids() { return [...new Set([...batches.values()].flat())].sort(); }
  function refresh() {
    ++generation;
    if (queued) return;
    queued = true;
    queueMicrotask(async () => {
      queued = false;
      const ticket = generation, visible = ids();
      if (!visible.length) { counts.set({}); return; }
      try {
        const rows = await api.counts(visible);
        const indexed = new Map(rows.map(row => [row.task_uuid, row.count]));
        if (ticket === generation) counts.set(Object.fromEntries(visible.map(id => [id, indexed.get(id) ?? 0])));
      } catch { if (ticket === generation) counts.set({}); }
    });
  }
  return { subscribe: counts.subscribe, refresh,
    register(taskUuids: string[]) {
      const key = Symbol(); batches.set(key, taskUuids); refresh();
      return { update(next: string[]) { if (JSON.stringify(next) !== JSON.stringify(batches.get(key))) { batches.set(key, next); refresh(); } },
        dispose() { batches.delete(key); refresh(); } };
    },
    onChanged(callback: () => void) { subscribers.add(callback); return () => subscribers.delete(callback); },
    notify() { refresh(); for (const callback of subscribers) callback(); },
    committed(page: ProgressPage) {
      ++generation;
      counts.update(s => ({ ...s, [page.task_uuid]: page.total }));
      for (const callback of subscribers) callback();
    },
  };
}
export const taskProgressCounts = createProgressCounts();
let consumers = 0, unlisteners: (() => void)[] = [], listenerGeneration = 0;
export function watchTaskProgress() {
  if (consumers++ === 0 && isTauri()) {
    const generation = ++listenerGeneration;
    for (const event of ['todos-changed', 'task-progress-changed']) {
      void listen(event, () => taskProgressCounts.notify()).then(stop => {
        if (!consumers || generation !== listenerGeneration) stop(); else unlisteners.push(stop);
      }).catch(() => { /* A later mounted consumer retries native registration. */ });
    }
  }
  return () => { if (--consumers === 0) { ++listenerGeneration; unlisteners.forEach(stop => stop()); unlisteners = []; } };
}
export function taskProgressCommitted(page: ProgressPage) {
  taskProgressCounts.committed(page);
  try { scheduleAutoSync(); } catch { /* The committed revision remains dirty. */ }
}
