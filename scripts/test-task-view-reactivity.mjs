// Production task views with isolated IPC. No native app, user database or cloud.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = mkdtempSync(resolve(tmpdir(), 'eggdone-task-view-reactivity-'));
const native = `
export const isTauri = () => false;
export class Channel { constructor(callback) { this.onmessage = callback; } }
export async function invoke(command, args) {
  window.calls.push({ command, args });
  if (command === 'list_todos') return structuredClone(window.rows);
  if (['list_groups', 'list_notes', 'list_task_checklist_progress'].includes(command)) return [];
  if (command === 'list_daily_plans') return {date:args.date,revision:'0',current:[],previous:[]};
  if (command === 'list_task_workflow') return {date:args.date,revision:'0',entries:[]};
  if (command === 'recurrence_editor_context') return {rules:[],device_id:'fixture'};
  if (command === 'get_sync_settings') return {enabled:false,credentialsConfigured:false};
  if (command === 'get_sync_runtime_state') return {schemaVersion:1,dirtyDomains:[],lastResult:'never',pendingAttachmentCount:0};
  if (['get_system_calendar_state','refresh_system_calendar'].includes(command))
    return {configured:false,document:null,last_received_at:null,error:''};
  if (command === 'set_todo_completed') {
    if (window.failComplete) throw Error('synthetic completion failure');
    const todo = window.rows.find(t => t.id === args.id);
    todo.completed = args.completed;
    todo.completed_at = args.completed ? Date.now() : null;
    return {updated_todo:structuredClone(todo),created_todo:null};
  }
  throw Error('Unexpected synthetic IPC: ' + command);
}`;
const html = `<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount} from 'svelte';
import Panel from '/src/lib/components/TodoPanel.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import {todos} from '/src/lib/stores/todoStore.ts';
import '/src/app.css';
const p = new URLSearchParams(location.search);
localStorage.clear();
localStorage.setItem('eggdone-theme', 'dark');
localStorage.setItem('eggdone-show-completed', p.has('hidden') ? 'false' : 'true');
await setLanguageMode('en-US');
window.calls = []; window.todos = todos;
const make = (id, extra={}) => ({id,uuid:'task-'+id,title:'Task '+id,note:null,group_uuid:null,
  completed:false,pinned:false,priority:0,sort_order:id,created_at:1,updated_at:1,completed_at:null,
  deleted_at:null,archived_at:null,due_date:null,due_at:null,reminder_at:null,repeat_rule:null,
  repeat_next_due_date:null,repeat_series_uuid:null,...extra});
window.rows = [make(1),make(2,{due_date:'2026-09-20'}),make(3,{due_date:'2026-09-19',priority:1}),
  make(4,{due_date:'2026-09-21'})];
mount(Panel, {target:document.body}); window.ready = true;
</script></body></html>`;
const server = await createServer({root,configFile:false,publicDir:resolve(root,'static'),cacheDir:resolve(output,'vite-cache'),
  resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'virtual:task-view-ipc'},
    {find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser'],dedupe:['svelte']},
  plugins:[svelte(),{name:'task-view-reactivity',
    resolveId:id=>id==='virtual:task-view-ipc'?'\0task-view-ipc':null,
    load:id=>id==='\0task-view-ipc'?native:null,
    configureServer(s){s.middlewares.use('/__task-views',async(_req,res)=>{
      res.setHeader('Content-Type','text/html; charset=utf-8');
      res.end(await s.transformIndexHtml('/__task-views',html));
    });}}],
  optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},
  server:{host:'127.0.0.1',port:0,watch:null}});

let browser;
try {
  await server.listen();
  browser = await chromium.launch({headless:true,channel:'msedge'});
  const page = await browser.newPage({viewport:{width:480,height:900},reducedMotion:'reduce',timezoneId:'Asia/Shanghai'});
  await page.route('**/*',route=>new URL(route.request().url()).hostname==='127.0.0.1'?route.continue():route.abort());
  await page.clock.setFixedTime(new Date('2026-09-20T12:00:00+08:00'));
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on('pageerror',error=>errors.push(error.message));
  const card = id => page.locator('[data-todo-id="'+id+'"]');
  async function go(view, hidden=false) {
    await page.goto(server.resolvedUrls.local[0]+'__task-views?'+(hidden?'hidden':''));
    await page.waitForFunction(()=>window.ready);
    await card(2).waitFor();
    if(view!=='all') await page.locator('.view-switch button').nth(view==='quadrants'?2:3).click();
    if(view==='date') await page.locator('.agenda-week-strip button').first().click();
    await card(2).waitFor();
  }
  async function assertCompleted(id, completed) {
    await page.waitForFunction(({id,completed})=>{
      const e=document.querySelector('[data-todo-id="'+id+'"]');
      return e?.classList.contains('completed')===completed &&
        e.querySelector('.checkbox').classList.contains('checked')===completed;
    },{id,completed},{timeout:1500});
  }
  for(const view of ['all','quadrants','calendar','date']) for(const hidden of [false,true]) {
    await go(view,hidden);
    await page.evaluate(()=>{window.failComplete=true;});
    await card(2).locator('.checkbox').click();
    await page.getByRole('alert').filter({hasText:'synthetic completion failure'}).waitFor();
    await assertCompleted(2,false);
    await page.evaluate(()=>{window.failComplete=false;});
    await card(2).locator('.checkbox').click();
    await page.waitForFunction(()=>window.rows.find(t=>t.id===2).completed);
    if(hidden) {
      await card(2).waitFor({state:'detached',timeout:1500});
      if(view==='date') assert.equal(await page.locator('.selected-date header small').innerText(),'0');
      if(view==='calendar'||view==='date') assert.equal(await page.locator('.agenda-week-strip button').first().locator('small').innerText(),'0');
    } else {
      await assertCompleted(2,true);
      await card(2).locator('.checkbox').click();
      await assertCompleted(2,false);
    }
    assert.deepEqual(await page.evaluate(()=>window.calls.filter(c=>c.command==='set_todo_completed').map(c=>c.args.completed)),
      hidden?[true,true]:[true,true,false]);
    console.log('PASS immediate completion, failure/retry and '+(hidden?'hidden':'visible')+' completed: '+view);
  }
  await go('quadrants');
  await page.evaluate(()=>{window.rows[0].priority=1;return window.todos.refresh();});
  await page.waitForFunction(()=>document.querySelector('[data-todo-id="1"]')?.closest('.quadrant-section')?.classList.contains('gold'));
  await page.screenshot({path:resolve(output,'quadrants.png')});
  await go('date');
  await page.evaluate(()=>{window.rows[1].due_date='2026-09-21';return window.todos.refresh();});
  await card(2).waitFor({state:'detached',timeout:1500});
  assert.equal(await page.locator('.agenda-week-strip button').nth(1).locator('small').innerText(),'2');
  await page.locator('.agenda-week-strip button').nth(1).click();
  await card(2).waitFor();
  await page.evaluate(()=>{window.rows[1].title='Refreshed task';return window.todos.refresh();});
  await card(2).getByText('Refreshed task',{exact:true}).waitFor({timeout:1500});
  await page.screenshot({path:resolve(output,'calendar.png')});
  assert.deepEqual(errors,[]);
  console.log('PASS updated grouping, selected-date rows and week counts without view switching');
  console.log('Screenshots: '+output);
} finally {
  await browser?.close();
  await server.close();
}
