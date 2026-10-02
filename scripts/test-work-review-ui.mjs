// Isolated production Svelte/IPC harness; never reads user data or touches a real clipboard.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(tmpdir(), 'eggdone-review-ui-' + Date.now());
mkdirSync(output, { recursive: true });
const fixture = JSON.parse(readFileSync(resolve(root, 'docs/fixtures/work-review-v1.json'), 'utf8'));
const fixtureScript = JSON.stringify(fixture).replace(/[\u0080-\uffff<]/g, character => '\\u' + character.charCodeAt(0).toString(16).padStart(4, '0'));
const native = `export const isTauri=()=>true;
export async function invoke(command,args){
  window.calls.push({command,args:structuredClone(args)});
  if(command==='list_work_review'){
    if(window.failLoad)throw Error('REVIEW_DATABASE');
    const query=JSON.stringify(args.query),token=String(window.revision);
    if(args.cursor&&(args.cursor.query_key!==query||args.cursor.snapshot_token!==token))throw Error('REVIEW_CHANGED');
    let rows=window.rows.filter(r=>r.created_at>=args.query.start_at&&r.created_at<args.query.end_at);
    if(args.query.group_scope==='ungrouped')rows=rows.filter(r=>!r.group_uuid);
    if(args.query.group_scope==='group')rows=rows.filter(r=>r.group_uuid===args.query.group_uuid);
    if(args.query.keyword)rows=rows.filter(r=>(r.body+' '+r.task_title).toLowerCase().includes(args.query.keyword));
    const start=args.cursor?rows.findIndex(r=>r.record_uuid===args.cursor.record_uuid)+1:0;
    const pageRows=structuredClone(rows.slice(start,start+30)),last=pageRows.at(-1);
    const result={rows:pageRows,matching_entry_count:rows.length,matching_task_count:new Set(rows.map(r=>r.task_uuid)).size,snapshot_token:token,
      next_cursor:start+30<rows.length?{created_at:last.created_at,record_uuid:last.record_uuid,query_key:query,snapshot_token:token}:null};
    if(window.holdList){window.holdList=false;await new Promise(r=>window.releaseList=r);}return result;
  }
  if(command==='snapshot_work_review'){
    const result={rows:structuredClone(window.rows),matching_entry_count:window.rows.length,
      matching_task_count:new Set(window.rows.map(r=>r.task_uuid)).size,snapshot_token:String(window.revision)};
    if(window.holdCopy){window.holdCopy=false;await new Promise(r=>window.releaseCopy=r);}return result;
  }
  if(command==='validate_work_review')return !window.invalidToken&&args.snapshotToken===String(window.revision);
  if(command==='resolve_search_target'){
    if(window.missing)throw Error('SEARCH_UNAVAILABLE');
    const row=window.rows.find(r=>r.task_uuid===args.uuid);return {kind:'todo',uuid:args.uuid,title:row.task_title,archived:row.archived,completed:row.completed,content:''};
  }
  if(command==='list_task_progress'){
    const row=window.rows.find(r=>r.task_uuid===args.taskUuid);return {task_uuid:row.task_uuid,title:row.task_title,read_only:row.archived,
      entries:[{token:'record',record:{uuid:row.record_uuid,task_uuid:row.task_uuid,body:row.body,created_at:row.created_at,updated_at:row.updated_at,clock:1,deleted_at:null}}],
      total:1,next_cursor:null,overwritten:false};
  }
  if(command==='count_task_progress')return args.taskUuids.map(task_uuid=>({task_uuid,count:1}));
  throw Error('Unexpected IPC '+command);
}`;
const events = `export async function listen(event,callback){window.listeners??=new Map();let set=window.listeners.get(event);if(!set)window.listeners.set(event,set=new Set());set.add(callback);return()=>set.delete(callback);}`;
const html = `<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/WorkReviewDialog.svelte';
import {languageState} from '/src/lib/i18n/index.ts';
import {reviewDateRange,reviewPeriodDates,formatReviewSummary} from '/src/lib/utils/workReview.ts';
import {systemCalendar} from '/src/lib/stores/systemCalendarStore.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search),fixture=${fixtureScript};
languageState.set({mode:p.get('lang')||'en-US',resolvedLocale:p.get('lang')||'en-US'});
document.documentElement.dataset.theme=p.get('theme')||'light';document.documentElement.style.zoom=p.get('scale')||'1';
window.calls=[];window.listeners=new Map();window.revision=1;window.clipboard=[];window.reviewClosed=false;
Object.defineProperty(navigator,'clipboard',{value:{writeText:async text=>{if(window.clipboardFail)throw Error('denied');window.clipboard.push(text);}}});
const week=reviewPeriodDates('week'),start=reviewDateRange(week).start_at;
window.rows=Array.from({length:p.has('empty')?0:65},(_,i)=>({...fixture.snapshot.rows[i%3],
  record_uuid:'10000000-0000-4000-8000-'+String(1000-i).padStart(12,'0'),created_at:start+3600000*(72-i),updated_at:start+3600000*(72-i)+1,
  body:i===0?'Long plain text '+('x'.repeat(600))+'\\nLine two\\nLine three\\nLine four':'Progress '+i}));
window.emit=event=>{for(const callback of window.listeners.get(event)||[])callback({event,payload:null});};
window.dateRange=reviewDateRange;window.fixture=fixture;window.formatSummary=formatReviewSummary;
window.beginTargetChange=()=>systemCalendar.beginSettingsUpdate();window.endTargetChange=()=>systemCalendar.endSettingsUpdate();
const component=mount(Dialog,{target:document.body,props:{onClose:()=>{window.reviewClosed=true;void unmount(component);}}});window.ready=true;
</script></body></html>`;
const server = await createServer({ root, configFile: false, cacheDir: resolve(output, 'vite-cache'),
  resolve: { alias: [{ find: '@tauri-apps/api/core', replacement: 'virtual:review-ipc' },
    { find: '@tauri-apps/api/event', replacement: 'virtual:review-events' }, { find: '$lib', replacement: resolve(root, 'src/lib') }], conditions: ['browser'] },
  plugins: [svelte(), { name: 'review-ui',
    resolveId: id => id === 'virtual:review-ipc' ? '\0review-ipc' : id === 'virtual:review-events' ? '\0review-events' : null,
    load: id => id === '\0review-ipc' ? native : id === '\0review-events' ? events : null,
    configureServer(server) { server.middlewares.use('/__review', async (_req, res) => { res.setHeader('Content-Type', 'text/html; charset=utf-8'); res.end(await server.transformIndexHtml('/__review', html)); }); },
  }], optimizeDeps: { noDiscovery: true, include: ['svelte', 'svelte/store'] },
  server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/**'] } },
});
let browser, page;
try {
  await server.listen(); browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL || 'msedge' });
  const context = await browser.newContext({ timezoneId: 'UTC' });
  page = await context.newPage(); const errors = [];
  page.on('pageerror', error => { errors.push(error.message); console.error(error.message); }); page.setDefaultTimeout(15000);
  page.on('response', response => { if (response.status() >= 400) console.error(response.status(), response.url()); });
  const url = server.resolvedUrls.local[0] + '__review?';
  const open = async query => { await page.goto(url + query); await page.waitForFunction(() => window.ready); await page.locator('.counts').waitFor(); };
  const copy = () => page.locator('dialog > footer button[data-tone=primary]');
  let matrixCases = 0;
  for (const lang of ['zh-CN', 'en-US']) for (const theme of ['light', 'dark'])
    for (const size of [{ width: 320, height: 430 }, { width: 480, height: 720 }, { width: 1100, height: 800 }]) for (const scale of [1, 1.5]) {
      await page.setViewportSize(size); await open('lang='+lang+'&theme='+theme+'&scale='+scale);
      assert.equal(await page.locator('.entries article').count(),30);
      assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'no horizontal dialog clipping');
      const rect=await copy().boundingBox(); assert.ok(rect.y>=0&&rect.y+rect.height<=size.height+1,'copy remains reachable');
      await page.screenshot({path:resolve(output,`${lang}-${theme}-${size.width}-${scale}.png`)});matrixCases++;
    }
  await page.setViewportSize({width:480,height:720}); await open('lang=en-US');
  // Golden contract and actual browser DST conversions.
  const summaries=await page.evaluate(()=>['zh-CN','en-US'].map(locale=>window.formatSummary(window.fixture.snapshot,{dates:window.fixture.dates,locale,groupName:locale==='en-US'?'All groups':'全部分组'})));
  assert.deepEqual(summaries,[fixture.expected_summary['zh-CN'],fixture.expected_summary['en-US']]);
  const dstContext=await browser.newContext({timezoneId:'America/New_York'});const dstPage=await dstContext.newPage();
  await dstPage.goto(url+'lang=en-US');await dstPage.waitForFunction(()=>window.ready);
  for(const vector of fixture.date_cases){const range=await dstPage.evaluate(dates=>window.dateRange(dates),vector.dates);assert.deepEqual(range,{start_at:vector.start_at,end_at:vector.end_at});}
  await dstContext.close();
  await page.getByRole('button',{name:'Expand text',exact:true}).click();assert.ok((await page.locator('.entries .body').first().innerText()).includes('Line four'));
  await copy().click();await page.waitForFunction(()=>window.clipboard.length===1);assert.ok((await page.evaluate(()=>window.clipboard[0])).includes('Progress 64'));
  await page.getByRole('button',{name:'Load more',exact:true}).click();await page.waitForFunction(()=>document.querySelectorAll('.entries article').length===60);
  await page.locator('.entries').evaluate(el=>el.scrollTop=700);
  await page.locator('.entries .title').nth(1).click();await page.locator('.progress-dialog[open]').waitFor();
  const position=await page.locator('.work-review-dialog > .entries').evaluate(el=>el.scrollTop);
  assert.equal(await page.locator('.progress-dialog textarea').count(),0,'archive uses readonly progress');
  await page.keyboard.press('Escape');await page.waitForFunction(()=>!document.querySelector('.progress-dialog[open]'));
  assert.equal(await page.locator('.entries').evaluate(el=>el.scrollTop),position);
  await page.evaluate(()=>{window.rows[0].body='Updated progress';window.revision++;window.emit('task-progress-changed');});
  await page.waitForFunction(()=>document.querySelectorAll('.entries article').length===60&&document.querySelector('.entries .body')?.textContent==='Updated progress');
  assert.equal(await page.locator('.entries').evaluate(el=>el.scrollTop),position,'refresh preserves loaded depth and scroll');
  await page.evaluate(()=>window.clipboardFail=true);await copy().click();await page.getByRole('alert').waitFor();
  assert.ok((await page.getByRole('alert').innerText()).includes('clipboard'));assert.equal(await page.evaluate(()=>window.clipboard.length),1);
  await page.evaluate(()=>{window.clipboardFail=false;window.holdCopy=true;});await copy().click();await page.waitForFunction(()=>!!window.releaseCopy);
  await page.getByRole('searchbox').fill('no-match');await page.evaluate(()=>window.releaseCopy());
  await page.waitForFunction(()=>document.querySelector('.empty')?.textContent==='No matching entries');assert.equal(await page.evaluate(()=>window.clipboard.length),1);
  await page.getByRole('searchbox').fill('');await page.locator('.entries article').first().waitFor();
  await page.evaluate(()=>window.holdCopy=true);await copy().click();await page.waitForFunction(()=>!!window.releaseCopy);
  await page.evaluate(()=>{window.beginTargetChange();window.releaseCopy();});await page.getByRole('status').filter({hasText:'sync target'}).waitFor();
  assert.equal(await page.locator('.entries article').count(),0);assert.equal(await page.evaluate(()=>window.clipboard.length),1);
  await page.evaluate(()=>window.endTargetChange());await page.locator('.entries article').first().waitFor();
  await page.getByRole('button',{name:'Custom',exact:true}).click();await page.locator('input[type=date]').first().fill('2026-10-04');await page.locator('input[type=date]').last().fill('2026-10-01');
  await page.getByRole('alert').waitFor();assert.ok(await copy().isDisabled());assert.equal(await page.locator('.entries article').count(),0);
  await open('lang=en-US&empty');assert.equal(await page.locator('.empty').innerText(),'No progress in this period');assert.ok(await copy().isDisabled());
  await page.keyboard.press('Escape');await page.waitForFunction(()=>window.reviewClosed);
  assert.deepEqual(errors,[]);console.log(JSON.stringify({matrixCases,behaviorCases:12,dstCases:2,screenshots:output,errors},null,2));
} catch(error) {
  if(page){console.error(await page.locator('body').innerText());await page.screenshot({path:resolve(output,'failure.png')});}
  console.error(output);throw error;
} finally {await browser?.close();await server.close();}
