import assert from 'node:assert/strict';
import path from 'node:path';
import fs from 'node:fs/promises';
import os from 'node:os';
import { createRequire } from 'node:module';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { compile } from 'svelte/compiler';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = path.resolve(import.meta.dirname, '..');
const artifacts = await fs.mkdtemp(path.join(os.tmpdir(), 'eggdone-capability-'));
const initial = {
  shortcut: 'Alt+Shift+Space', shortcutEnabled: false, shortcutPreferenceKnown: false,
  noteShortcut: 'Alt+Shift+N', noteShortcutEnabled: false, noteShortcutPreferenceKnown: false,
  autostartEnabled: false, shortcutStatus: 'unknown', noteShortcutStatus: 'unknown', autostartStatus: 'unknown',
  shortcutError: null, noteShortcutError: null, autostartError: null,
};
// This harness verifies the production settings UI. OS behavior is covered separately by API tests.
const mock = `
export const shortcutOptions=[{value:'Alt+Shift+Space',label:'Alt + Shift + Space'}];
export const noteShortcutOptions=[{value:'Alt+Shift+N',label:'Alt + Shift + N'}];
export async function refreshDesktopSettings(){window.reads++;return {...window.capability};}
export async function updateShortcut(){window.registrations++;window.capability.shortcutStatus='enabled';}
export async function updateNoteShortcut(){window.registrations++;}
export async function updateAutostart(enabled){window.autostartWrites++;return enabled;}
`;
const component = compile(`<script>
import SettingsPanel from '/src/lib/components/SettingsPanel.svelte';
let settings=window.capability;
</script>
<SettingsPanel {settings} defaultListViewMode="remember" onChange={value=>settings=value} onClose={()=>{}} onDefaultListViewChange={()=>{}} />`,
  { filename: 'CapabilityHarness.svelte', generate: 'client' }).js.code;
const entry = `
import {mount} from 'svelte';
import Root from '/__capability-root.js';
import {initializeLanguage,setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
await setLanguageMode(new URLSearchParams(location.search).get('locale'));initializeLanguage();
window.capability=${JSON.stringify(initial)};window.reads=0;window.registrations=0;window.autostartWrites=0;
mount(Root,{target:document.querySelector('#app')});window.ready=true;
`;
const modules = { '/__capability-api.js': mock, '/__capability-root.js': component, '/__capability-entry.js': entry };
const server = await createServer({ root, configFile: false, logLevel: 'error',
  resolve: { alias: [
    { find: '$lib/api/desktopSettings', replacement: '/__capability-api.js' },
    { find: '$lib', replacement: path.join(root, 'src/lib') },
  ] }, plugins: [svelte({ configFile: false }), {
    name: 'capability-ui-harness', resolveId: id => modules[id] ? '\0' + id : undefined,
    load: id => modules[id.slice(1)], configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (!req.url?.startsWith('/__capability.html')) return next();
        res.setHeader('Content-Type', 'text/html');
        res.end('<html><body><div id="app"></div><script type="module" src="/__capability-entry.js"></script></body></html>');
      });
    },
  }], optimizeDeps: { noDiscovery: true, include: [] },
  server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/**'] } },
});
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true, channel: process.env.BROWSER_CHANNEL });
  for (const locale of ['en-US', 'zh-CN']) for (const theme of ['light', 'dark']) for (const width of [380, 820]) {
    const page = await browser.newPage({ viewport: { width, height: 900 } });
    const errors = []; page.on('pageerror', e => errors.push(e.message));
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__capability.html?locale=${locale}`);
    await page.waitForFunction(() => window.ready && window.reads > 0);
    await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
    const refresh = page.getByRole('button', { name: locale === 'en-US' ? 'Refresh system status' : '刷新系统状态', exact: true });
    await refresh.waitFor();
    const toggles = page.locator('.setting-row input[type=checkbox]');
    assert.equal(await toggles.count(), 3);
    for (const toggle of await toggles.all()) assert.equal(await toggle.isDisabled(), true);
    assert.equal(await page.locator('[role=status]').filter({ hasText: locale === 'en-US' ? 'System status unknown' : '系统状态未知' }).count(), 3);
    await page.evaluate(() => {
      Object.assign(window.capability, { shortcutPreferenceKnown: true, shortcutEnabled: true, shortcutStatus: 'inactive',
        noteShortcutPreferenceKnown: true, noteShortcutEnabled: true, noteShortcutStatus: 'enabled', autostartStatus: 'enabled', autostartEnabled: true });
    });
    await refresh.click();
    const retry = page.getByRole('button', { name: locale === 'en-US' ? 'Retry shortcut registration' : '重试注册快捷键', exact: true });
    await retry.waitFor();
    assert.equal(await toggles.first().isChecked(), true);
    assert.equal(await page.evaluate(() => window.registrations + window.autostartWrites), 0);
    await page.screenshot({ path: path.join(artifacts, `${locale}-${theme}-${width}.png`) });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth), false);
    await retry.click();
    await page.waitForFunction(() => window.registrations === 1);
    await retry.waitFor({ state: 'detached' });
    await page.evaluate(() => {
      window.capability.autostartStatus = 'disabled';window.capability.autostartEnabled = false;
      window.dispatchEvent(new Event('focus'));
    });
    await page.waitForFunction(() => !document.querySelectorAll('.setting-row input[type=checkbox]')[2].checked);
    assert.equal(await page.evaluate(() => window.autostartWrites), 0);
    assert.deepEqual(errors, []);
    console.log(`PASS ${locale} ${theme} ${width}px: unknown/recovery/retry/focus refresh, no implicit OS writes`);
    await page.close();
  }
  console.log(`Screenshots: ${artifacts}`);
} finally { await browser?.close(); await server.close(); }
