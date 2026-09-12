// Real WebView2/Rust preferences. Invoked by run-preferences-native.ps1 across separate processes.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

const [phase, statePath] = process.argv.slice(2);
assert.ok(['write', 'read', 'restore'].includes(phase) && statePath, 'Expected phase and state file');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
let browser;
let page;
const report = { schemaVersion: 1, phase, passed: false, checks: [], pageErrors: [], error: null };
const call = (target, command, args) => target.evaluate(({ command, args }) =>
  window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const invoke = (command, args) => call(page, command, args);
const expected = {
  'eggdone-language': 'en-US', 'eggdone-theme': 'dark',
  'eggdone-default-list-view': 'calendar', 'eggdone-list-view': 'calendar',
  'eggdone-focus-duration-minutes': '45', 'eggdone-break-duration-minutes': '15',
  'eggdone-smart-view': null,
  'eggdone-pinned-smart-views': JSON.stringify({ version: 1, ids: ['no_date', 'next7'] }),
};
const legacyKeys = ['eggdone-language', 'eggdone-theme', 'eggdone-default-list-view'];

async function nativePage(matches) {
  for (let attempt = 0; attempt < 150; attempt++) {
    const found = browser.contexts().flatMap(c => c.pages()).find(p => matches(p.url()));
    if (found) return found;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw Error(`Native page did not finish initial navigation: ${JSON.stringify(browser.contexts().flatMap(c => c.pages()).map(p => p.url()))}`);
}
async function preferencesMatch(values) {
  await page.waitForFunction(async values => {
    const current = await window.__TAURI_INTERNALS__.invoke('get_general_preferences');
    return Object.entries(values).every(([key, value]) => (current?.values[key] ?? null) === value);
  }, values);
}
async function showMain() {
  await invoke('plugin:window|show', { label: 'main' });
  await invoke('plugin:window|set_focus', { label: 'main' });
  await page.locator('.settings-button').waitFor();
}
async function settings() {
  if (!await page.locator('.settings-card').count()) await page.locator('.settings-button').click();
  return page.locator('.settings-card');
}
async function capture(target, name) {
  const session = await target.context().newCDPSession(target);
  try {
    const shot = await session.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
    writeFileSync(resolve(dirname(statePath), `${phase}-${name}.png`), Buffer.from(shot.data, 'base64'));
  } finally { await session.detach(); }
}
async function windowState() {
  const size = await invoke('plugin:window|inner_size', { label: 'main' });
  const scale = await invoke('plugin:window|scale_factor', { label: 'main' });
  const viewport = await page.evaluate(() => ({ width: innerWidth, height: innerHeight, ratio: devicePixelRatio }));
  return { size, scale, viewport };
}

try {
  browser = await chromium.connectOverCDP('http://127.0.0.1:9228');
  page = await nativePage(url => url === 'http://tauri.localhost/');
  page.setDefaultTimeout(15000);
  assert.equal(await invoke('plugin:app|identifier'), 'com.eggdone.searchtest', 'Refuse the ordinary client');
  const sync = await invoke('get_sync_settings');
  assert.equal(sync.enabled, false);
  assert.equal(sync.credentialsConfigured, false);
  page.on('pageerror', error => report.pageErrors.push(String(error)));
  await showMain();

  if (phase === 'write') {
    assert.equal(existsSync(statePath), false, 'Never overwrite recovery state');
    const saved = {
      general: await invoke('get_general_preferences'), window: await invoke('get_window_preferences'),
      legacy: await page.evaluate(keys => Object.fromEntries(keys.map(key => [key, localStorage.getItem(key)])), legacyKeys),
    };
    assert.ok(saved.general && saved.window, 'Wait for native initialization before testing');
    writeFileSync(statePath, JSON.stringify(saved, null, 2), { flag: 'wx' });
    // Reset only the isolated test application's filter fixture, not real task data.
    for (const [key, value] of Object.entries({ 'eggdone-smart-view': null,
      'eggdone-pinned-smart-views': JSON.stringify({ version: 1, ids: [] }) })) {
      await invoke('patch_general_preference', { key, value });
    }
    await page.reload(); await showMain();
    const card = await settings();
    await card.getByRole('button', { name: 'English', exact: true }).click();
    await page.waitForFunction(() => document.documentElement.lang === 'en-US');
    await card.locator('.preference-select select').selectOption('calendar');
    await card.locator('.duration-options').nth(0).getByRole('button', { name: '45 min', exact: true }).click();
    await card.locator('.duration-options').nth(1).getByRole('button', { name: '15 min', exact: true }).click();
    await card.getByRole('button', { name: 'Comfortable', exact: true }).click();
    await page.waitForFunction(async () => {
      const p = await window.__TAURI_INTERNALS__.invoke('get_window_preferences');
      return p?.width === 480 && p.height === 680 && p.zoom === 1.25;
    });
    await card.locator('header button').click();
    if (await page.evaluate(() => document.documentElement.dataset.theme) !== 'dark') await page.locator('.theme-button').click();
    await page.locator('.view-switch').getByRole('button', { name: 'Calendar', exact: true }).click();
    await page.locator('.summary-menu-button').click();
    await page.locator('.smart-pin[aria-label="Pin No date"]').click();
    await page.locator('.smart-pin[aria-label="Pin Next 7 days"]').click();
    await page.locator('.summary-menu-button').click();
    await preferencesMatch(expected);
    await capture(page, 'main');
    saved.expected = expected;
    saved.actualWindow = await windowState();
    saved.expectedWindow = await invoke('get_window_preferences');
    writeFileSync(statePath, JSON.stringify(saved, null, 2));
    // Conflicting legacy values must not win after native migration and process restart.
    await page.evaluate(() => {
      localStorage.setItem('eggdone-language', 'zh-CN');
      localStorage.setItem('eggdone-theme', 'light');
      localStorage.setItem('eggdone-default-list-view', 'today');
    });
    report.checks.push('settings-ui-persisted');
  } else {
    const saved = JSON.parse(readFileSync(statePath, 'utf8'));
    if (phase === 'read') {
      assert.ok(saved.expected && saved.actualWindow, 'Write phase did not complete');
      await preferencesMatch(saved.expected);
      assert.deepEqual(await invoke('get_window_preferences'), saved.expectedWindow);
      assert.deepEqual(await windowState(), saved.actualWindow);
      assert.equal(await page.evaluate(() => document.documentElement.lang), 'en-US');
      assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), 'dark');
      assert.equal(await page.locator('.view-switch button[aria-pressed="true"]').innerText(), 'Calendar');
      assert.deepEqual(await page.locator('.pinned-smart-views button').allInnerTexts(), ['No date', 'Next 7 days']);
      assert.equal(await page.locator('.smart-filter').count(), 0, 'Pinning must not apply a filter');
      await capture(page, 'main');
      report.checks.push('restart-native-window-and-legacy-precedence');
      const card = await settings();
      assert.equal(await card.locator('.preference-select select').inputValue(), 'calendar');
      assert.equal(await card.locator('.window-zoom select').inputValue(), '1.25');
      assert.equal(await card.locator('.duration-options').nth(0).locator('button.active').innerText(), '45 min');
      assert.equal(await card.locator('.duration-options').nth(1).locator('button.active').innerText(), '15 min');
      await capture(page, 'settings');
      await card.locator('header button').click();
      await invoke('open_focus_window');
      const focus = await nativePage(url => url.startsWith('http://tauri.localhost/') && url.includes('window=focus'));
      await focus.waitForFunction(() => document.documentElement.lang === 'en-US' && document.documentElement.dataset.theme === 'dark');
      assert.equal(await focus.locator('.focus-window-body strong').innerText(), '45:00');
      await capture(focus, 'focus');
      await invoke('hide_focus_window');
      report.checks.push('settings-controls-and-focus-window');
    } else {
      const current = await invoke('get_general_preferences');
      const values = Object.fromEntries([...new Set([...Object.keys(current.values), ...Object.keys(saved.general.values)])]
        .map(key => [key, saved.general.values[key] ?? null]));
      for (const [key, value] of Object.entries(values)) await invoke('patch_general_preference', { key, value });
      await page.evaluate(values => {
        for (const [key, value] of Object.entries(values)) {
          if (value === null) localStorage.removeItem(key); else localStorage.setItem(key, value);
        }
      }, saved.legacy);
      await invoke('save_window_preferences', { preferences: saved.window });
      await page.reload(); await showMain();
      await preferencesMatch(values);
      await page.waitForFunction(async p => JSON.stringify(await window.__TAURI_INTERNALS__.invoke('get_window_preferences')) === JSON.stringify(p), saved.window);
      assert.deepEqual(await page.evaluate(keys => Object.fromEntries(keys.map(key => [key, localStorage.getItem(key)])), legacyKeys), saved.legacy);
      report.checks.push('isolated-preferences-restored');
    }
  }
  assert.deepEqual(report.pageErrors, []);
  report.passed = true;
} catch (error) {
  report.error = String(error);
  throw error;
} finally {
  writeFileSync(resolve(dirname(statePath), `${phase}-report.json`), JSON.stringify(report, null, 2));
  await browser?.close();
}
console.log(JSON.stringify(report, null, 2));
