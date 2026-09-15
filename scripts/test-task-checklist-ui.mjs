// Real checklist Svelte dialog with isolated IPC; never opens the user's database.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {mkdirSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {createServer} from 'vite';
import {svelte} from '@sveltejs/vite-plugin-svelte';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_PATH||'playwright');
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const output=resolve(tmpdir(),'eggdone-checklist-ui-'+Date.now());mkdirSync(output,{recursive:true});
const native=`export const isTauri=()=>false;
export async function invoke(command,args){
  window.calls.push({command,args});
  if(command==='read_task_checklist_editor'){
    if(window.failLoad) throw Error('read');
    const uuid='123e4567-e89b-42d3-a456-000000000001';
    return {task:{todo_uuid:uuid,title:'Task',note:'Keep note',updated_at:1,read_only:window.readOnly,
      items:{format_version:1,items:Array.from({length:window.count},(_,i)=>({uuid:'item-'+i,content:'Step '+i,completed:false,sort_order:i*1000,deleted_at:null}))}},
      fields:{due_date:'2026-09-20',due_at:null,reminder_at:5000,group_uuid:null,priority:0,repeat_rule:window.legacy?'daily':null},
      completed:window.completed,repeat_series_uuid:window.recurring?'root':null,definitions:{format_version:1,definitions:[]},next_occurrence_date:'2026-09-21',
      rules:{format_version:1,rules:window.recurring?[{uuid:'old-rule',first_todo_uuid:'root',current_todo_uuid:uuid,current_date:'2026-09-20',generated_count:3,
        exhausted:false,deleted_at:window.stopped?1:null,schedule:{anchor_date:'2026-09-18',frequency:'daily',interval:1,weekdays:[],month_day:null,
        end_type:'count',end_date:null,max_occurrences:5,local_time_minutes:null},timezone_id:null}]:[]}};
  }
  if(command==='save_task_checklist_editor'||command==='create_task_checklist_editor'){if(window.failSave)throw Error(window.failSave);return {updated_at:2,rule_uuid:null,reminder_changed:false};}
  if(command==='resolve_checklist_rule_time')return args.schedule.local_time_minutes===null?null:Date.parse(args.schedule.anchor_date+'T00:00:00Z')+args.schedule.local_time_minutes*60000;
  if(command==='list_task_checklist_progress')return [];
  throw Error('Unexpected IPC '+command);
}`;
const html=`<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/TaskChecklistDialog.svelte';
import {newChecklistDraft} from '/src/lib/utils/taskChecklistCreation.ts';
import TodoItem from '/src/lib/components/TodoItem.svelte';
import {checklistProgress} from '/src/lib/stores/taskChecklistStore.ts';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang'));document.documentElement.dataset.theme=p.get('theme');
document.documentElement.style.zoom=p.get('scale')||'1';
window.calls=[];window.saved=0;window.cancelled=0;window.failLoad=p.has('failLoad');window.failSave='';window.readOnly=p.has('readOnly');window.count=Number(p.get('count')||2);
window.recurring=p.has('recurring');window.completed=p.has('completed');window.stopped=p.has('stopped');window.legacy=p.has('legacy');
window.openPanel=()=>{let dialog=mount(Dialog,{target:document.body,props:{uuid:'123e4567-e89b-42d3-a456-000000000001',
 creation:p.has('create')?newChecklistDraft('123e4567-e89b-42d3-a456-000000000001','',{due_date:'2026-09-21',due_at:null,reminder_at:null,group_uuid:null,priority:0,repeat_rule:null}):null,
 onClose:()=>{window.cancelled++;void unmount(dialog);},onSaved:()=>{window.saved++;void unmount(dialog);}}});};
if(p.has('row')){
 const noop=async()=>{};const uuid='123e4567-e89b-42d3-a456-000000000001';
 checklistProgress.set({[uuid]:{todo_uuid:uuid,total:2,completed:0}});
 mount(TodoItem,{target:document.body,props:{todo:{id:1,uuid,title:'Task with checklist',note:null,group_uuid:'work',
 completed:false,pinned:false,priority:0,sort_order:0,created_at:1,updated_at:1,completed_at:null,deleted_at:null,archived_at:null,
 due_date:null,due_at:null,reminder_at:null,repeat_rule:null,repeat_next_due_date:null,repeat_series_uuid:null},
 groups:[{id:1,uuid:'work',name:'Work',color:'yellow',sort_order:0,created_at:1,updated_at:1,deleted_at:null}],
 animationEnabled:false,onToggle:noop,onEdit:noop,onNote:noop,onPin:noop,onPriority:noop,onFocus:noop,onSchedule:noop,
 onSnooze:noop,onGroupChange:noop,onDelete:noop,onMove:noop,onDragStart:noop}});
}else window.openPanel();
window.ready=true;
</script></body></html>`;
const server=await createServer({root,configFile:false,resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'virtual:checklist-ipc'},{find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser']},
 plugins:[svelte(),{name:'checklist-ui',resolveId:id=>id==='virtual:checklist-ipc'?'\0checklist-ipc':null,load:id=>id==='\0checklist-ipc'?native:null,
 configureServer(server){server.middlewares.use('/__checklist',async(_req,res)=>{res.setHeader('Content-Type','text/html; charset=utf-8');res.end(await server.transformIndexHtml('/__checklist',html));});}}],
 optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},server:{host:'127.0.0.1',port:5191,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try{
 await server.listen();browser=await chromium.launch({headless:true,channel:'msedge'});
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>{errors.push(e.message);console.error(e.message);});page.setDefaultTimeout(15000);
 let cases=0;
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const size of [{width:320,height:430},{width:480,height:720},{width:1100,height:800}])for(const scale of [1,1.5]){
  await page.setViewportSize(size);await page.goto(server.resolvedUrls.local[0]+'__checklist?lang='+lang+'&theme='+theme+'&scale='+scale);
  try { await page.locator('li textarea').first().waitFor(); }
  catch(error) { console.error(await page.locator('body').innerText()); await page.screenshot({path:resolve(output,'failure.png')});console.error(output);throw error; }
  for(const b of await page.locator('dialog footer button').all()){
   const r=await b.boundingBox();assert.ok(r.x>=-1&&r.y>=0&&r.x+r.width<=size.width+1&&r.y+r.height<=size.height+1,'visible footer');
  }
  assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'no horizontal overflow');
  assert.equal(await page.locator('.item-tools').count(),0);
  if(size.width===480&&scale===1)await page.screenshot({path:resolve(output,lang+'-'+theme+'-compact.png')});
  await page.locator('li textarea').first().fill('Updated step');await page.locator('li input[type=checkbox]').first().check();
  await page.evaluate(()=>window.failSave='offline');await page.locator('button[type=submit]').click();await page.locator('[role=alert]').waitFor();
  assert.equal(await page.locator('li textarea').first().inputValue(),'Updated step');
  await page.evaluate(()=>window.failSave='');await page.locator('button[type=submit]').click();await page.waitForFunction(()=>window.saved===1);
  const calls=await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor'));
  assert.equal(calls.length,2);assert.deepEqual(calls[0].args.request,calls[1].args.request);assert.equal(calls[1].args.request.task.items[0].completed,true);
  await page.evaluate(()=>window.openPanel());await page.locator('li textarea').first().waitFor();await page.keyboard.press('Escape');await page.waitForFunction(()=>window.cancelled===1);
  assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').length),2);cases++;
 }
 await page.setViewportSize({width:360,height:640});
 await page.goto(server.resolvedUrls.local[0]+'__checklist?lang=en-US&theme=dark&count=24');
 await page.locator('li').nth(23).waitFor();assert.equal(await page.locator('li').count(),24);
 assert.ok(await page.locator('.section-head button').isDisabled());
 await page.locator('.fields').evaluate(el=>el.scrollTop=el.scrollHeight);
 await page.screenshot({path:resolve(output,'long-dark.png')});
 await page.locator('li:last-child .item-more').click();
 await page.locator('li:last-child .item-tools button').last().click();assert.equal(await page.locator('li').count(),23);
 await page.goto(server.resolvedUrls.local[0]+'__checklist?lang=en-US&theme=light&count=0');
 await page.locator('.section-head button').click();assert.equal(await page.locator('li').count(),1);
 assert.ok(await page.locator('li textarea').evaluate(el=>el===document.activeElement));
 await page.locator('button[type=submit]').click();await page.locator('[role=alert]').waitFor();
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').length),0);
 await page.locator('li textarea').fill('One\nTwo');await page.locator('button[type=submit]').click();
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').length),0);
 await page.locator('li textarea').fill('One');
 await page.evaluate(()=>window.failSave='CHECKLIST_STALE');await page.locator('button[type=submit]').click();
 await page.waitForFunction(()=>document.querySelector('[role=alert]')?.textContent.includes('changed elsewhere'));
 await page.screenshot({path:resolve(output,'conflict-light.png')});
 await page.goto(server.resolvedUrls.local[0]+'__checklist?lang=en-US&theme=dark&readOnly');
 await page.locator('li textarea').first().waitFor();assert.ok(await page.locator('button[type=submit]').isDisabled());
 assert.ok(await page.locator('li textarea').first().isDisabled());
 await page.goto(server.resolvedUrls.local[0]+'__checklist?lang=en-US&theme=dark&failLoad');
 await page.locator('[role=alert]').waitFor();assert.ok(await page.locator('button[type=submit]').isDisabled());
 await page.evaluate(()=>window.failLoad=false);await page.getByRole('button',{name:'Retry',exact:true}).click();await page.locator('li textarea').first().waitFor();
 await page.locator('.task-details summary').click();
 await page.locator('.task-fields input').fill('Changed task');
 await page.locator('.task-fields textarea').fill('Changed note');
 await page.locator('.task-details summary').click();
 assert.equal(await page.locator('.task-fields').isVisible(),false);
 await page.locator('li textarea').first().fill('Long checklist content '.repeat(8));
 await page.setViewportSize({width:320,height:430});
 await page.waitForFunction(()=>{
   const el=document.querySelector('li textarea');return el.scrollHeight<=el.clientHeight+1;
 });
 await page.locator('li .item-more').first().click();
 await page.locator('li .item-tools button').nth(1).click();
 assert.equal(await page.locator('li textarea').first().inputValue(),'Step 1');
 assert.equal(await page.locator('.item-tools').count(),1);
 await page.locator('button[type=submit]').click();await page.waitForFunction(()=>window.saved===1);
 const edited=await page.evaluate(()=>window.calls.find(c=>c.command==='save_task_checklist_editor').args.request.task);
 assert.equal(edited.title,'Changed task');assert.equal(edited.note,'Changed note');
 assert.equal(edited.items[0].content,'Step 1');
 for(const theme of ['light','dark'])for(const size of [{width:320,height:480},{width:620,height:720}])for(const scale of [1,1.5]){
   await page.setViewportSize(size);
   await page.goto(server.resolvedUrls.local[0]+'__checklist?lang=zh-CN&theme='+theme+'&scale='+scale+'&row');
   const badge=page.locator('.todo-meta .checklist-progress');await badge.waitFor();
   const group=await page.locator('.todo-group-badge').boundingBox(),rect=await badge.boundingBox();
   if(size.width>=620||scale===1)assert.ok(Math.abs(rect.y-group.y)<2,'checklist and group share metadata row when space permits');
   else assert.ok(rect.y>=group.y&&rect.x>=0&&rect.x+rect.width<=size.width,'metadata wraps within narrow zoomed window');
   assert.equal(await badge.evaluate(el=>getComputedStyle(el).borderTopWidth),'0px');
   assert.ok(rect.height<=28*scale,'compact checklist badge');
   await page.screenshot({path:resolve(output,'badge-'+theme+'-'+size.width+'.png')});
   await badge.click();await page.locator('dialog[open]').waitFor();
   await page.locator('dialog li textarea').first().waitFor();
   const sizes=await page.evaluate(()=>{
     const font=s=>parseFloat(getComputedStyle(document.querySelector(s)).fontSize);
     return {task:font('.todo-content > p'),summary:font('dialog summary > span'),item:font('dialog .item-content'),
       heading:font('dialog h2'),button:font('dialog footer button'),buttonHeight:parseFloat(getComputedStyle(document.querySelector('dialog footer button')).height)};
   });
   assert.equal(sizes.summary,sizes.task,'same task title size inside and outside dialog');
   assert.equal(sizes.item,sizes.task,'checklist text follows main task text size');
   assert.ok(sizes.heading<=sizes.task+2,'restrained panel heading');
   assert.equal(sizes.button,12);assert.equal(sizes.buttonHeight,32);
   await page.screenshot({path:resolve(output,'density-'+theme+'-'+size.width+'-'+scale+'.png')});
 }
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const width of [320,620]){
   await page.setViewportSize({width,height:640});
   await page.goto(server.resolvedUrls.local[0]+'__checklist?lang='+lang+'&theme='+theme+'&scale=1.5&recurring');
   const choices=page.locator('.scope-choices input');await choices.first().waitFor();
   assert.ok(await choices.first().isChecked());await choices.last().check();
   await page.locator('li input[type=checkbox]').first().check();
   await page.evaluate(()=>window.failSave='offline');await page.locator('button[type=submit]').click();await page.locator('[role=alert]').waitFor();
   assert.ok(await choices.last().isChecked());
   assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth));
   await page.screenshot({path:resolve(output,'scope-'+lang+'-'+theme+'-'+width+'.png')});
   await page.evaluate(()=>window.failSave='');await page.locator('button[type=submit]').click();await page.waitForFunction(()=>window.saved===1);
   const requests=await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').map(c=>c.args.request));
   assert.deepEqual(requests[0],requests[1]);const r=requests[1];
   assert.equal(r.mode,'replace');assert.equal(r.replaces_uuid,'old-rule');
   assert.equal(r.replacement.schedule.anchor_date,'2026-09-20');assert.equal(r.replacement.schedule.max_occurrences,3);
   assert.equal(r.fields.reminder_at,5000);assert.equal(r.fields.due_date,'2026-09-20');
   assert.equal(r.future_entries.length,2);assert.equal('completed' in r.future_entries[0],false);
   await page.evaluate(()=>window.openPanel());await page.locator('.scope-choices input').last().check();
   await page.keyboard.press('Escape');await page.waitForFunction(()=>window.cancelled===1);
   assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').length),2);
 }
 for(const flag of ['completed','stopped','legacy']){
   await page.goto(server.resolvedUrls.local[0]+'__checklist?lang=en-US&theme=dark&recurring&'+flag);
   await page.locator('.scope-choices input').last().waitFor();assert.ok(await page.locator('.scope-choices input').last().isDisabled());
 }
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const width of [320,620]){
   await page.setViewportSize({width,height:640});
   await page.goto(server.resolvedUrls.local[0]+'__checklist?lang='+lang+'&theme='+theme+'&scale=1.5');
   await page.locator('.task-settings summary').click();
   await page.locator('.task-settings input[type=date]').fill('2026-10-02');
   await page.locator('.task-settings input[type=time]').fill('16:30');
   await page.locator('.task-settings input[type=datetime-local]').fill('2099-10-02T16:00');
   await page.locator('.task-settings input[type=checkbox]').check();
   await page.locator('.task-settings select').last().selectOption('weekdays');
   await page.locator('li textarea').first().fill('Full task step');
   await page.evaluate(()=>window.failSave='offline');await page.locator('button[type=submit]').click();await page.locator('[role=alert]').waitFor();
   assert.equal(await page.locator('.task-settings select').last().inputValue(),'weekdays');
   assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth));
   await page.locator('.task-settings summary').scrollIntoViewIfNeeded();
   await page.screenshot({path:resolve(output,'settings-'+lang+'-'+theme+'-'+width+'.png')});
   await page.evaluate(()=>window.failSave='');await page.locator('button[type=submit]').click();await page.waitForFunction(()=>window.saved===1);
   const requests=await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').map(c=>c.args.request));
   assert.deepEqual(requests[0],requests[1]);const r=requests[1];
   assert.equal(r.mode,'replace');assert.equal(r.fields.priority,1);assert.equal(r.fields.repeat_rule,null);
   assert.equal(r.replacement.schedule.local_time_minutes,990);assert.equal(r.task.items[0].content,'Full task step');
 }
 await page.goto(server.resolvedUrls.local[0]+'__checklist?lang=en-US&theme=dark');
 await page.locator('.task-settings summary').click();await page.locator('.task-settings select').last().selectOption('custom');
 await page.getByRole('button',{name:'Configure custom recurrence',exact:true}).click();
 await page.locator('.recurrence-editor input[type=number]').first().fill('2');
 await page.keyboard.press('Escape');assert.equal(await page.locator('.recurrence-editor').count(),0);
 await page.locator('button[type=submit]').click();await page.locator('[role=alert]').waitFor();
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').length),0);
 await page.getByRole('button',{name:'Configure custom recurrence',exact:true}).click();
 await page.locator('.recurrence-editor input[type=number]').first().fill('2');
 await page.locator('.recurrence-editor button[type=submit]').click();
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_recurrence_rule').length),0);
 await page.keyboard.press('Escape');await page.waitForFunction(()=>window.cancelled===1);
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').length),0);
 await page.goto(server.resolvedUrls.local[0]+'__checklist?lang=en-US&theme=dark&legacy');
 await page.locator('.scope-choices input').last().check();await page.locator('button[type=submit]').click();await page.waitForFunction(()=>window.saved===1);
 assert.equal(await page.evaluate(()=>window.calls.find(c=>c.command==='save_task_checklist_editor').args.request.fields.repeat_rule),null);
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const width of [320,620]){
   await page.setViewportSize({width,height:640});
   await page.goto(server.resolvedUrls.local[0]+'__checklist?create&lang='+lang+'&theme='+theme+'&scale=1.5');
   await page.locator('.task-fields input').waitFor();
   assert.equal(await page.evaluate(()=>window.calls.length),0,'new draft never loads a parent');
   await page.locator('button[type=submit]').click();await page.locator('[role=alert]').waitFor();
   assert.equal(await page.evaluate(()=>window.calls.length),0,'blank title never creates a task');
   await page.locator('.task-fields input').fill('Creation preview');
   await page.locator('.section-head button').click();
   assert.ok(await page.locator('li textarea').evaluate(el=>el===document.activeElement));
   await page.locator('button[type=submit]').click();
   assert.equal(await page.evaluate(()=>window.calls.length),0,'blank child never writes');
   await page.locator('li textarea').fill('First step');
   assert.ok(await page.locator('li input[type=checkbox]').isDisabled());
   await page.locator('.section-head button').click();await page.locator('li textarea').last().fill('Second step');
   await page.locator('li .item-more').last().click();await page.locator('li .item-tools button').first().click();
   assert.equal(await page.locator('li textarea').first().inputValue(),'Second step');
   await page.locator('.task-settings summary').click();await page.locator('.task-settings select').last().selectOption('daily');
   assert.equal(await page.locator('.task-settings option[value=keep]').count(),0);
   await page.locator('.task-settings summary').click();
   await page.evaluate(()=>window.failSave='offline');await page.locator('button[type=submit]').click();await page.locator('[role=alert]').waitFor();
   assert.equal(await page.locator('.task-fields input').inputValue(),'Creation preview');
   assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth));
   for(const b of await page.locator('dialog footer button').all()){
     const r=await b.boundingBox();assert.ok(r.x>=0&&r.y>=0&&r.x+r.width<=width+1&&r.y+r.height<=641);
   }
   await page.screenshot({path:resolve(output,'create-'+lang+'-'+theme+'-'+width+'.png')});
   await page.evaluate(()=>window.failSave='');await page.locator('button[type=submit]').click();await page.waitForFunction(()=>window.saved===1);
   const requests=await page.evaluate(()=>window.calls.filter(c=>c.command==='create_task_checklist_editor').map(c=>c.args.request));
   assert.equal(requests.length,2);assert.deepEqual(requests[0],requests[1]);assert.equal(requests[1].task.expected_updated_at,0);
   assert.equal(requests[1].task.items[0].content,'Second step');assert.equal(requests[1].mode,'replace');
   assert.deepEqual(requests[1].future_entries.map(e=>e.content),['Second step','First step']);
   assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_checklist_editor').length),0);
   await page.evaluate(()=>window.openPanel());await page.locator('.task-fields input').waitFor();
   await page.locator('.task-fields input').fill('Discard');await page.keyboard.press('Escape');await page.waitForFunction(()=>window.cancelled===1);
   assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='create_task_checklist_editor').length),2);
 }
 assert.deepEqual(errors,[]);console.log('Checklist UI: '+cases+' baseline, 8 density, 8 scope, 8 full-settings and 8 creation matrices; draft-only cancel, validation and stable retries passed. Screenshots: '+output);
}finally{await browser?.close();await server.close();}
