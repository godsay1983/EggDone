// Production Svelte template UI with isolated IPC. Never opens a user database.
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
const output=resolve(tmpdir(),'eggdone-template-ui-'+Date.now());mkdirSync(output,{recursive:true});
const native=`export const isTauri=()=>false;
export async function invoke(command,args){
 window.calls.push({command,args});
 if(command==='list_task_templates'){if(window.failLoad)throw Error('offline');return {format_version:1,templates:structuredClone(window.rows)};}
 if(command==='read_task_checklist_editor')return {task:{todo_uuid:args.uuid,title:'Pack',note:'Travel notes',items:{items:[
  {uuid:'a',content:'Ticket',completed:true,sort_order:1000,deleted_at:null},{uuid:'b',content:'Passport',completed:false,sort_order:2000,deleted_at:null}]}},
  fields:{group_uuid:null,priority:1,reminder_at:1000}};
 if(command==='save_task_template'){
  if(window.failSave)throw Error(window.failSave);
  const r=args.request;
  if(window.receipts.has(r.operation_uuid))return structuredClone(window.receipts.get(r.operation_uuid));
  const row={uuid:r.uuid,content:r.content,created_at:1,updated_at:Date.now(),updated_by:'device',deleted_at:r.deleted?Date.now():null};
  window.rows=window.rows.filter(x=>x.uuid!==row.uuid);window.rows.push(row);window.receipts.set(r.operation_uuid,row);
  if(window.loseReply){window.loseReply=false;throw Error('lost');}
  return structuredClone(row);
 }
 throw Error('Unexpected IPC '+command);
}`;
const html=`<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/TaskTemplateDialog.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang')||'en-US');document.documentElement.dataset.theme=p.get('theme')||'light';
document.documentElement.style.zoom=p.get('scale')||'1';window.calls=[];window.used=null;window.closeCount=0;window.failLoad=p.has('failLoad');window.failSave='';
window.receipts=new Map();window.loseReply=false;
window.rows=p.has('empty')?[]:[{uuid:'123e4567-e89b-42d3-a456-000000000001',created_at:1,updated_at:2,updated_by:'device',deleted_at:null,
 content:{name:'Travel / 出行准备',title:'Pack',note:'Travel notes',group_uuid:p.has('missing')?'missing':null,checklist:Array.from({length:p.has('long')?21:2},(_,i)=>i?'Passport':'Ticket')}}];
let panel;window.openPanel=()=>{panel=mount(Dialog,{target:document.body,props:{sourceUuid:p.has('source')?'source':'',groups:[],
 onClose:()=>{window.closeCount++;void unmount(panel);},onUse:draft=>{window.used=draft;void unmount(panel);}}});};window.openPanel();
</script></body></html>`;
const server=await createServer({root,configFile:false,resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'virtual:template-ipc'},{find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser']},
 plugins:[svelte(),{name:'template-ui',resolveId:id=>id==='virtual:template-ipc'?'\0template-ipc':null,load:id=>id==='\0template-ipc'?native:null,
 configureServer(server){server.middlewares.use('/__templates',async(_req,res)=>{res.setHeader('Content-Type','text/html; charset=utf-8');res.end(await server.transformIndexHtml('/__templates',html));});}}],
 optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try{
 await server.listen();browser=await chromium.launch({headless:true,channel:'msedge'});
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>{errors.push(e.message);console.error(e.message);});page.setDefaultTimeout(15000);
 page.on('console',m=>{if(m.type()==='error')console.error(m.text());});
 const go=async(q='')=>page.goto(server.resolvedUrls.local[0]+'__templates?'+q);
 const button=name=>page.getByRole('button',{name,exact:true});
 let count=0;
 for(const lang of ['en-US','zh-CN'])for(const theme of ['light','dark'])for(const size of [{width:320,height:430},{width:480,height:720},{width:1100,height:800}])for(const scale of [1,1.5]){
  await page.setViewportSize(size);await go('lang='+lang+'&theme='+theme+'&scale='+scale);
  try { await page.locator('.template-row').click(); }
  catch(e){console.error(await page.locator('body').innerText());console.error(await page.evaluate(()=>window.calls));throw e;}
  await button(lang==='en-US'?'Close':'关闭').waitFor();
  assert.ok(await page.locator('dialog').evaluate(e=>e.scrollWidth<=e.clientWidth),'no horizontal overflow');
  for(const b of await page.locator('dialog footer button').all()){
   const r=await b.boundingBox();
   if(!(r.x>=-1&&r.y>=0&&r.x+r.width<=size.width+1&&r.y+r.height<=size.height+1)){
    console.error({lang,theme,size,scale,r});await page.screenshot({path:resolve(output,'overflow.png')});console.error(output);
   }
   assert.ok(r.x>=-1&&r.y>=0&&r.x+r.width<=size.width+1&&r.y+r.height<=size.height+1,'footer visible');
  }
  if(size.width===480&&scale===1)await page.screenshot({path:resolve(output,lang+'-'+theme+'.png')});
  await button(lang==='en-US'?'Edit':'编辑').click();
  await page.getByLabel(lang==='en-US'?'Template name':'模板名称',{exact:true}).fill('Reusable task');
  await page.evaluate(()=>window.loseReply=true);
  await button(lang==='en-US'?'Save template':'保存模板').click();await page.locator('[role=alert]').waitFor();
  await button(lang==='en-US'?'Save template':'保存模板').click();await page.locator('[role=status]').waitFor();
  const calls=await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_template'));assert.equal(calls.length,2);assert.deepEqual(calls[0].args,calls[1].args);
  await button(lang==='en-US'?'Use template':'使用模板').click();await page.waitForFunction(()=>window.used!==null);
  assert.equal(await page.evaluate(()=>window.used.items.length),2);
  assert.equal(await page.evaluate(()=>window.used.creation.fields.reminder_at),null);
  assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='create_task_checklist_editor').length),0);count++;
 }
 await page.setViewportSize({width:480,height:720});
 await go('empty');await page.getByText('No templates yet.',{exact:false}).waitFor();
 await go('failLoad');await page.locator('[role=alert]').waitFor();await page.evaluate(()=>window.failLoad=false);
 await button('Reload (discard changes)').click();await page.locator('.template-row').waitFor();
 await page.getByRole('searchbox').fill('not found');assert.equal(await page.locator('.template-row').count(),0);
 await page.getByRole('searchbox').fill('passport');await page.locator('.template-row').click();
 await button('Delete template').click();await page.getByText('Delete this template?',{exact:false}).waitFor();
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_template').length),0);
 await button('Cancel').click();await button('Delete template').click();await button('Confirm delete').click();
 await page.getByText('No templates yet.',{exact:false}).waitFor();
 await go('source&empty');await page.getByLabel('Template name',{exact:true}).waitFor();
 assert.equal(await page.getByLabel('Template name',{exact:true}).inputValue(),'Pack');
 await page.keyboard.press('Escape');await page.waitForFunction(()=>window.closeCount===1);
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='save_task_template').length),0);
 await go('source&empty');await page.getByLabel('Template name',{exact:true}).waitFor();
 await button('Save template').click();await page.locator('[role=status]').waitFor();
 await button('Use template').click();await page.waitForFunction(()=>window.used!==null);
 assert.ok(await page.evaluate(()=>window.used.items.every(i=>!i.completed)));
 await go('missing');await page.locator('.template-row').click();assert.ok(await button('Use template').isDisabled());
 await page.getByLabel('Group',{exact:true}).selectOption('');await button('Use template').click();await page.waitForFunction(()=>window.used!==null);
 await go('long');await page.locator('.template-row').click();await button('Use template').click();await page.locator('[role=alert]').waitFor();
 assert.equal(await page.evaluate(()=>window.used),null);
 await go();await page.locator('.template-row').click();await button('Edit').click();
 await page.evaluate(()=>window.failSave='TEMPLATE_STALE_DRAFT');await button('Save template').click();
 await page.getByText('The template changed or was deleted.',{exact:false}).waitFor();
 assert.equal(await page.getByLabel('Task title',{exact:true}).inputValue(),'Pack');
 assert.deepEqual(errors,[]);console.log(count+' template browser matrix cases plus empty/search/retry/source/cancel/delete/missing-group/limit/conflict passed. Screenshots: '+output);
}finally{await browser?.close();await server.close();}
