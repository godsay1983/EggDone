// Production UI and store with isolated IPC, not native database or cloud acceptance.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(tmpdir(), 'eggdone-trash-ui-' + Date.now());
mkdirSync(output, { recursive: true });
const native = `export const isTauri=()=>false;
export async function invoke(command,args){
  if(command==='list_trash'){if(window.failLoad)throw Error('read');return structuredClone(window.rows.slice(args.offset,args.offset+args.limit));}
  if(command==='preview_trash')return structuredClone(window.rows.find(r=>r.uuid===args.uuid));
  if(command==='restore_trash'){
    window.writes.push(args.expected);
    if(window.failWrite){window.failWrite=false;throw Error('TRASH_CONFLICT');}
    if(window.hold)await new Promise(resolve=>window.release=resolve);
    window.rows=window.rows.filter(r=>r.uuid!==args.expected.uuid);return;
  }
  throw Error('Unexpected IPC '+command);
}`;
const html = String.raw`<!doctype html><html><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/TrashDialog.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang'));
document.documentElement.dataset.theme=p.get('theme');document.documentElement.style.zoom=p.get('scale')||'1';
window.writes=[];window.trashClosed=false;window.failRefresh=false;window.failLoad=p.has('failLoad');window.hold=false;
window.rows=Array.from({length:p.has('pages')?51:2},(_,i)=>({kind:i%2?'note':'todo',uuid:String(i),
title:i===0?'Long task title '.repeat(7):'Note '+i,content:'Full body line\n'.repeat(35),
deleted_at:1789190000000,updated_at:1789190000000,updated_by:'test',completed:true,repeating:true,
attachments:i%2?[{uuid:'a',name:'Attachment-'.repeat(20)+'.md',updated_at:1,updated_by:'test',deleted_at:1}]:[]}));
const instance=mount(Dialog,{target:document.body,props:{afterCommit:async()=>{if(window.failRefresh)throw Error('refresh');},
onClose:()=>{window.trashClosed=true;void unmount(instance);}}});
</script></body></html>`;
const server = await createServer({ root, configFile: false,
  resolve: { alias: [{ find: '@tauri-apps/api/core', replacement: 'virtual:trash-ipc' }, { find: '$lib', replacement: resolve(root, 'src/lib') }], conditions: ['browser'] },
  plugins: [svelte(), { name: 'trash-ui', resolveId: id => id === 'virtual:trash-ipc' ? '\0trash-ipc' : null,
    load: id => id === '\0trash-ipc' ? native : null,
    configureServer(s) { s.middlewares.use('/__trash', async (_req, res) => { res.setHeader('Content-Type', 'text/html'); res.end(await s.transformIndexHtml('/__trash', html)); }); } }],
  optimizeDeps: { noDiscovery: true, include: ['svelte', 'svelte/store'] },
  server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/**'] } }
});
let browser, page;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true, channel: 'msedge' });
  page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => { errors.push(error.message); console.error('Browser:', error.message); });
  page.setDefaultTimeout(15000);
  const url = server.resolvedUrls.local[0] + '__trash';
  let count = 0;
  for (const lang of ['zh-CN', 'en-US']) for (const theme of ['light', 'dark'])
    for (const size of [{ width: 320, height: 430 }, { width: 480, height: 720 }, { width: 1000, height: 760 }]) for (const scale of [1, 1.5]) {
      console.log('Checking', lang, theme, size.width, scale);
      await page.setViewportSize(size);
      await page.goto(url + '?lang=' + lang + '&theme=' + theme + '&scale=' + scale);
      await page.locator('.record').first().waitFor();
      const restore = () => page.getByRole('button', { name: lang === 'zh-CN' ? '确认恢复' : 'Restore', exact: true });
      await page.locator('.record').first().click();
      await restore().waitFor();
      await page.keyboard.press('Escape');
      assert.equal(await page.evaluate(() => window.writes.length), 0);
      await page.locator('.record').first().click();
      await page.evaluate(() => window.failWrite = true);
      await restore().click(); await page.locator('.record').first().waitFor();
      assert.equal(await page.evaluate(() => window.rows.length), 2);
      await page.locator('.record').first().click();
      await page.evaluate(() => window.hold = true);
      await restore().click();
      await page.waitForFunction(() => !!window.release);
      assert.equal(await restore().isDisabled(), true);
      await page.keyboard.press('Escape');
      assert.equal(await page.evaluate(() => window.trashClosed), false);
      assert.equal(await page.locator('dialog').evaluate(el => el.open), true);
      await page.evaluate(() => { window.hold = false; window.release(); });
      await page.waitForFunction(() => window.rows.length === 1);
      await page.locator('.record').first().waitFor();
      await page.locator('.record').first().click();
      await page.locator('.attachments li').waitFor();
      assert.equal(await page.locator('.attachments li').count(), 1);
      assert.ok(await page.locator('dialog').evaluate(el => el.scrollWidth <= el.clientWidth), 'horizontal overflow');
      for (const button of await page.locator('footer button').all()) {
        const b = await button.boundingBox();
        assert.ok(b.x >= 0 && b.y >= 0 && b.x + b.width <= size.width + 1 && b.y + b.height <= size.height + 1, 'footer clipped');
      }
      if (size.width === 320) await page.screenshot({ path: resolve(output, lang + '-' + theme + '-' + scale + '.png') });
      await page.evaluate(() => window.failRefresh = true);
      await restore().click();
      await page.waitForFunction(() => window.rows.length === 0);
      await page.getByRole('status').filter({ hasText: lang === 'zh-CN' ? '页面刷新' : 'could not refresh' }).waitFor();
      await page.evaluate(() => window.failRefresh = false);
      await page.getByRole('button', { name: lang === 'zh-CN' ? '刷新列表' : 'Refresh list', exact: true }).click();
      assert.equal(await page.evaluate(() => window.writes.length), 3);
      await page.keyboard.press('Escape'); await page.waitForFunction(() => window.trashClosed); count++;
    }
  await page.goto(url + '?lang=en-US&theme=light&failLoad=1&pages=1');
  await page.getByRole('alert').waitFor(); assert.equal(await page.locator('.record').count(), 0);
  await page.evaluate(() => window.failLoad = false);
  await page.getByRole('button', { name: 'Refresh list', exact: true }).click();
  await page.locator('.record').first().waitFor(); assert.equal(await page.locator('.record').count(), 50);
  await page.getByRole('button', { name: 'Load more', exact: true }).click();
  await page.waitForFunction(() => document.querySelectorAll('.record').length === 51);
  assert.deepEqual(errors, []);
  console.log('Trash UI: ' + count + ' locale/theme/window/zoom combinations, cancel/conflict/busy/refresh retry/pagination passed. Screenshots: ' + output);
} catch (error) {
  if (page) {
    await page.screenshot({ path: resolve(output, 'failure.png') });
    console.error(await page.locator('dialog').evaluate(el => ({ open: el.open, text: el.innerText,
      height: el.getBoundingClientRect().height, content: el.querySelector('.content').getBoundingClientRect().height })));
    console.error('Failure screenshot:', resolve(output, 'failure.png'));
  }
  throw error;
} finally { await browser?.close(); await server.close(); }
