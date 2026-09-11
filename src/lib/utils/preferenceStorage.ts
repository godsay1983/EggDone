import { writable } from 'svelte/store';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { readGeneralPreferences, migrateGeneralPreferences, patchGeneralPreference, type GeneralPreferences } from '../api/generalPreferencesApi';

export const GENERAL_PREFERENCE_KEYS = [
  'eggdone-theme', 'eggdone-language', 'eggdone-show-completed',
  'eggdone-default-list-view', 'eggdone-list-view', 'eggdone-selected-group',
  'eggdone-smart-view', 'eggdone-pinned-smart-views', 'eggdone-focus-duration-minutes', 'eggdone-break-duration-minutes',
];
export const PREFERENCES_CHANGED_EVENT = 'eggdone-preferences-changed';
let native: GeneralPreferences | null = null;
let initialization: Promise<boolean> | null = null;
let listening: Promise<unknown> | null = null;
let writes: Promise<unknown> = Promise.resolve();

function accept(snapshot: GeneralPreferences) {
  if (snapshot.version !== 1 || !Number.isSafeInteger(snapshot.revision) || !snapshot.values) {
    throw Error('Unsupported preferences');
  }
  if (native && snapshot.revision < native.revision) return;
  native = snapshot;
  if (typeof window !== 'undefined') window.dispatchEvent(new Event(PREFERENCES_CHANGED_EVENT));
}

export function initializePreferences(): Promise<boolean> {
  if (!isTauri()) return Promise.resolve(true);
  if (initialization) return initialization;
  initialization = (async () => {
    try {
      listening ??= listen<GeneralPreferences>('general-preferences-changed', event => {
        try { accept(event.payload); } catch { report('native', 'read'); }
      }).catch(() => { listening = null; });
      await listening;
      let saved = await readGeneralPreferences();
      if (saved === null) {
        // Read every legacy key successfully before attempting a one-time migration.
        const values: Record<string, string | null> = {};
        for (const key of GENERAL_PREFERENCE_KEYS) values[key] = localStorage.getItem(key);
        saved = await migrateGeneralPreferences(values);
      }
      accept(saved);
      clear('native', false);
      return true;
    } catch {
      report('native', 'read');
      return false;
    } finally {
      initialization = null;
    }
  })();
  return initialization;
}

type StorageFailure = 'read' | 'write';
const failures = new Map<string, StorageFailure>();
const lastRead = new Map<string, string | null>();
export const preferenceStorageFailures = writable<StorageFailure[]>([]);

function report(key: string, failure: StorageFailure) {
  if (failure === 'write' || failures.get(key) !== 'write') failures.set(key, failure);
  preferenceStorageFailures.set([...new Set(failures.values())]);
}

function clear(key: string, reading: boolean) {
  if (!reading || failures.get(key) === 'read') failures.delete(key);
  preferenceStorageFailures.set([...new Set(failures.values())]);
}

// A denied WebView store must not abort mounting the task list or erase known values.
export function readPreference(key: string): string | null {
  if (isTauri() && GENERAL_PREFERENCE_KEYS.includes(key)) {
    return native?.values[key] ?? null;
  }
  try {
    if (typeof localStorage === 'undefined') return null;
    const value = localStorage.getItem(key);
    lastRead.set(key, value);
    clear(key, true);
    return value;
  } catch {
    report(key, 'read');
    return lastRead.get(key) ?? null;
  }
}

// Confirmed edits must distinguish an unreadable value from a genuinely absent preference.
export function readPreferenceStrict(key: string): string | null {
  if (isTauri() && GENERAL_PREFERENCE_KEYS.includes(key)) {
    if (!native) throw Error('Preferences not initialized');
    return native.values[key] ?? null;
  }
  return localStorage.getItem(key);
}

export function writePreference(key: string, value: string | null): boolean | Promise<boolean> {
  if (isTauri() && GENERAL_PREFERENCE_KEYS.includes(key)) {
    const result = writes.then(async () => {
      if (!native && !(await initializePreferences())) { report(key, 'write'); return false; }
      try {
        // Native patches merge under the DB mutex, not against this WebView's snapshot.
        accept(await patchGeneralPreference(key, value));
        clear(key, false);
        return true;
      } catch {
        report(key, 'write');
        return false;
      }
    });
    writes = result;
    return result;
  }
  try {
    // Establish readability before replacing an unknown persisted value.
    localStorage.getItem(key);
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
    lastRead.set(key, value);
    clear(key, false);
    return true;
  } catch {
    report(key, 'write');
    return false;
  }
}
