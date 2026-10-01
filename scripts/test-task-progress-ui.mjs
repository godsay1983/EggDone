// Production Svelte UI with isolated IPC. No native database, cloud or signing access.
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
const output = resolve(tmpdir(), 'eggdone-progress-ui-' + Date.now());
mkdirSync(output, { recursive: true });
const native = `export const isTauri=()=>true;
export async function invoke(command,args){
 window.calls.push({command,args:structuredClone(args)});
 if(command==='preview_todo_import'||command==='preview_full_backup_import')return structuredClone(window.backupPreview);
 if(command==='list_task_note_links')return [];
 if(command==='count_task_progress')return args.taskUuids.map(task_uuid=>({task_uuid,count:window.entries.length}));
 if(command==='list_task_progress'){
   if(window.failLoad)throw Error(window.failLoad);
   const snapshot=window.snapshot(args.cursor);
   if(window.holdRead){window.holdRead=false;await new Promise(resolve=>window.releaseRead=resolve);}
   return snapshot;
 }
 if(command==='dismiss_task_progress_notice'){window.overwritten=false;return;}
 if(command==='write_task_progress'){
   if(window.holdSave){window.holdSave=false;await new Promise(resolve=>window.releaseSave=resolve);}
   const r=args.request,digest=JSON.stringify(r);
   if(window.receipts.has(r.operation_uuid)){
     const receipt=window.receipts.get(r.operation_uuid);if(receipt.digest!==digest)throw Error('PROGRESS_CONFLICT');return structuredClone(receipt.page);
   }
   if(window.failSave)throw Error(window.failSave);
   if(window.readOnly)throw Error('PROGRESS_READ_ONLY');
   const index=window.entries.findIndex(entry=>entry.record.uuid===r.record_uuid);
   if(r.action==='create'){
     if(index>=0)throw Error('PROGRESS_CONFLICT');
     window.entries.unshift({record:{uuid:r.record_uuid,task_uuid:r.task_uuid,body:r.body,created_at:Date.now(),created_by:'desktop',
       updated_at:Date.now(),updated_by:'desktop',clock:1,deleted_at:null},token:'created'});
   }else{
     if(index<0)throw Error('PROGRESS_DELETED');if(window.entries[index].token!==r.expected_record)throw Error('PROGRESS_CONFLICT');
     if(r.action==='delete')window.entries.splice(index,1);
     else{const entry=window.entries[index];entry.record.body=r.body;entry.record.clock++;entry.record.updated_at++;entry.token='updated-'+entry.record.clock;}
   }
   const page=window.snapshot(null);window.receipts.set(r.operation_uuid,{digest,page:structuredClone(page)});
   if(window.loseReply){window.loseReply=false;throw Error('lost response');}return page;
 }
 throw Error('Unexpected IPC '+command);
}`;
const events = `export async function listen(event,callback){window.listeners??=new Map();let set=window.listeners.get(event);if(!set)window.listeners.set(event,set=new Set());set.add(callback);return()=>set.delete(callback);}`;
const html = `<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/TaskProgressDialog.svelte';
import Launcher from '/src/lib/components/TaskProgressLauncher.svelte';
import TodoItem from '/src/lib/components/TodoItem.svelte';
import Host from '/src/lib/components/TaskProgressHost.svelte';
import DataManager from '/src/lib/components/DataManager.svelte';
import ArchiveDialog from '/src/lib/components/ArchiveDialog.svelte';
import {languageState} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search),taskUuid='123e4567-e89b-42d3-a456-000000000001';
languageState.set({mode:p.get('lang')||'en-US',resolvedLocale:p.get('lang')||'en-US'});
document.documentElement.dataset.theme=p.get('theme')||'light';document.documentElement.style.zoom=p.get('scale')||'1';
window.calls=[];window.panelClosed=0;window.changed=0;window.receipts=new Map();window.listeners=new Map();window.failSave='';
window.failLoad=p.has('failLoad')?'PROGRESS_DATABASE':'';window.readOnly=p.has('readOnly');window.overwritten=p.has('notice');window.loseReply=false;
window.entries=Array.from({length:Number(p.get('count')||2)},(_,i)=>({token:'token-'+i,record:{uuid:'123e4567-e89b-42d3-a456-'+String(i+10).padStart(12,'0'),
 task_uuid:taskUuid,body:i===0?'Long text / 长文本 '+('abcdefghij'.repeat(60))+'\\nKeep internal line breaks.':'Entry '+i,
 created_at:1790810400000-i*60000,created_by:'desktop',updated_at:1790810400000-i*60000,updated_by:'desktop',clock:1,deleted_at:null}}));
window.backupPreview={path:'isolated-fixture.json',file_name:'isolated-fixture.json',total:1,added:1,updated:0,unchanged:0,note_total:0,note_added:0,note_updated:0,note_unchanged:0,
 attachment_total:0,recurrence_total:0,link_metadata_included:false,checklist_metadata_included:false,template_metadata_included:false,planning_metadata_included:false,
 attachment_added:0,attachment_updated:0,attachment_unchanged:0,attachment_files_included:false,backup_file_count:0,backup_total_bytes:0,
 ...(p.has('legacy')?{}:p.has('camel')?{progressMetadataIncluded:true,progressTotal:7,progressAdded:2,progressUpdated:3,progressDeleted:1,progressUnchanged:1}:
 {progress_metadata_included:true,progress_total:7,progress_added:2,progress_updated:3,progress_deleted:1,progress_unchanged:1})};
window.snapshot=cursor=>{
 const start=cursor?window.entries.findIndex(e=>e.record.uuid===cursor.uuid)+1:0,entries=structuredClone(window.entries.slice(start,start+30));
 const last=entries.at(-1)?.record;return {task_uuid:taskUuid,title:'Server task title / 任务标题',read_only:window.readOnly,total:window.entries.length,entries,
 next_cursor:start+30<window.entries.length?{created_at:last.created_at,uuid:last.uuid}:null,overwritten:window.overwritten};
};
window.emit=event=>{for(const callback of window.listeners.get(event)||[])callback({event,payload:null});};
window.openPanel=()=>{const component=mount(Dialog,{target:document.body,props:{uuid:taskUuid,title:'Fallback title',readOnly:p.has('forcedReadOnly'),
 onClose:()=>{window.panelClosed++;void unmount(component);},onChanged:()=>window.changed++}});};
if(p.has('rows')){
 mount(Host,{target:document.body});
 const noop=async()=>{};
 for(let i=0;i<12;i++)mount(Launcher,{target:document.body,props:{uuid:i%2?taskUuid:'123e4567-e89b-42d3-a456-000000000002',title:'Task '+i}});
 const row=mount(TodoItem,{target:document.body,props:{todo:{id:1,uuid:taskUuid,title:'Task row',note:null,group_uuid:null,completed:p.has('completed'),
 pinned:false,priority:0,sort_order:0,created_at:1,updated_at:1,completed_at:null,deleted_at:null,archived_at:null,
 due_date:null,due_at:null,reminder_at:null,repeat_rule:null,repeat_next_due_date:null,repeat_series_uuid:null},animationEnabled:false,
 onToggle:noop,onEdit:noop,onNote:noop,onPin:noop,onPriority:noop,onFocus:noop,onSchedule:noop,onSnooze:noop,onGroupChange:noop,
 onDelete:noop,onMove:noop,onDragStart:noop}});window.dropRow=()=>unmount(row);
}else if(p.has('archive')){
 window.archiveClosed=false;mount(ArchiveDialog,{target:document.body,props:{onClose:()=>window.archiveClosed=true,afterCommit:async()=>{},onViewTask:async()=>{},
 initialItem:{expected:{uuid:taskUuid,scope:'isolated',fingerprint:'fixture'},title:'Archived task',content:'Readonly',completed:true,archived_at:1790810400000,
 updated_at:1790810400000,completed_at:1790810400000,due_date:null,due_at:null,group_name:null,checklist_json:'{"items":[]}',links_json:'[]'}}});
}else if(p.has('backup')){
 mount(DataManager,{target:document.body,props:{onClose:()=>{},onImported:async()=>{}}});
}else window.openPanel();
window.ready=true;
</script></body></html>`;
const server = await createServer({ root, configFile: false, cacheDir: resolve(output, 'vite-cache'),
  resolve: { alias: [{ find: '@tauri-apps/api/core', replacement: 'virtual:progress-ipc' },
    { find: '@tauri-apps/api/event', replacement: 'virtual:progress-events' }, { find: '$lib', replacement: resolve(root, 'src/lib') }], conditions: ['browser'] },
  plugins: [svelte(), { name: 'progress-ui',
    resolveId: id => id === 'virtual:progress-ipc' ? '\0progress-ipc' : id === 'virtual:progress-events' ? '\0progress-events' : null,
    load: id => id === '\0progress-ipc' ? native : id === '\0progress-events' ? events : null,
    configureServer(server) { server.middlewares.use('/__progress', async (_req, res) => { res.setHeader('Content-Type', 'text/html; charset=utf-8'); res.end(await server.transformIndexHtml('/__progress', html)); }); },
  }], optimizeDeps: { noDiscovery: true, include: ['svelte', 'svelte/store'] },
  server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/**'] } },
});
let browser, page;
async function assertEditorContrast(page) {
  const colors = await page.evaluate(() => {
    const panel = document.querySelector('.progress-dialog');
    const input = document.querySelector('#progress-body');
    const css = node => getComputedStyle(node);
    const placeholder = getComputedStyle(input, '::placeholder');
    return { panel: css(panel).backgroundColor, label: css(panel.querySelector('label')).color,
      input: css(input).color, background: css(input).backgroundColor, placeholder: placeholder.color,
      placeholderOpacity: placeholder.opacity, counter: css(panel.querySelector('.editor-actions small')).color };
  });
  const luminance = color => color.match(/[\d.]+/g).slice(0, 3).map(Number).map(value => {
    const channel = value / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  }).reduce((total, value, index) => total + value * [0.2126, 0.7152, 0.0722][index], 0);
  for (const [name, foreground, background] of [
    ['label', colors.label, colors.panel], ['input', colors.input, colors.background],
    ['placeholder', colors.placeholder, colors.background], ['counter', colors.counter, colors.panel],
  ]) {
    const a = luminance(foreground), b = luminance(background);
    assert.ok((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05) >= 4.5, `${name} contrast: ${foreground} on ${background}`);
  }
  assert.equal(colors.placeholderOpacity, '1');
}
try {
  await server.listen(); browser = await chromium.launch({ headless: true, channel: 'msedge' });
  page = await browser.newPage(); const errors = []; page.on('pageerror', error => { errors.push(error.message); console.error(error.message); }); page.setDefaultTimeout(15000);
  const url = server.resolvedUrls.local[0] + '__progress?';
  const save = () => page.locator('.editor-actions button[data-tone="primary"]');
  const input = () => page.locator('#progress-body');
  const open = async query => { await page.goto(url + query); await page.waitForFunction(() => window.ready); await page.locator('.progress-dialog[open]').waitFor(); };
  let cases = 0;
  for (const lang of ['zh-CN', 'en-US']) for (const theme of ['light', 'dark'])
    for (const size of [{ width: 320, height: 430 }, { width: 480, height: 720 }, { width: 1100, height: 800 }]) for (const scale of [1, 1.5]) {
      await page.setViewportSize(size); await open('lang=' + lang + '&theme=' + theme + '&scale=' + scale);
      await assertEditorContrast(page);
      await page.locator('.record-body').first().waitFor(); assert.equal(await page.locator('.task-title').innerText(), 'Server task title / 任务标题');
      assert.ok(await save().isDisabled()); await input().fill(' \n\t '); assert.ok(await save().isDisabled());
      await input().fill('plain text \u202e'); assert.ok(await save().isDisabled());
      await input().fill('  Local progress\nNext action  ');
      await page.evaluate(() => window.failSave = 'PROGRESS_DATABASE'); await save().click(); await page.locator('.recovery [role=alert]').waitFor();
      assert.equal(await input().inputValue(), '  Local progress\nNext action  ');
      await page.evaluate(() => window.failSave = ''); await save().click(); await page.waitForFunction(() => window.changed === 1);
      assert.equal(await input().inputValue(), '');
      const writes = await page.evaluate(() => window.calls.filter(c => c.command === 'write_task_progress'));
      assert.deepEqual(writes[0].args.request, writes[1].args.request); assert.equal(writes[1].args.request.body, 'Local progress\nNext action');
      assert.ok(await page.locator('.progress-dialog').evaluate(el => el.scrollWidth <= el.clientWidth), 'no horizontal clipping');
      await page.locator('.entries').evaluate(el => el.scrollTop = el.scrollHeight);
      await page.locator('.record-actions > button').last().click(); const menu = page.locator('.record-menu:popover-open'); await menu.waitFor();
      const rect = await menu.boundingBox(); assert.ok(rect.x >= -1 && rect.y >= 0 && rect.x + rect.width <= size.width + 1 && rect.y + rect.height <= size.height + 1, 'menu within viewport');
      await page.screenshot({ path: resolve(output, `${lang}-${theme}-${size.width}-${scale}.png`) });
      await page.keyboard.press('Escape'); await input().fill('unsaved'); await page.keyboard.press('Escape'); await page.locator('.progress-confirm[open]').waitFor();
      assert.equal(await input().inputValue(), 'unsaved'); await page.locator('.progress-confirm button').first().click(); assert.equal(await input().inputValue(), 'unsaved');
      await page.keyboard.press('Escape'); await page.locator('.progress-confirm button[data-tone=danger]').click(); await page.waitForFunction(() => window.panelClosed === 1);
      cases++;
    }
  await page.setViewportSize({ width: 480, height: 720 });
  for (const lang of ['zh-CN', 'en-US']) for (const theme of ['light', 'dark']) {
    await open(`lang=${lang}&theme=${theme}&count=0`); await assertEditorContrast(page);
    await page.screenshot({ path: resolve(output, `empty-${lang}-${theme}.png`) });
  }
  await open('lang=en-US&theme=dark&count=65&notice'); await page.locator('.record-body').nth(29).waitFor(); assert.equal(await page.locator('.record-body').count(), 30);
  await page.getByRole('button', { name: 'Load more', exact: true }).click(); await page.locator('.record-body').nth(59).waitFor();
  await page.getByRole('button', { name: 'Load more', exact: true }).click(); await page.locator('.record-body').nth(64).waitFor(); assert.equal(await page.locator('.record-body').count(), 65);
  assert.equal((await page.evaluate(() => window.calls.filter(c => c.command === 'list_task_progress')))[1].args.cursor.uuid, '123e4567-e89b-42d3-a456-000000000039');
  await page.getByRole('button', { name: 'Dismiss', exact: true }).click(); assert.equal(await page.locator('.notice').count(), 0);
  await page.locator('.record-actions > button').first().click(); await page.locator('.record-menu').getByRole('menuitem', { name: 'Edit', exact: true }).click();
  await input().fill('Editing draft');
  await page.evaluate(() => { window.entries[0].record.body = 'Remote edit'; window.entries[0].token = 'peer'; window.emit('task-progress-changed'); });
  await page.waitForFunction(() => document.querySelector('.record-body')?.textContent === 'Remote edit'); assert.equal(await input().inputValue(), 'Editing draft');
  await save().click(); await page.getByRole('alert').waitFor(); assert.equal(await input().inputValue(), 'Editing draft');
  await page.getByRole('button', { name: 'Use latest version, keep input', exact: true }).click(); await save().click(); await page.waitForFunction(() => window.changed === 1);
  assert.equal(await input().inputValue(), '');
  await page.locator('.record-actions > button').first().click(); await page.locator('.record-menu').getByRole('menuitem', { name: 'Delete', exact: true }).click();
  await page.locator('.progress-confirm button[data-tone=danger]').click(); await page.waitForFunction(() => window.changed === 2);
  assert.equal(await page.locator('.record-body').count(), 30);
  for (const mode of ['readOnly', 'forcedReadOnly']) {
    await open('lang=en-US&theme=dark&' + mode); await page.locator('.record-body').first().waitFor();
    assert.equal(await page.locator('.record-actions').count(), 0); assert.equal(await page.locator('textarea').count(), 0);
  }
  await open('lang=en-US&theme=light&failLoad'); await page.getByRole('alert').waitFor(); assert.ok(await save().isDisabled());
  await page.evaluate(() => window.failLoad = ''); await page.getByRole('button', { name: 'Refresh entries', exact: true }).click(); await page.locator('.record-body').first().waitFor();
  await input().fill('Lost response input'); await page.evaluate(() => window.loseReply = true); await save().click(); await page.locator('.recovery [role=alert]').waitFor();
  assert.equal(await input().inputValue(), 'Lost response input'); await page.getByRole('button', { name: 'Retry original operation', exact: true }).click(); await page.waitForFunction(() => window.changed === 1);
  assert.equal(await page.evaluate(() => window.entries.filter(e => e.record.body === 'Lost response input').length), 1);
  await open('lang=en-US&theme=dark'); await input().fill('Committed before reply lost'); await page.evaluate(() => window.loseReply = true); await save().click();
  await page.locator('.recovery [role=alert]').waitFor(); await input().fill('Changed ambiguous input'); await save().click();
  await page.waitForFunction(() => document.querySelector('.recovery [role=alert]')?.textContent.includes('entry changed'));
  const changedWrites = await page.evaluate(() => window.calls.filter(c => c.command === 'write_task_progress').map(c => c.args.request));
  assert.equal(changedWrites[0].record_uuid, changedWrites[1].record_uuid); assert.notEqual(changedWrites[0].operation_uuid, changedWrites[1].operation_uuid);
  assert.equal(await input().inputValue(), 'Changed ambiguous input'); assert.equal(await page.evaluate(() => window.entries.length), 3);
  await open('lang=en-US&theme=light'); await input().fill('Held save'); await page.evaluate(()=>window.holdSave=true); await save().click();
  await page.waitForFunction(()=>!!window.releaseSave); await page.keyboard.press('Escape'); assert.equal(await page.evaluate(()=>window.panelClosed),0);
  assert.equal(await page.locator('.progress-dialog').evaluate(el=>el.open),true);
  await page.evaluate(()=>window.releaseSave());await page.waitForFunction(()=>window.changed===1);
  await page.goto(url+'lang=en-US&theme=light&archive');await page.getByRole('button',{name:'Progress entries',exact:true}).click();
  await page.locator('.progress-dialog[open]').waitFor();assert.equal(await page.locator('.progress-dialog textarea').count(),0);
  await page.keyboard.press('Escape');await page.waitForFunction(()=>!document.querySelector('.progress-dialog[open]'));
  assert.equal(await page.evaluate(()=>window.archiveClosed),false);assert.equal(await page.locator('dialog.management-dialog').evaluate(el=>el.open),true);
  await page.keyboard.press('Escape');await page.waitForFunction(()=>window.archiveClosed);
  await page.goto(url + 'lang=en-US&theme=light&rows&completed'); await page.locator('.todo-meta .progress-badge').waitFor();
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === 'count_task_progress').length), 1);
  assert.equal((await page.evaluate(() => window.calls.find(c => c.command === 'count_task_progress'))).args.taskUuids.length, 2);
  await page.locator('.todo-meta .progress-badge').click(); await page.locator('.progress-dialog[open]').waitFor(); await input().fill('Completed result'); assert.ok(await save().isEnabled());
  await page.evaluate(() => window.dropRow()); assert.equal(await input().inputValue(), 'Completed result');
  await page.evaluate(() => { window.entries = []; window.emit('todos-changed'); });
  await page.waitForFunction(() => document.querySelectorAll('.record-body').length === 0); assert.equal(await input().inputValue(), 'Completed result');
  await page.evaluate(() => { window.failLoad = 'PROGRESS_UNAVAILABLE'; window.emit('task-progress-changed'); });
  await page.waitForFunction(() => document.querySelector('#progress-body')?.value === '');
  let previewCases=0;
  for(const lang of ['zh-CN','en-US'])for(const wire of ['snake','camel','legacy'])for(const kind of ['json','backup']){
    await page.goto(url+'backup&lang='+lang+'&'+wire);await page.locator('.data-actions').waitFor();
    await page.locator('.data-actions button').nth(kind==='json'?1:4).click();await page.locator('.import-preview').waitFor();
    const text=await page.locator('.import-preview').innerText();
    assert.ok(wire==='legacy'?text.includes(lang==='en-US'?'existing entries will be kept':'现有记录将保留'):
      text.includes(lang==='en-US'?'Progress entries 7: added 2, updated 3, deleted 1, unchanged 1':'进展记录 7 条：新增 2，更新 3，删除 1，不变 1'));
    assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith('confirm_')).length),0);previewCases++;
  }
  assert.deepEqual(errors, []); console.log(JSON.stringify({ matrixCases: cases, editorContrastCases: cases + 4, behaviorCases: 15, previewCases, screenshots: output, errors }, null, 2));
} catch (error) {
  if (page) { console.error(await page.locator('body').innerText()); await page.screenshot({ path: resolve(output, 'failure.png') }); }
  console.error(output); throw error;
} finally { await browser?.close(); await server.close(); }
