import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
let data: Map<string, string>, failRead: boolean, failWrite: boolean;
beforeEach(() => {
  vi.resetModules(); data = new Map(); failRead = false; failWrite = false;
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => { if (failRead) throw Error('denied'); return data.get(key) ?? null; },
    setItem: (key: string, value: string) => { if (failWrite) throw Error('full'); data.set(key, value); },
    removeItem: (key: string) => { if (failWrite) throw Error('full'); data.delete(key); }
  });
});
afterEach(() => vi.unstubAllGlobals());
describe('preference storage failure boundaries', () => {
  it('strict reads distinguish absent preferences from a denied store', async () => {
    const api = await import('./preferenceStorage');
    expect(api.readPreferenceStrict('eggdone-pinned-smart-views')).toBeNull();
    failRead = true;
    expect(() => api.readPreferenceStrict('eggdone-pinned-smart-views')).toThrow();
  });
  it('keeps reads nonthrowing, reports failures and preserves known values', async () => {
    const api = await import('./preferenceStorage'); data.set('theme', 'dark');
    expect(api.readPreference('theme')).toBe('dark'); failRead = true;
    expect(api.readPreference('theme')).toBe('dark'); expect(api.readPreference('missing')).toBeNull();
    expect(get(api.preferenceStorageFailures)).toEqual(['read']);
    expect(api.writePreference('theme', 'light')).toBe(false); expect(data.get('theme')).toBe('dark');
  });
  it('reports failures until each affected key is saved, including removal', async () => {
    const api = await import('./preferenceStorage'); data.set('view', 'today'); failWrite = true;
    expect(api.writePreference('view', null)).toBe(false); expect(api.readPreference('view')).toBe('today');
    expect(api.writePreference('theme', 'dark')).toBe(false); failWrite = false;
    expect(api.writePreference('theme', 'dark')).toBe(true); expect(get(api.preferenceStorageFailures)).toEqual(['write']);
    expect(api.writePreference('view', null)).toBe(true); expect(get(api.preferenceStorageFailures)).toEqual([]);
    expect(data.has('view')).toBe(false);
  });
  it('keeps focus defaults on failed save and allows retry', async () => {
    vi.stubGlobal('window', { dispatchEvent: vi.fn() }); data.set('eggdone-focus-duration-minutes', '45');
    const api = await import('./focusSettings'); failWrite = true;
    expect(await api.saveFocusDurationMinutes(15)).toBe(45); expect(window.dispatchEvent).not.toHaveBeenCalled(); failWrite = false;
    expect(await api.saveFocusDurationMinutes(15)).toBe(15); expect(data.get('eggdone-focus-duration-minutes')).toBe('15');
    expect(window.dispatchEvent).toHaveBeenCalledOnce();
  });
  it('language initialization survives denied storage without overwriting', async () => {
    vi.stubGlobal('window', { addEventListener: vi.fn() }); vi.stubGlobal('navigator', { languages: ['en-US'] });
    failRead = true; const api = await import('../i18n');
    expect(api.initializeLanguage().mode).toBe('system'); expect((await api.setLanguageMode('zh-CN')).mode).toBe('system');
    expect(data.size).toBe(0); failRead = false; expect((await api.setLanguageMode('zh-CN')).mode).toBe('zh-CN');
  });
});
