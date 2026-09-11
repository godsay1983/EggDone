import assert from "node:assert/strict";
import path from "node:path";
import os from "node:os";
import { createRequire } from "node:module";
import { createServer } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || "playwright");
const root = path.resolve(import.meta.dirname, "..");
// Mount production components with in-memory callbacks, never native storage or sync.
const harness = `
import { mount } from 'svelte';
import NoteCard from '/src/lib/components/NoteCard.svelte';
import TodoItem from '/src/lib/components/TodoItem.svelte';
import { setLanguageMode } from '/src/lib/i18n/index.ts';
import { recurrenceRules } from '/src/lib/stores/recurrenceStore.ts';
import '/src/app.css';
const p = new URLSearchParams(location.search);
setLanguageMode(p.get('locale'));
document.documentElement.dataset.theme = p.get('theme');
document.documentElement.style.zoom = p.get('zoom');
window.events = [];
const record = name => async () => { window.events.push(name); };
const title = p.get('locale') === 'en-US' ? 'Review the release checklist and coordinate the next milestone '.repeat(3) : '整理发布检查清单并确认下一阶段的任务与资料内容'.repeat(4);
const group = 'LongUnbrokenProjectGroupName'.repeat(4);
const custom = p.get('theme') === 'dark';
if (custom) recurrenceRules.set([{ uuid:'rule',first_todo_uuid:'todo',current_todo_uuid:'todo',current_date:'2026-09-12',generated_count:1,
 updated_at:1,updated_by:'layout-test',deleted_at:null,exhausted:false,timezone_id:'Asia/Shanghai',
 schedule:{anchor_date:'2026-09-12',frequency:'weekly',interval:3,weekdays:[1,2,3,4,5,6,7],month_day:null,local_time_minutes:990,
 end_type:'date',end_date:'2027-12-31',max_occurrences:null} }]);
mount(TodoItem, { target: document.querySelector('#task'), props: {
 todo: {id:1,uuid:'todo',title,note:'Additional context',group_uuid:'group',completed:false,pinned:true,priority:1,created_at:1,updated_at:1,due_date:'2026-09-12',due_at:1789200000000,reminder_at:1789196400000,repeat_rule:custom?null:'weekdays',repeat_series_uuid:custom?'rule':null},
 groups:[{uuid:'group',name:group,color:'yellow'}],animationEnabled:false,
 ...Object.fromEntries(['onToggle','onEdit','onNote','onPin','onPriority','onFocus','onSchedule','onSnooze','onGroupChange','onDelete','onMove','onDragStart','onBatchSelect'].map(k => [k,record(k)]))
}});
mount(NoteCard, { target: document.querySelector('#note'), props: {
 note:{id:1,uuid:'note',title,content:title,color:'default',pinned:true,updated_at:1789200000000},
 attachments:innerWidth === 800 ? [{uuid:'file',kind:'file',display_name:group+'.md'},{uuid:'file2',kind:'file',display_name:'Report.pdf'}] :
 [{uuid:'image',kind:'image',display_name:group+'.png'},{uuid:'file',kind:'file',display_name:group+'.md'}],
 onOpen:record('open'),onPin:record('pin'),onColor:record('color'),onDelete:record('delete')
}});
window.ready = true;
`;
const server = await createServer({
  root, configFile: false, logLevel: "error", resolve: { alias: { $lib: path.join(root, "src/lib") } },
  plugins: [svelte({ configFile: false }), {
    name: "content-layout-harness",
    resolveId: id => id === "/__e4.js" ? "\0e4" : undefined,
    load: id => id === "\0e4" ? harness : undefined,
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (!req.url?.startsWith("/__e4.html")) return next();
        res.setHeader("Content-Type", "text/html");
        res.end('<html><head><style>body{height:100%!important;overflow:hidden!important;padding:8px}main{width:100%;height:100%;overflow:auto}#note{margin-top:12px}</style></head><body><main><div id="task"></div><div id="note"></div><button id="outside">Outside</button></main><script type="module" src="/__e4.js"></script></body></html>');
      });
    }
  }], optimizeDeps: { noDiscovery: true, include: [] },
  server: { host: "127.0.0.1", port: 0, watch: { ignored: ["**/src-tauri/**"] } }
});
let browser;
let checks = 0;
try {
  await server.listen();
  console.log('Content-layout harness started.');
  browser = await chromium.launch({ headless: true, channel: process.env.BROWSER_CHANNEL });
  const port = server.httpServer.address().port;
  for (const width of [280, 380, 800]) for (const zoom of [1, 1.5])
    for (const theme of ["light", "dark"]) for (const locale of ["zh-CN", "en-US"]) {
      const page = await browser.newPage({ viewport: { width, height: 1000 } });
      console.log(JSON.stringify({ width, zoom, theme, locale }));
      const errors = [];
      page.on("pageerror", e => errors.push(e.message));
      await page.goto(`http://127.0.0.1:${port}/__e4.html?locale=${locale}&theme=${theme}&zoom=${zoom}`);
      await page.waitForFunction(() => window.ready === true);
      if (theme === 'dark') assert.match(await page.locator('.custom-repeat-badge').innerText(), /Asia\/Shanghai/);
      const more = page.locator('.note-card-more button');
      assert.equal(await page.locator('.note-card-actions button').count(), 0);
      await more.focus();
      await page.keyboard.press('Enter');
      await page.locator('.note-card-actions button').first().waitFor();
      assert.equal(await more.getAttribute('aria-expanded'), 'true');
      const issues = await page.evaluate(() => {
        const issues = [];
        const selectors = ['.todo-item p', '.todo-meta > *', '.checkbox', '.more-button', '.note-card-more button', '.note-card-actions button'];
        for (const el of document.querySelectorAll(selectors.join(','))) {
          const r = el.getBoundingClientRect();
          if (r.left < 0 || r.right > innerWidth + 1) issues.push(el.className + ': outside viewport');
          if (el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1) issues.push(el.className + ': clipped');
        }
        const r = document.querySelector('.todo-item').getBoundingClientRect();
        const c = document.querySelector('.checkbox').getBoundingClientRect();
        const m = document.querySelector('.more-button').getBoundingClientRect();
        if (Math.abs(c.top - m.top) > 6 * Number(new URLSearchParams(location.search).get('zoom'))) issues.push('unstable controls');
        if (m.top - r.top > 20 * Number(new URLSearchParams(location.search).get('zoom'))) issues.push('controls not top aligned');
        if (getComputedStyle(document.querySelector('.more-button')).opacity !== '1') issues.push('More hidden without hover');
        if (getComputedStyle(document.querySelector('.todo-group-badge')).borderTopLeftRadius !== '8px') issues.push('oversized tag corners');
        const rectangles = [...document.querySelectorAll('.todo-meta > *')].map(el => el.getBoundingClientRect());
        for (let i = 0; i < rectangles.length; i++) for (let j = i + 1; j < rectangles.length; j++) {
          const a = rectangles[i], b = rectangles[j];
          if (Math.min(a.right,b.right)-Math.max(a.left,b.left)>1 && Math.min(a.bottom,b.bottom)-Math.max(a.top,b.top)>1) issues.push('overlapping metadata');
        }
        return issues;
      });
      assert.deepEqual(issues, [], JSON.stringify({ width, zoom, theme, locale }));
      await page.locator('.note-card-actions button').first().click();
      assert.deepEqual(await page.evaluate(() => window.events), ['pin']);
      await page.locator('.note-card-actions button').nth(1).click();
      await page.locator('.note-color-picker button').last().click();
      assert.deepEqual(await page.evaluate(() => window.events), ['pin', 'color']);
      assert.equal(await more.evaluate(el => el === document.activeElement), true);
      await page.keyboard.press('Escape');
      assert.equal(await more.getAttribute('aria-expanded'), 'false');
      await more.click();
      await page.locator('.note-card-actions button').last().click();
      assert.deepEqual(await page.evaluate(() => window.events), ['pin', 'color', 'delete']);
      await page.keyboard.press('Escape');
      assert.equal(await more.getAttribute('aria-expanded'), 'false');
      assert.equal(await more.evaluate(el => el === document.activeElement), true);
      await page.locator('.note-card-body').click({ button: 'right' });
      assert.equal(await more.getAttribute('aria-expanded'), 'true');
      await page.locator('#outside').click();
      assert.equal(await more.getAttribute('aria-expanded'), 'false');
      await more.click();
      await page.locator('.note-card-body').click();
      assert.equal(await more.getAttribute('aria-expanded'), 'false');
      assert.deepEqual(await page.evaluate(() => window.events), ['pin', 'color', 'delete', 'open']);
      await page.locator('.checkbox').click();
      assert.equal(await page.evaluate(() => window.events.at(-1)), 'onToggle');
      await page.locator('.more-button').click();
      assert.equal(await page.locator('.actions-menu').count(), 1);
      assert.deepEqual(errors, []);
      if (width === 380 && zoom === 1.5 && theme === 'dark' && locale === 'en-US') {
        await page.locator('#outside').click();
        await more.click();
        const output = path.join(os.tmpdir(), 'eggdone-content-layout.png');
        await page.screenshot({ path: output, fullPage: true });
        console.log('Screenshot: ' + output);
      }
      checks++;
      await page.close();
    }
  console.log(checks + ' production-component layout/interaction combinations passed (not native acceptance).');
} catch (error) {
  const page = browser?.contexts().at(-1)?.pages().at(-1);
  if (page) {
    console.error('Failed page: ' + page.url());
    const output = path.join(os.tmpdir(), 'eggdone-content-layout-failure.png');
    await page.screenshot({ path: output, timeout: 5000 }).catch(() => {});
    console.error('Failure screenshot: ' + output);
  }
  throw error;
} finally {
  await browser?.close();
  await server.close();
}
