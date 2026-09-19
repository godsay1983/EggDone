// Production TodoPanel and planning components with isolated IPC, not native/S3 acceptance.
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
const output = mkdtempSync(resolve(tmpdir(), 'eggdone-daily-plan-ui-'));
const native = `
export const isTauri=()=>false;
export class Channel { constructor(callback){this.onmessage=callback;} }
export async function invoke(command,args){
 window.calls.push({command,args:structuredClone(args)});
 if(command==='get_sync_settings')return structuredClone(window.currentSettings);
 if(command==='get_sync_runtime_state')return {schemaVersion:1,lastAttemptAt:null,lastSuccessAt:null,dirtySince:null,
   dirtyDomains:[],lastResult:'never',lastErrorCode:null,lastErrorMessage:null,pendingAttachmentCount:0,updatedAt:1};
 if(command==='get_note_attachment_cache_stats')return {totalBytes:0,reclaimableBytes:0,protectedBytes:0,fileCount:0,
   reclaimableFileCount:0,protectedFileCount:0,pendingCount:0};
 if(command==='save_sync_settings'){
   if(window.failSave)throw Error('save failed');
   window.currentSettings={...window.makeSettings(args.settings.objectKey),...args.settings};
   return structuredClone(window.currentSettings);
 }
 if(command==='list_todos')return structuredClone(window.rows);
 if(command==='list_groups')return [{id:1,uuid:'work',name:'Work',color:'yellow',sort_order:0,created_at:1,updated_at:1,deleted_at:null}];
 if(command==='list_task_checklist_progress')return [{todo_uuid:'task-2',total:3,completed:1}];
 if(command==='list_notes')return [];
 if(['preview_todo_import','preview_full_backup_import'].includes(command))return structuredClone(window.importPreview);
 if(['confirm_todo_import','confirm_full_backup_import'].includes(command))return {
   added:0,updated:0,unchanged:0,note_added:0,note_updated:0,note_unchanged:0,
   attachment_added:0,attachment_updated:0,attachment_unchanged:0,restored_file_count:0,
 };
 if(command==='list_daily_plans'){
   if(window.holdRead)await new Promise(r=>window.releaseRead=r);
   if(window.failRead)throw Error('read failed');
   return window.snapshot(args.date);
 }
 if(command==='write_daily_plan'){
   const r=args.request;
   if(window.holdWrite)await new Promise(ok=>window.releaseWrite=ok);
   if(window.conflict){window.conflict=false;window.revision++;throw Error('PLAN_CONFLICT');}
   if(window.receipts.has(r.operation_uuid))return window.snapshot(r.plan_date);
   if(r.expected!==String(window.revision))throw Error('PLAN_CONFLICT');
   const entries=window.plans[r.plan_date]||=[];
   const index=entries.findIndex(e=>e.task_uuid===r.task_uuid);
   if(r.action==='remove'&&index>=0)entries.splice(index,1);
   if(r.action==='add'&&index<0)entries.push({task_uuid:r.task_uuid,plan_date:r.plan_date,status:'planned',position:entries.length});
   if(r.action==='up'||r.action==='down'){
     const other=index+(r.action==='up'?-1:1);
     if(index>=0&&other>=0&&other<entries.length)[entries[index],entries[other]]=[entries[other],entries[index]];
   }
   entries.forEach((entry,i)=>entry.position=i);window.revision++;window.receipts.add(r.operation_uuid);
   if(window.loseReply){window.loseReply=false;throw Error('lost reply');}
   return window.snapshot(r.plan_date);
 }
 if(command==='set_todo_completed'){
   if(window.failComplete)throw Error('completion failed');
   const todo=window.rows.find(t=>t.id===args.id);todo.completed=args.completed;
   for(const entries of Object.values(window.plans))for(const entry of entries)if(entry.task_uuid===todo.uuid)entry.status='completed';
   window.revision++;return {updated_todo:structuredClone(todo),created_todo:null};
 }
 throw Error('Unexpected IPC '+command);
}`;
const html = `<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount} from 'svelte';
import Panel from '/src/lib/components/TodoPanel.svelte';
import DataManager from '/src/lib/components/DataManager.svelte';
import SyncSettings from '/src/lib/components/SyncSettings.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import {dailyPlans} from '/src/lib/stores/dailyPlanStore.ts';
import {todos} from '/src/lib/stores/todoStore.ts';
import {syncStatus,savedSyncSettings} from '/src/lib/sync/autoSync.ts';
import '/src/app.css';
const params=new URLSearchParams(location.search);
localStorage.clear();localStorage.setItem('eggdone-theme',params.get('theme')||'light');
await setLanguageMode(params.get('lang')||'en-US');
window.calls=[];window.revision=1;window.receipts=new Set();window.failRead=params.has('failRead');
window.holdRead=params.has('holdRead');window.holdWrite=false;window.loseReply=false;window.conflict=false;
window.dailyPlans=dailyPlans;window.todos=todos;window.syncStatus=syncStatus;
window.savedSyncSettings=savedSyncSettings;
window.makeSettings=objectKey=>({enabled:false,endpoint:'https://example.test',region:'us-east-1',bucket:'fixture',objectKey,
 noteObjectKey:objectKey.replace('todos.json','notes.json'),noteAttachmentObjectKey:objectKey.replace('todos.json','note-attachments.json'),
 noteAssetPrefix:objectKey.replace('todos.json','note-assets/v1/'),pathStyle:true,allowHttp:false,credentialsConfigured:true});
window.currentSettings=window.makeSettings(params.get('key')||'eggdone/todos.json');
const make=(id,title,other={})=>({id,uuid:'task-'+id,title,note:null,group_uuid:null,completed:false,pinned:false,priority:0,
 sort_order:id,created_at:1,updated_at:1,completed_at:null,deleted_at:null,archived_at:null,due_date:null,due_at:null,
 reminder_at:null,repeat_rule:null,repeat_next_due_date:null,repeat_series_uuid:null,...other});
window.rows=[make(1,'Plan without due date / 无截止日期的今日任务 '.repeat(3)),make(2,'Second planned task',{group_uuid:'work',note:'Shared task note'}),
 make(3,'Yesterday unfinished'),make(4,'Due task',{due_date:'2026-09-19'}),make(5,'Completed planned task',{completed:true}),
 make(6,'Add from menu')];
const entry=(id,date,position,status='planned')=>({task_uuid:'task-'+id,plan_date:date,status,position});
window.plans={'2026-09-19':[entry(1,'2026-09-19',0),entry(2,'2026-09-19',1),entry(5,'2026-09-19',2,'completed')],
 '2026-09-18':[entry(3,'2026-09-18',0),entry(1,'2026-09-18',1)]};
window.snapshot=date=>{const d=new Date(date+'T12:00:00');d.setDate(d.getDate()-1);const previous=d.getFullYear()+'-'+String(d.getMonth()+1).padStart(2,'0')+'-'+String(d.getDate()).padStart(2,'0');
 return structuredClone({date,revision:String(window.revision),current:window.plans[date]||[],previous:window.plans[previous]||[]});};
window.importPreview={path:'backup.json',file_name:'backup.json',total:0,added:0,updated:0,unchanged:0,
 note_total:0,note_added:0,note_updated:0,note_unchanged:0,attachment_total:0,recurrence_total:0,
 link_total:0,link_deleted:0,link_metadata_included:false,checklist_total:0,checklist_deleted:0,
 checklist_definition_total:0,checklist_missing_parent_total:0,checklist_metadata_included:false,
 template_total:0,template_deleted:0,template_metadata_included:false,
 planning_metadata_included:!params.has('legacy'),planning_relations:params.has('empty')?0:12,planning_completions:params.has('empty')?0:3,
 attachment_added:0,attachment_updated:0,attachment_unchanged:0,attachment_files_included:false,backup_file_count:0,backup_total_bytes:0};
if(params.has('settings')){
 document.documentElement.dataset.theme=params.get('theme')||'light';
 const backdrop=document.createElement('div');backdrop.className='settings-backdrop';
 const target=document.createElement('section');target.className='settings-card';
 backdrop.append(target);document.body.append(backdrop);mount(SyncSettings,{target});
}else if(params.has('manager')){
 document.documentElement.dataset.theme=params.get('theme')||'light';
 mount(DataManager,{target:document.body,props:{onClose:()=>{},onImported:async()=>{window.imported=true;}}});
}else mount(Panel,{target:document.body});window.ready=true;
</script></body></html>`;

