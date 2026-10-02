// Production components with isolated IPC and synthetic data; no native app or cloud access.
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
const output = resolve(tmpdir(), 'eggdone-calendar-todo-ui-' + Date.now()); mkdirSync(output, { recursive: true });
const fixture = JSON.parse(readFileSync(resolve(root, 'docs/fixtures/calendar-todo-v1.json'), 'utf8'));
const fixtureScript = JSON.stringify(fixture).replace(/[\u0080-\uffff<]/g, c => '\\u' + c.charCodeAt(0).toString(16).padStart(4, '0'));
const native = `export const isTauri=()=>true;export async function invoke(command,args){
  window.calls.push({command,args:structuredClone(args)});
  if(command==='get_system_calendar_state'||command==='refresh_system_calendar')return structuredClone(window.calendarState);
  if(command==='list_todos')return structuredClone(window.saved);
  if(command==='list_groups')return [{uuid:'11111111-1111-4111-8111-111111111111',name:'Work',deleted_at:null}];
  if(command==='create_calendar_todo'){
    if(window.holdSave)await new Promise(r=>window.releaseSave=r);
    if(window.createError)throw Error(window.createError);
    const draft=args.draft;
    const existing=window.saved.find(t=>t.uuid===draft.uuid);
    if(existing)return {todo:structuredClone(existing),created:false};
    const todo={...draft,id:window.saved.length+1,completed:false,pinned:false,priority:0,sort_order:0,
      created_at:Date.now(),updated_at:Date.now(),completed_at:null,deleted_at:null,archived_at:null,
      due_at:null,due_date:null,reminder_at:null,repeat_rule:null,repeat_next_due_date:null,repeat_series_uuid:null};
    window.saved.push(todo);if(window.loseReply)throw Error('reply lost');return {todo:structuredClone(todo),created:true};
  }
  if(command==='resolve_calendar_todo'){
    if(window.resolveError)throw Error(window.resolveError);
    const todo=window.saved.find(t=>t.uuid===args.uuid);return todo?{todo:structuredClone(todo),created:false}:null;
  }
  throw Error('Unexpected IPC '+command);
}`;
const html = `<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount} from 'svelte';import Calendar from '/src/lib/components/SystemCalendar.svelte';
import {languageState} from '/src/lib/i18n/index.ts';import {systemCalendar} from '/src/lib/stores/systemCalendarStore.ts';
import {todos} from '/src/lib/stores/todoStore.ts';import {get} from 'svelte/store';import '/src/app.css';
const fixture=${fixtureScript},p=new URLSearchParams(location.search);
languageState.set({mode:p.get('lang')||'en-US',resolvedLocale:p.get('lang')||'en-US'});
document.documentElement.dataset.theme=p.get('theme')||'light';document.documentElement.style.zoom=p.get('scale')||'1';
window.calls=[];window.saved=[];window.syncCalls=0;window.viewed=null;
const occurrence=structuredClone(fixture.cases[0].occurrence);
window.calendarState={configured:true,document:{format_version:1,owner_id:'private',owner_generation:'private',revision:1,
  operation_id:'private',state:'active',captured_at:Date.now(),source_timezone:'UTC',calendars:[{id:occurrence.calendarId,title:'Work'}],
  coverage:{start:'2026-10-01',end:'2026-10-05',start_time:Date.UTC(2026,9,1),end_time:Date.UTC(2026,9,5)},occurrences:[occurrence]},
  loading:false,error:'',last_received_at:Date.now(),hydrated:true};
window.todoItems=()=>get(todos).items;
window.withdraw=async()=>{window.calendarState.document.state='withdrawn';await systemCalendar.refresh();};
window.changeTarget=()=>systemCalendar.beginSettingsUpdate();
window.outside=async()=>{window.calendarState.document.coverage=null;await systemCalendar.refresh();};
systemCalendar.configure({endpoint:'synthetic',bucket:'synthetic',region:'synthetic',objectKey:'synthetic',enabled:true,credentialsConfigured:true});
await systemCalendar.rehydrate();
await todos.load();
mount(Calendar,{target:document.body,props:{dates:['2026-10-02'],now:Date.now(),onViewTodo:uuid=>window.viewed=uuid}});window.ready=true;
</script></body></html>`;
const server = await createServer({ root, configFile: false, cacheDir: resolve(output, 'vite-cache'),
  resolve: { alias: [{find:'@tauri-apps/api/core',replacement:'virtual:calendar-ipc'},
    {find:'$lib/sync/autoSync',replacement:'virtual:calendar-sync'}, {find:'$lib',replacement:resolve(root,'src/lib')}], conditions:['browser'] },
  plugins: [svelte(), {name:'calendar-todo-harness',
    resolveId:id=>id==='virtual:calendar-ipc'?'\0calendar-ipc':id==='virtual:calendar-sync'?'\0calendar-sync':null,
    load:id=>id==='\0calendar-ipc'?native:id==='\0calendar-sync'?'export function scheduleAutoSync(){window.syncCalls++;}':null,
    configureServer(server){server.middlewares.use('/__calendar-todo',async(_req,res)=>{res.setHeader('Content-Type','text/html; charset=utf-8');res.end(await server.transformIndexHtml('/__calendar-todo',html));});},
  }], optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}} });
