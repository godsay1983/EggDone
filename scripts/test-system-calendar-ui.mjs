// Real TodoPanel components, synthetic IPC and golden wire fixtures. No native app or user storage.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = mkdtempSync(resolve(tmpdir(), 'eggdone-system-calendar-ui-'));
const active = JSON.parse(readFileSync(resolve(root, 'tests/fixtures/system-calendar-v1-active.json'), 'utf8'));
const withdrawn = JSON.parse(readFileSync(resolve(root, 'tests/fixtures/system-calendar-v1-withdrawn.json'), 'utf8'));
const native = `
export const isTauri=()=>false;
export class Channel { constructor(callback){this.onmessage=callback;} }
export async function invoke(command,args){
 window.calls.push({command,args});
 if(command==='get_system_calendar_state'||command==='refresh_system_calendar') {
   const reply=structuredClone(window.calendarReply);
   if(window.holdCalendar)await new Promise(r=>window.releaseCalendar=r);
   if(window.failCalendar)throw Error('synthetic denied');
   return reply;
 }
 if(command==='get_sync_settings')return structuredClone(window.settings);
 if(command==='get_sync_runtime_state')return {schemaVersion:1,lastAttemptAt:null,lastSuccessAt:null,dirtySince:null,
   dirtyDomains:[],lastResult:'never',lastErrorCode:null,lastErrorMessage:null,pendingAttachmentCount:0,updatedAt:1};
 if(command==='sync_now')return {message:'done',todoCount:0,noteCount:0,noteAttachmentCount:0,pendingAttachmentCount:0,
   conflictRetried:false,todoRemoteEtag:null,noteRemoteEtag:null,noteAttachmentRemoteEtag:null};
 if(command==='get_remote_sync_state')return {recurrenceToken:'',todoObjectExists:false,noteObjectExists:false,
   noteAttachmentObjectExists:false,todoEtag:null,noteEtag:null,noteAttachmentEtag:null};
 if(command==='list_todos')return structuredClone(window.rows);
 if(['list_groups','list_notes','list_task_checklist_progress'].includes(command))return [];
 if(command==='list_daily_plans')return {date:args.date,revision:'0',current:[],previous:[]};
 if(command==='list_task_workflow')return {date:args.date,revision:'0',entries:[]};
 if(command==='recurrence_editor_context')return {rules:[],device_id:'fixture'};
 throw Error('Unexpected synthetic IPC '+command);
}`;
const html = `<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount} from 'svelte';
import Panel from '/src/lib/components/TodoPanel.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import {systemCalendar} from '/src/lib/stores/systemCalendarStore.ts';
import {configureAutoSync,runManualSync,runSettingsUpdate} from '/src/lib/sync/autoSync.ts';
import {calendarDayBounds,occurrenceOnDate} from '/src/lib/utils/systemCalendarDates.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);
localStorage.clear();localStorage.setItem('eggdone-theme',p.get('theme')||'light');
await setLanguageMode(p.get('lang')||'en-US');
window.calls=[];window.active=${JSON.stringify(active)};window.withdrawn=${JSON.stringify(withdrawn)};
window.calendarReply={document:structuredClone(window.active),configured:true,last_received_at:Date.now(),loading:false,error:''};
window.settings={enabled:true,credentialsConfigured:true,endpoint:'https://fixture.invalid',region:'test',bucket:'fixture',
 objectKey:'eggdone/todos.json',noteObjectKey:'',noteAttachmentObjectKey:'',noteAssetPrefix:'',pathStyle:true,allowHttp:false};
window.rows=p.has('empty')?[]:[{id:1,uuid:'fixture-task',title:'Independent task',note:null,group_uuid:null,completed:false,
 pinned:false,priority:0,sort_order:0,created_at:1,updated_at:1,completed_at:null,deleted_at:null,archived_at:null,
 due_date:'2026-09-20',due_at:null,reminder_at:null,repeat_rule:null,repeat_next_due_date:null,repeat_series_uuid:null}];
window.systemCalendar=systemCalendar;window.runManualSync=runManualSync;window.runSettingsUpdate=runSettingsUpdate;
window.configureAutoSync=configureAutoSync;window.calendarDayBounds=calendarDayBounds;window.occurrenceOnDate=occurrenceOnDate;
configureAutoSync(window.settings);await systemCalendar.rehydrate();
mount(Panel,{target:document.body});window.ready=true;
</script></body></html>`;