const server = await createServer({ root, configFile: false, publicDir: resolve(root, 'static'), cacheDir: resolve(output, 'vite-cache'),
  resolve: { alias: [{ find: '@tauri-apps/api/core', replacement: 'virtual:daily-plan-ipc' },
    { find: '$lib', replacement: resolve(root, 'src/lib') }], conditions: ['browser'], dedupe: ['svelte'] },
  plugins: [svelte(), { name: 'daily-plan-ui',
    resolveId: id => id === 'virtual:daily-plan-ipc' ? '\0daily-plan-ipc' : null,
    load: id => id === '\0daily-plan-ipc' ? native : null,
    configureServer(s) { s.middlewares.use('/__daily-plan', async (_req, res) => {
      res.setHeader('Content-Type', 'text/html; charset=utf-8');
      res.end(await s.transformIndexHtml('/__daily-plan', html));
    }); },
  }],
  optimizeDeps: { noDiscovery: true, include: ['svelte', 'svelte/store'] },
  server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/**'] } },
});

let browser;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true, channel: 'msedge' });
  const page = await browser.newPage();
  await page.clock.install({ time: new Date(2026, 8, 19, 12) });
  const errors = [];
  page.on('pageerror', error => { errors.push(error.message); console.error(error.message); });
  page.setDefaultTimeout(15000);
  const go = async (query = '') => { await page.goto(server.resolvedUrls.local[0] + '__daily-plan?' + query); await page.waitForFunction(() => window.ready); };
  const button = name => page.getByRole('button', { name, exact: true });
  const menuitem = name => page.getByRole('menuitem', { name, exact: true });
  const openPlanMenu = () => page.locator('[data-plan-uuid="task-2"] .more-button').click();
  const cardStyle = card => card.evaluate(e => {
    const s = getComputedStyle(e), checkbox = getComputedStyle(e.querySelector('.checkbox'));
    return [s.backgroundColor, s.borderRadius, s.padding, s.fontSize, checkbox.width, checkbox.height];
  });
  const planned = () => page.locator('[data-plan-uuid]');
  const writes = () => page.evaluate(() => window.calls.filter(c => c.command === 'write_daily_plan').map(c => c.args.request));
  let cases = 0;
  for (const lang of ['en-US', 'zh-CN']) for (const theme of ['light', 'dark']) for (const width of [320, 480, 1000]) {
    await page.setViewportSize({ width, height: 720 });
    await go('lang=' + lang + '&theme=' + theme);
    const t = (en, zh) => lang === 'en-US' ? en : zh;
    await page.locator('.view-switch button').nth(1).click();
    assert.equal(await button(t('Due / overdue', '到期 / 逾期')).getAttribute('aria-pressed'), 'true');
    await page.locator('[data-todo-id="4"]').waitFor();
    await page.waitForFunction(() => document.querySelectorAll('.todo-item').length === 1);
    assert.equal(await page.locator('.todo-item').count(), 1, 'default Today is unchanged due filter');
    assert.equal(await page.locator('[data-todo-id="4"] .daily-plan-badge').count(), 0);
    const normalStyle = await cardStyle(page.locator('[data-todo-id="4"]'));
    await button(t('Planned', '计划做')).focus();
    await page.keyboard.press('Space');
    await planned().first().waitFor();
    assert.equal(await planned().count(), 2);
    assert.equal(await page.locator('.daily-plan-badge').count(), 2, 'both planned tasks have a badge');
    assert.equal(await page.locator('[data-previous-uuid] .daily-plan-badge').count(), 0);
    assert.equal(await page.locator('[data-previous-uuid]').count(), 1, 'yesterday suggestions deduplicated');
    assert.equal(await page.locator('.completed-plans').getAttribute('open'), null);
    assert.equal(await page.locator('.completed-plans button').count(), 0, 'completion evidence is read-only');
    assert(await page.locator('.daily-plan-list').evaluate(e => e.scrollWidth <= e.clientWidth), 'list overflow');
    assert.deepEqual(await cardStyle(page.locator('[data-todo-id="2"]')), normalStyle, 'planned cards use normal task styling');
    await page.locator('[data-plan-uuid="task-2"]').getByText('Work', {exact:true}).waitFor();
    await page.locator('[data-plan-uuid="task-2"]').getByText('Shared task note', {exact:true}).waitFor();
    assert.match(await page.locator('[data-plan-uuid="task-2"] .checklist-progress').innerText(), /1\/3/);
    await openPlanMenu();
    const planLabels = await page.locator('.actions-menu [role="menuitem"]').allTextContents();
    assert.deepEqual(planLabels.slice(0, 3).map(s => s.trim()), [t('Earlier in plan', '计划内上移'),
      t('Later in plan', '计划内下移'), t("Remove from today's plan", '移出今日计划')]);
    assert.equal(await menuitem(t('Later in plan', '计划内下移')).isEnabled(), false, 'last planned row cannot move down');
    assert(await page.locator('.actions-menu').evaluate(e => {
      const menu = e.getBoundingClientRect(), list = e.closest('.daily-plan-list').getBoundingClientRect();
      return menu.top >= list.top && menu.bottom <= list.bottom && getComputedStyle(e).overflowY === 'auto';
    }), 'complete menu remains inside the list viewport');
    await page.screenshot({ path: resolve(output, `${lang}-${theme}-${width}.png`) });
    await page.locator('.actions-menu [role="menuitem"]').last().scrollIntoViewIfNeeded();
    assert(await page.locator('.actions-menu [role="menuitem"]').last().evaluate(e => {
      const item = e.getBoundingClientRect(), menu = e.closest('.actions-menu').getBoundingClientRect();
      return item.top >= menu.top && item.bottom <= menu.bottom;
    }), 'last ordinary action can be scrolled fully into view');
    for (const control of await page.locator('.daily-plan-list button:visible, .plan-tabs button').all()) {
      const box = await control.boundingBox();
      assert(box.x >= -1 && box.x + box.width <= width + 1, 'control horizontally clipped');
    }
    const before = await page.evaluate(() => structuredClone(window.rows));
    await menuitem(t('Earlier in plan', '计划内上移')).click();
    await page.waitForFunction(() => document.querySelector('[data-plan-uuid]')?.getAttribute('data-plan-uuid') === 'task-2');
    assert.deepEqual(await page.evaluate(() => window.rows), before, 'plan order must not change any task fields');
    await button(t('Plan today', '继续安排今天')).click();
    await page.locator('[data-plan-uuid="task-3"]').waitFor();
    assert.equal(await page.locator('[data-previous-uuid]').count(), 0);
    await page.locator('[data-plan-uuid="task-2"] .checkbox').click();
    await page.waitForFunction(() => !document.querySelector('[data-plan-uuid="task-2"]'));
    assert.equal(await page.locator('.completed-plans').getAttribute('open'), null);
    await page.locator('.completed-plans summary').click();
    assert.equal(await page.locator('.completed-title').count(), 2);
    await page.locator('.view-switch button').first().click();
    assert.equal(await page.locator('[data-todo-id="2"] .daily-plan-badge').count(), 0, 'completion clears badge');
    await page.locator('[data-todo-id="1"] .daily-plan-badge').waitFor();
    await page.locator('[data-todo-id="6"] .more-button').click();
    const normalLabels = await page.locator('.actions-menu [role="menuitem"]').allTextContents();
    assert.deepEqual(planLabels.slice(3).map(s => s.trim() === t('Edit note', '编辑备注') ? t('Add note', '添加备注') : s),
      normalLabels.slice(1), 'planned menu retains every ordinary task action');
    const add = page.getByRole('menuitem', { name: t("Add to today's plan", '加入今日计划'), exact: true });
    await add.focus(); await page.keyboard.press('Space');
    await page.waitForFunction(() => window.plans['2026-09-19'].some(e => e.task_uuid === 'task-6'));
    await page.locator('[data-todo-id="6"] .daily-plan-badge').waitFor();
    await page.locator('[data-todo-id="6"] .more-button').click();
    await page.getByRole('menuitem', { name: t("Remove from today's plan", '移出今日计划'), exact: true }).click();
    await page.waitForFunction(() => !window.plans['2026-09-19'].some(e => e.task_uuid === 'task-6'));
    assert.equal(await page.locator('[data-todo-id="6"] .daily-plan-badge').count(), 0, 'removal clears badge in All');
    const handle = page.locator('[data-todo-id="1"] .drag-handle');
    await handle.scrollIntoViewIfNeeded();
    const box = await handle.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.locator('.todo-item.dragging').waitFor();
    assert(await page.locator('.todo-item.dragging').evaluate(e => {
      const item = e.getBoundingClientRect(), viewport = e.closest('.todo-list').getBoundingClientRect();
      return getComputedStyle(e).transform === 'none' && item.left >= viewport.left && item.right <= viewport.right;
    }), 'dragged card border stays inside the scroll viewport');
    await page.screenshot({ path: resolve(output, `drag-${lang}-${theme}-${width}.png`) });
    await page.mouse.up();
    assert(!(await page.evaluate(() => window.calls)).some(c => ['reorder_todos', 'update_todo_schedule'].includes(c.command)));
    cases++;
  }

  await page.setViewportSize({ width: 480, height: 720 });
  await go(); await page.locator('.view-switch button').nth(1).click(); await button('Planned').click();
  await planned().first().waitFor();
  await page.evaluate(() => { window.failComplete = true; });
  await page.locator('[data-plan-uuid="task-2"] .checkbox').click();
  await page.getByRole('alert').filter({ hasText: 'Could not complete this task.' }).waitFor();
  assert.equal(await planned().count(), 2, 'failed completion must not remove the planned task');
  assert.equal(await page.evaluate(() => window.rows.find(t => t.uuid === 'task-2').completed), false);
  assert.equal(await page.locator('.completed-title').count(), 1, 'failed completion must not create a completion receipt');
  assert.equal(await page.locator('[data-plan-uuid="task-2"] .checkbox').isEnabled(), true, 'failed completion can be retried');
  await page.screenshot({ path: resolve(output, 'completion-error.png') });
  await page.evaluate(() => { window.failComplete = false; });
  await page.locator('[data-plan-uuid="task-2"] .checkbox').click();
  await page.waitForFunction(() => !document.querySelector('[data-plan-uuid="task-2"]'));
  assert.equal(await page.getByRole('alert').count(), 0, 'successful retry clears the completion error');

  await go(); await page.locator('.view-switch button').nth(1).click(); await button('Planned').click();
  await openPlanMenu();
  await page.evaluate(() => { window.loseReply = true; });
  await menuitem('Earlier in plan').click(); await button('Retry same action').waitFor();
  await page.screenshot({ path: resolve(output, 'retry.png') });
  await button('Retry same action').click();
  await page.waitForFunction(() => document.querySelector('[data-plan-uuid]')?.getAttribute('data-plan-uuid') === 'task-2');
  const retries = await writes(); assert.deepEqual(retries[0], retries[1], 'uncertain retry preserves all request fields');
  await page.evaluate(() => { window.conflict = true; });
  await openPlanMenu();
  await menuitem('Later in plan').click();
  await page.getByText('The plan or task changed.', { exact: false }).waitFor();
  assert.equal((await writes()).length, 3, 'conflicts never replay automatically');
  await openPlanMenu();
  await menuitem('Later in plan').click();
  await page.waitForFunction(() => document.querySelector('[data-plan-uuid]')?.getAttribute('data-plan-uuid') === 'task-1');
  const confirmed = await writes(); assert.notEqual(confirmed[2].operation_uuid, confirmed[3].operation_uuid);
  await page.evaluate(() => { window.rows[0].title = 'Refreshed after task change'; return window.todos.refresh(); });
  await page.getByText('Refreshed after task change', { exact: true }).waitFor();
  await page.evaluate(() => {
    window.syncStatus.set({ kind: 'syncing', message: '', updatedAt: 1 });
    window.plans['2026-09-19'] = window.plans['2026-09-19'].filter(e => e.task_uuid !== 'task-1'); window.revision++;
    window.syncStatus.set({ kind: 'synced', message: '', updatedAt: 2 });
  });
  await page.waitForFunction(() => !document.querySelector('[data-plan-uuid="task-1"]'));

  await go('failRead'); await page.locator('.view-switch button').nth(1).click(); await button('Planned').click();
  await page.getByText("Could not refresh today's plan.", { exact: false }).waitFor();
  await page.evaluate(() => { window.failRead = false; }); await button('Retry').click(); await planned().first().waitFor();
  await go('holdRead'); await page.locator('.view-switch button').nth(1).click(); await button('Planned').click();
  await page.getByRole('status').filter({ hasText: 'Loading' }).waitFor();
  await page.evaluate(() => { window.holdRead = false; window.releaseRead(); }); await planned().first().waitFor();

  await page.clock.setFixedTime(new Date(2026, 8, 20, 0, 1));
  await page.evaluate(() => window.dailyPlans.checkDate());
  await page.getByText('The date changed.', { exact: false }).waitFor();
  assert.equal(await planned().count(), 0, 'no automatic migration at midnight');
  assert.equal(await page.locator('[data-previous-uuid]').count(), 2);
  assert.equal((await writes()).length, 0);
  assert.equal(await page.locator('.daily-plan-badge').count(), 0, 'yesterday is not labeled today');
  await page.screenshot({ path: resolve(output, 'rollover.png') });
  let previewCases = 0;
  await page.setViewportSize({ width: 320, height: 720 });
  for (const lang of ['en-US', 'zh-CN']) for (const theme of ['light', 'dark'])
  for (const kind of ['json', 'backup']) for (const state of ['included', 'empty', 'legacy']) {
    await go(`manager&lang=${lang}&theme=${theme}&${state}`);
    const choose = () => page.locator('.data-actions > button').nth(kind === 'json' ? 1 : 4).click();
    await choose();
    const expected = state === 'legacy'
      ? lang === 'zh-CN' ? '不包含今日计划，保留当前安排' : 'No daily planning data. Current plans are preserved.'
      : lang === 'zh-CN' ? `今日计划关系 ${state === 'empty' ? 0 : 12} 条，完成记录 ${state === 'empty' ? 0 : 3} 条`
        : `Daily plan relations: ${state === 'empty' ? 0 : 12}, completion records: ${state === 'empty' ? 0 : 3}`;
    const summary = page.getByText(expected, { exact: true });
    await summary.waitFor(); await summary.scrollIntoViewIfNeeded();
    assert(await page.locator('.data-card').evaluate(e => e.scrollWidth <= e.clientWidth), 'planning import summary must wrap at 320px');
    if (kind === 'json') await page.screenshot({ path: resolve(output, `import-${lang}-${theme}-${state}.png`) });
    assert.equal(await page.evaluate(() => window.calls.filter(c => c.command.startsWith('confirm_')).length), 0);
    await page.locator('.preview-actions button').first().click();
    assert.equal(await page.locator('.import-preview').count(), 0);
    assert.equal(await page.evaluate(() => window.calls.filter(c => c.command.startsWith('confirm_')).length), 0, 'cancel must not apply unseen domains');
    await choose(); await page.getByText(expected, { exact: true }).waitFor();
    await page.locator('.preview-actions button').last().click();
    await page.waitForFunction(() => window.imported);
    assert.deepEqual(await page.evaluate(() => window.calls.filter(c => c.command.startsWith('confirm_'))),
      [{ command: kind === 'json' ? 'confirm_todo_import' : 'confirm_full_backup_import', args: { path: 'backup.json' } }]);
    previewCases++;
  }
  let settingsCases = 0;
  const generated = 'eggdone-spaces/v2/d1998d01-68fb-4f95-9519-b9a0e84eac73/todos.json';
  for (const lang of ['en-US', 'zh-CN']) for (const theme of ['light', 'dark'])
  for (const key of ['eggdone/todos.json', generated, 'custom/todos.json']) {
    await go(`settings&lang=${lang}&theme=${theme}&key=${encodeURIComponent(key)}`);
    const path = page.locator('.sync-advanced .sync-field input').first();
    const editor = page.locator('.sync-advanced input[type="checkbox"]');
    const save = page.locator('.sync-actions button').first();
    await page.locator('.storage-location').waitFor();
    assert.equal(await page.locator('.sync-advanced').getAttribute('open'), null);
    assert.equal(await path.isVisible(), false, 'internal paths hidden from initial setup');
    const expected = key === 'custom/todos.json'
      ? lang === 'zh-CN' ? '自定义路径' : 'Custom path'
      : lang === 'zh-CN' ? '自动管理' : 'Automatically managed';
    assert.equal(await page.locator('.storage-location strong').innerText(), expected);
    await save.click();
    await page.waitForFunction(() => window.calls.some(c => c.command === 'save_sync_settings'));
    assert.equal(await page.evaluate(() => window.currentSettings.objectKey), key, 'ordinary save preserves location');
    await page.locator('.sync-advanced summary').click();
    assert.equal(await path.inputValue(), key);
    assert.equal(await path.getAttribute('readonly'), '');
    await editor.check();
    await path.fill('unsaved/todos.json');
    await editor.uncheck();
    assert.equal(await path.inputValue(), key, 'cancel restores saved path, never defaults');
    await editor.check();
    await path.fill('changed/todos.json');
    await page.evaluate(() => { window.failSave = true; });
    await save.click();
    await page.getByRole('alert').waitFor();
    assert.equal(await path.inputValue(), 'changed/todos.json', 'failed save retains custom draft');
    assert.equal(await editor.isChecked(), true);
    await page.evaluate(() => { window.failSave = false; });
    await save.click();
    await page.waitForFunction(() => window.currentSettings.objectKey === 'changed/todos.json');
    assert.equal(await editor.isChecked(), false, 'successful save returns to read-only');
    assert.equal(await page.locator('.sync-advanced .sync-field input').nth(1).inputValue(), 'changed/notes.json');
    await page.evaluate(generated => {
      window.currentSettings = window.makeSettings(generated);
      window.savedSyncSettings.set(window.currentSettings);
    }, generated);
    assert.equal(await path.inputValue(), generated, 'automatic target updates refresh read-only diagnostics');
    await page.locator('.storage-location').scrollIntoViewIfNeeded();
    await page.screenshot({ path: resolve(output, `settings-${lang}-${theme}-${settingsCases}.png`) });
    assert(await page.locator('.sync-section').evaluate(e => e.scrollWidth <= e.clientWidth), 'settings fit a narrow viewport');
    assert.equal(await page.evaluate(() => window.calls.filter(c => ['sync_now','test_sync_connection'].includes(c.command)).length), 0);
    settingsCases++;
  }
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ cases, previewCases, settingsCases, output, acceptance: 'Mocked IPC browser only; not native database or S3 proof' }));
} finally {
  await browser?.close();
  await server.close();
}