let browser;
try {
  await server.listen();browser=await chromium.launch({headless:true,channel:process.env.PLAYWRIGHT_CHANNEL||'msedge'});
  const context=await browser.newContext({timezoneId:'UTC'}),page=await context.newPage(),errors=[];
  page.on('pageerror',error=>{errors.push(error.message);console.error(error.message);});page.setDefaultTimeout(15000);
  page.on('console',message=>{if(message.type()==='error')console.error(message.text());});
  page.on('response',response=>{if(response.status()>=400)console.error(response.status(),response.url());});
  const url=server.resolvedUrls.local[0]+'__calendar-todo?';
  const open=async(query='')=>{await page.goto(url+query);await page.waitForFunction(()=>window.ready);};
  const draft=async()=>{await page.locator('.event-toggle').click();await page.locator('.create-todo').click();await page.locator('dialog input').waitFor();};
  const save=()=>page.locator('dialog footer button[type=submit]');
  let layouts=0;
  for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const size of [{width:320,height:430},{width:480,height:720},{width:1100,height:800}])for(const scale of [1,1.5]){
    await page.setViewportSize(size);await open('lang='+lang+'&theme='+theme+'&scale='+scale);await draft();
    assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'no horizontal clipping');
    const rect=await save().boundingBox();assert.ok(rect.y>=0&&rect.y+rect.height<=size.height+1,'save reachable '+JSON.stringify({lang,theme,size,scale,rect}));
    if(scale===1&&size.width===480)await page.screenshot({path:resolve(output,lang+'-'+theme+'.png')});layouts++;
  }
  await page.setViewportSize({width:480,height:720});await open();await draft();
  assert.equal(await page.locator('dialog select').inputValue(),'');
  await page.locator('dialog footer button[type=button]').click();assert.equal(await page.evaluate(()=>window.saved.length),0);
  await page.locator('.create-todo').click();await page.locator('dialog textarea').fill('😀'.repeat(501));assert.equal(await save().isDisabled(),true);
  await page.locator('dialog textarea').fill('Edited plain text <b>not markup</b>');await page.locator('dialog input').fill('明天 follow-up');
  await page.locator('dialog select').selectOption('11111111-1111-4111-8111-111111111111');
  await page.evaluate(()=>{window.loseReply=true;});await save().click();await page.locator('.saved-title').waitFor();
  assert.equal(await page.locator('.saved-title').textContent(),'明天 follow-up');
  assert.deepEqual(await page.evaluate(()=>({saved:window.saved.length,visible:window.todoItems().length,sync:window.syncCalls})),{saved:1,visible:1,sync:1});
  const saved=await page.evaluate(()=>window.saved[0]);assert.equal(saved.due_at,null);assert.equal(saved.due_date,null);assert.equal(saved.reminder_at,null);assert.equal(saved.repeat_rule,null);
  assert.equal(saved.group_uuid,'11111111-1111-4111-8111-111111111111');assert.equal(saved.note,'Edited plain text <b>not markup</b>');
  await page.locator('dialog footer button[data-tone=primary]').click();assert.equal(await page.evaluate(()=>window.viewed),saved.uuid);
  await page.locator('.create-todo').click();await save().click();await page.locator('.saved-title').waitFor();assert.equal(await page.evaluate(()=>window.saved.length),2);
  await open();await draft();await page.evaluate(()=>{window.createError='transient';window.resolveError='transient';});await save().click();
  await page.getByRole('button',{name:'Check Save Result'}).waitFor();assert.equal(await page.locator('dialog input').isDisabled(),true);
  await page.evaluate(()=>{window.createError=null;window.resolveError=null;});await save().click();assert.equal(await page.locator('dialog input').isDisabled(),false);
  await page.evaluate(()=>window.withdraw());assert.equal(await page.locator('.create-todo').count(),0);assert.equal(await page.locator('dialog input').inputValue(),'明天 项目会议');
  await save().click();await page.locator('.saved-title').waitFor();assert.equal(await page.evaluate(()=>window.saved.length),1);
  await open();await page.locator('.event-toggle').click();await page.evaluate(()=>window.changeTarget());assert.equal(await page.locator('.create-todo').count(),0);
  await open();await page.evaluate(()=>window.outside());await page.locator('.event-toggle').click();assert.equal(await page.locator('.create-todo').count(),0);
  await open();await draft();await page.evaluate(()=>{window.holdSave=true;});await save().click();assert.equal(await save().isDisabled(),true);assert.equal(await page.locator('dialog input').isDisabled(),true);
  await page.evaluate(()=>{window.holdSave=false;window.releaseSave();});await page.locator('.saved-title').waitFor();assert.equal(await page.evaluate(()=>window.saved.length),1);
  assert.deepEqual(errors,[]);console.log(JSON.stringify({layouts,behavior:'cancel, UTF16, lost reply, view, repeated new draft, uncertain result, withdrawal, target switch, coverage, duplicate click',errors,output}));
} finally {if(browser)await browser.close();await server.close();}
