import { beforeEach, afterEach, it, expect, vi } from 'vitest';
import { get } from 'svelte/store';
import type { GeneralPreferences } from '../api/generalPreferencesApi';

const api = vi.hoisted(() => ({ read: vi.fn(), migrate: vi.fn(), patch: vi.fn(), listen: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true }));
vi.mock('@tauri-apps/api/event', () => ({ listen: api.listen }));
vi.mock('../api/generalPreferencesApi', () => ({ readGeneralPreferences: api.read, migrateGeneralPreferences: api.migrate, patchGeneralPreference: api.patch }));
const snapshot = (values: GeneralPreferences['values'] = {}, revision = 1): GeneralPreferences => ({ version: 1, revision, values });

beforeEach(() => {
  vi.resetModules(); vi.resetAllMocks(); api.listen.mockResolvedValue(() => {});
  vi.stubGlobal('window', { dispatchEvent: vi.fn() });
  vi.stubGlobal('localStorage', { getItem: vi.fn(() => null), setItem: vi.fn() });
});
afterEach(() => vi.unstubAllGlobals());

it('migrates only absent native state and retains the legacy source', async () => {
  api.read.mockResolvedValue(null);
  vi.mocked(localStorage.getItem).mockImplementation(key => key === 'eggdone-theme' ? 'dark' : null);
  api.migrate.mockImplementation(values => snapshot(values));
  const prefs = await import('./preferenceStorage');
  expect(await prefs.initializePreferences()).toBe(true);
  expect(prefs.readPreference('eggdone-theme')).toBe('dark');
  expect(api.migrate).toHaveBeenCalledOnce(); expect(localStorage.setItem).not.toHaveBeenCalled();
});

it('uses native preferences with unavailable WebView storage, including focus-window restart', async () => {
  api.read.mockResolvedValue(snapshot({ 'eggdone-theme': 'dark', 'eggdone-focus-duration-minutes': '45' }));
  vi.mocked(localStorage.getItem).mockImplementation(() => { throw Error('denied'); });
  let prefs = await import('./preferenceStorage');
  expect(await prefs.initializePreferences()).toBe(true);
  expect(prefs.readPreference('eggdone-theme')).toBe('dark');
  vi.resetModules(); prefs = await import('./preferenceStorage'); await prefs.initializePreferences();
  expect(prefs.readPreference('eggdone-focus-duration-minutes')).toBe('45');
  expect(localStorage.getItem).not.toHaveBeenCalled(); expect(api.migrate).not.toHaveBeenCalled();
});

it('blocks migration and writes after failed reads; a later successful retry is allowed', async () => {
  api.read.mockRejectedValue(Error('unreadable'));
  const prefs = await import('./preferenceStorage');
  expect(await prefs.initializePreferences()).toBe(false);
  expect(await prefs.writePreference('eggdone-theme', 'light')).toBe(false);
  expect(api.migrate).not.toHaveBeenCalled(); expect(api.patch).not.toHaveBeenCalled();
  api.read.mockResolvedValue(snapshot({ 'eggdone-theme': 'dark' }));
  api.patch.mockResolvedValue(snapshot({ 'eggdone-theme': 'light' }, 2));
  expect(await prefs.writePreference('eggdone-theme', 'light')).toBe(true);
  expect(get(prefs.preferenceStorageFailures)).toEqual([]);
});

it('does not migrate partial legacy reads or accept a future version', async () => {
  api.read.mockResolvedValue(null);
  vi.mocked(localStorage.getItem).mockImplementation(key => { if (key === 'eggdone-language') throw Error('denied'); return 'dark'; });
  const prefs = await import('./preferenceStorage');
  expect(await prefs.initializePreferences()).toBe(false); expect(api.migrate).not.toHaveBeenCalled();
  api.read.mockResolvedValue({ ...snapshot(), version: 2 });
  expect(await prefs.writePreference('eggdone-theme', 'light')).toBe(false);
  expect(api.patch).not.toHaveBeenCalled();
});

it('serializes patches, preserves confirmed values on failure and ignores stale cross-window snapshots', async () => {
  api.read.mockResolvedValue(snapshot({ 'eggdone-theme': 'dark' }));
  const prefs = await import('./preferenceStorage'); await prefs.initializePreferences();
  api.patch.mockRejectedValueOnce(Error('full')).mockResolvedValueOnce(snapshot({ 'eggdone-theme': 'dark', 'eggdone-language': 'en-US' }, 3));
  const failed = prefs.writePreference('eggdone-theme', 'light');
  const next = prefs.writePreference('eggdone-language', 'en-US');
  expect(await failed).toBe(false); expect(await next).toBe(true);
  expect(prefs.readPreference('eggdone-theme')).toBe('dark');
  const receive = api.listen.mock.calls[0][1];
  receive({ payload: snapshot({ 'eggdone-theme': 'light' }, 2) });
  expect(prefs.readPreference('eggdone-theme')).toBe('dark');
  receive({ payload: snapshot({ 'eggdone-theme': 'light', 'eggdone-language': 'en-US' }, 4) });
  expect(prefs.readPreference('eggdone-theme')).toBe('light');
});