const server = await createServer({ root, configFile: false, publicDir: resolve(root, 'static'), cacheDir: resolve(output, 'vite-cache'),
  resolve: { alias: [{ find: '@tauri-apps/api/core', replacement: 'virtual:calendar-ipc' },
    { find: '$lib', replacement: resolve(root, 'src/lib') }], conditions: ['browser'], dedupe: ['svelte'] },
  plugins: [svelte(), { name: 'calendar-ui',
    resolveId: id => id === 'virtual:calendar-ipc' ? '\0calendar-ipc' : null,
    load: id => id === '\0calendar-ipc' ? native : null,
    configureServer(s) { s.middlewares.use('/__calendar', async (_req, res) => {
      res.setHeader('Content-Type', 'text/html; charset=utf-8');
      res.end(await s.transformIndexHtml('/__calendar', html));
    }); },
  }], optimizeDeps: { noDiscovery: true, include: ['svelte', 'svelte/store'] },
  server: { host: '127.0.0.1', port: 0, watch: null },
});

let browser;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true, channel: 'msedge' });
  const context = await browser.newContext({ timezoneId: 'America/New_York' });
  const page = await context.newPage();
  // All requests stay on the disposable component server, even if content contains URLs.
  await page.route('**/*', route => new URL(route.request().url()).hostname === '127.0.0.1'
    ? route.continue() : route.abort());
  await page.clock.install({ time: new Date('2026-09-20T12:00:00-04:00') });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.setDefaultTimeout(15000);
  const go = async (query = '') => {
    await page.goto(server.resolvedUrls.local[0] + '__calendar?' + query);
    await page.waitForFunction(() => window.ready);
    await page.locator('.view-switch button').filter({hasText:/^(Calendar|日历)$/}).click();
    await page.locator('.system-calendar').waitFor();
  };
  const refresh = async () => {
    await page.locator('.system-calendar .refresh').click();
    await page.waitForFunction(() => document.querySelector('.system-calendar').getAttribute('aria-busy') === 'false');
  };
  let cases = 0;
  for (const lang of ['en-US', 'zh-CN']) for (const theme of ['light', 'dark']) for (const width of [320, 480, 1000]) {
    await page.setViewportSize({ width, height: 800 });
    await go('lang=' + lang + '&theme=' + theme);
    assert.equal(await page.locator('.system-calendar input,.system-calendar [role="checkbox"]').count(), 0);
    assert.equal(await page.locator('.system-calendar button').count(), 1, 'only calendar refresh is actionable');
    assert.equal(await page.locator('.system-calendar li').count(), 2, 'all-day dates stay fixed in viewer timezone');
    assert.equal(await page.locator('.agenda-week-strip .has-events').count(), 2);
    assert(!(await page.locator('.system-calendar').innerText()).includes(active.owner_id), 'no UUID wall');
    assert((await page.locator('.system-calendar').innerText()).includes('11111111'));
    assert.equal(await page.locator('[data-todo-id="1"]').count(), 1);
    const sunday = page.locator('.agenda-week-strip button').first();
    await sunday.focus(); await page.keyboard.press('Space');
    assert.equal(await sunday.getAttribute('aria-pressed'), 'true');
    assert.equal(await page.locator('.system-calendar li').count(), 1, 'timed event projects to previous viewer date');
    assert((await page.locator('.all-day-span').innerText()).includes('2026-09-20'));
    assert((await page.locator('.all-day-span').innerText()).includes('2026-09-21'), 'all-day end is inclusive');
    assert(!(await page.locator('.all-day-span').innerText()).includes('2026-09-22'));
    assert((await page.locator('.event-zone').innerText()).includes('UTC'), 'different occurrence zone stays accessible');
    assert.equal(await page.locator('.metadata-details').getAttribute('open'), null, 'compact metadata starts collapsed');
    assert.equal(await page.locator('[data-todo-id="1"]').count(), 1, 'system events do not become tasks');
    assert(await page.locator('.system-calendar').evaluate(e => e.scrollWidth <= e.clientWidth), 'calendar overflow');
    assert(await page.locator('.panel-shell').evaluate(e => e.scrollWidth <= e.clientWidth), 'panel overflow');
    await page.screenshot({ path: resolve(output, `${lang}-${theme}-${width}.png`), fullPage: true });
    cases++;
  }

  await go('empty');
  assert.equal(await page.locator('.system-calendar li').count(), 2, 'calendar usable without tasks');
  const dst = await page.evaluate(() => ['2026-03-08', '2026-11-01'].map(d => {
    const [start,end]=window.calendarDayBounds(d); return (end-start)/3600000;
  }));
  assert.deepEqual(dst, [23,25], 'production day helper handles viewer DST');
  await page.locator('.metadata-details summary').focus();
  await page.keyboard.press('Space');
  assert.notEqual(await page.locator('.metadata-details').getAttribute('open'), null);
  await page.getByText('Source zone: Asia/Shanghai', {exact:true}).waitFor();
  await page.getByText('Received:', {exact:false}).waitFor();
  await page.screenshot({path:resolve(output,'expanded-source-metadata.png'),fullPage:true});
  await page.locator('.metadata-details summary').click();

  await page.evaluate(() => {
    window.calendarReply.document.occurrences[0].title='<img src=x onerror=alert(1)>Plain event';
    window.calendarReply.document.occurrences[1].title='长标题 '.repeat(90);
    window.calendarReply.document.occurrences[1].location='<b>Plain location</b>';
    window.calendarReply.document.calendars[0].title='<script>plain source</script>';
    window.calendarReply.document.captured_at=Date.now()-25*3600000;
    window.calendarReply.last_received_at=Date.now()-6*60000;
  });
  await refresh();
  assert.equal(await page.locator('.system-calendar img,.system-calendar script,.system-calendar b').count(), 0);
  await page.getByText('Source snapshot is over 24 hours old.', {exact:false}).waitFor();
  await page.getByText('Cache received over 5 minutes ago', {exact:false}).waitFor();
  assert(await page.locator('.system-calendar').evaluate(e => e.scrollWidth <= e.clientWidth));
  await page.screenshot({path:resolve(output,'long-plain-stale.png'),fullPage:true});

  for (const [code,text] of [
    ['CALENDAR_DENIED','Calendar access denied.'],
    ['CALENDAR_INVALID_DOCUMENT','Calendar snapshot or response is invalid'],
    ['CALENDAR_SOURCE_MISSING','The previously shared calendar snapshot'],
    ['CALENDAR_TARGET_CHANGED','Calendar sync target changed.'],
    ['CALENDAR_NETWORK','Calendar is temporarily unreachable.'],
  ]) {
    await page.evaluate(code=>{window.calendarReply.error=code;},code);await refresh();
    await page.getByText(text,{exact:false}).waitFor();
    await page.getByText('Showing the last received snapshot,', {exact:false}).waitFor();
  }
  await page.evaluate(()=>{window.calendarReply.error='';});

  await page.evaluate(() => { window.failCalendar=true; }); await refresh();
  await page.getByText('Calendar refresh failed.', {exact:false}).waitFor();
  assert.equal(await page.locator('.system-calendar li').count(), 2, 'failed refresh retains cached events');
  assert.equal(await page.evaluate(() => window.calls.filter(c=>c.command==='sync_now').length), 0);
  await page.evaluate(() => { window.failCalendar=false; window.calendarReply.document=null; }); await refresh();
  await page.getByText('No shared calendar snapshot was found.', {exact:true}).waitFor();
  await page.evaluate(() => { window.calendarReply.document=window.withdrawn; }); await refresh();
  await page.getByText('The source device has stopped calendar sharing.', {exact:true}).waitFor();
  assert.equal(await page.locator('.system-calendar li').count(), 0);
  assert.equal(await page.locator('.agenda-week-strip .has-events').count(), 0);

  await page.evaluate(() => { window.calendarReply.document=structuredClone(window.active); }); await refresh();
  await page.getByRole('button',{name:'Jump to date',exact:true}).click();
  await page.locator('.agenda-date-picker').fill('2027-03-21');
  await page.getByText('Outside snapshot coverage.',{exact:false}).waitFor();
  assert.equal(await page.getByText('No shared events in this snapshot for this date.',{exact:true}).count(),0);

  // Saving hides old content immediately; even a delayed prior IPC cannot rehydrate it.
  await page.evaluate(() => { window.holdCalendar=true; void window.systemCalendar.refresh(); });
  await page.waitForFunction(() => !!window.releaseCalendar);
  await page.evaluate(() => {
    window.save=window.runSettingsUpdate(async()=>{
      await new Promise(r=>window.releaseSave=r);
      window.settings={...window.settings,bucket:'new-target'};
      window.configureAutoSync(window.settings);
    });
  });
  await page.getByText('Sync settings are changing.',{exact:false}).waitFor();
  assert.equal(await page.locator('.system-calendar .metadata').count(),0);
  await page.evaluate(() => {window.holdCalendar=false;window.releaseCalendar();});
  assert.equal(await page.locator('.system-calendar .metadata').count(),0);
  await page.evaluate(async () => {window.calendarReply.document=null;window.releaseSave();await window.save;});
  await page.getByText('No shared calendar snapshot was found.',{exact:true}).waitFor();

  // A hanging or failed calendar request does not hold task completion or start task retries.
  await page.evaluate(async () => {
    window.holdCalendar=true;window.releaseCalendar=null;
    window.systemCalendar.setForeground(true);
    await window.runManualSync();window.taskCompleted=true;
  });
  assert.equal(await page.evaluate(() => window.taskCompleted),true);
  await page.waitForFunction(() => !!window.releaseCalendar);
  await page.evaluate(() => {window.failCalendar=true;window.holdCalendar=false;window.releaseCalendar();});
  await page.getByText('Calendar refresh failed.',{exact:false}).waitFor();
  assert.equal(await page.evaluate(() => window.calls.filter(c=>c.command==='sync_now').length),1);
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({cases, screenshots:output, checks:'calendar week/day, no tasks, plain text, freshness, coverage, target epochs, independent sync, DST'},null,2));
} finally {
  if(browser)await browser.close();
  await server.close();
}
