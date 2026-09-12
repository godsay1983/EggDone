// Production Svelte components and workflow with an isolated IPC substitute, not native Tauri acceptance.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_PATH||'playwright');
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const output=process.env.LINK_UI_OUTPUT||resolve(tmpdir(),'eggdone-link-ui-'+Date.now()); mkdirSync(output,{recursive:true});
const native=`export const isTauri=()=>false;
export async function invoke(command,args){
  window.calls.push({command,args});
  if(command==='list_task_note_links') {
    if(window.failLoad) throw Error('read failure');
    return ['active','completed','archived','deleted','missing'].map((state,i)=>({
      link:{uuid:'link-'+i},todo_title:i>2?null:'Linked task with a very long title '.repeat(3),
      todo_state:state,note_state:'active',is_repeating:i===2}));
  }
  if(command==='create_linked_todo') {if(window.failSave) throw Error('write failure'); return {uuid:'link'};}
  throw Error('Unexpected IPC '+command);
}`;
const html=`<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount,unmount} from 'svelte';
import Editor from '/src/lib/components/NoteEditor.svelte';
import Dialog from '/src/lib/components/LinkedTodoDialog.svelte';
import {createTaskNoteLinkStore} from '/src/lib/stores/taskNoteLinkStore.ts';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search); setLanguageMode(p.get('lang')); document.documentElement.dataset.theme=p.get('theme');
window.calls=[];window.cancelled=0;window.saved=0;window.failSave=false;window.failLoad=false;
const workflow=createTaskNoteLinkStore();
const note={uuid:'123e4567-e89b-42d3-a456-426614174001',title:'Source note',content:'Keep this content',color:'default',pinned:false};
let dialog;
const noop=async()=>{};
mount(Editor,{target:document.body,props:{note,onChange:()=>{},onDone:noop,onPin:noop,onColor:noop,onDelete:noop,
 onAddImages:noop,onAddFiles:noop,onOpenAttachment:noop,onOpenFile:noop,onMoveAttachment:noop,onDeleteAttachment:noop,onRetryAttachment:noop,
 onCreateLinked:()=>{dialog=mount(Dialog,{target:document.body,props:{noteUuid:note.uuid,initialTitle:note.title,
 onCancel:()=>{window.cancelled++;void unmount(dialog);},onSave:async draft=>{
   await workflow.create(draft,noop,async()=>{window.saved++;await unmount(dialog);});
 }}});}
}});
window.ready=true;
</script><style>body{display:flex;flex-direction:column;padding:12px;}</style></body></html>`;
const server=await createServer({root,configFile:false,resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'virtual:link-ipc'},
  {find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser']},
  plugins:[svelte(),{name:'link-ui',resolveId:id=>id==='virtual:link-ipc'?'\0link-ipc':null,
    load:id=>id==='\0link-ipc'?native:null,configureServer(server){server.middlewares.use('/__links',async(_req,res)=>{
      res.setHeader('Content-Type','text/html; charset=utf-8');res.end(await server.transformIndexHtml('/__links',html));
    });}}],optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},
  server:{host:'127.0.0.1',port:5190,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try{
  await server.listen(); browser=await chromium.launch({headless:true,channel:'msedge'});
  const page=await browser.newPage(); const errors=[];
  page.on('pageerror',e=>{errors.push(e.message);console.error('Browser:',e.message);}); page.setDefaultTimeout(15000);
  let count=0;
  for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const size of [{width:320,height:430},{width:480,height:720},{width:1000,height:760}]){
    await page.setViewportSize(size);
    await page.goto(server.resolvedUrls.local[0]+'__links?lang='+lang+'&theme='+theme);
    await page.waitForFunction(()=>window.ready);
    const add=page.locator('.attachment-trigger'); await add.click();
    await page.locator('.note-add-menu button').last().click();
    await page.locator('dialog[open]').waitFor();
    assert.equal(await page.locator('dialog input').first().inputValue(),'Source note');
    assert.equal(await page.locator('dialog textarea').inputValue(),'');
    for(const button of await page.locator('dialog footer button').all()){
      const b=await button.boundingBox(); assert.ok(b.x>=0&&b.y>=0&&b.x+b.width<=size.width&&b.y+b.height<=size.height,'visible footer');
    }
    assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'dialog must not overflow');
    await page.locator('dialog input').first().fill('New linked task');
    await page.locator('dialog textarea').fill('Task details');
    await page.evaluate(()=>window.failSave=true);
    await page.locator('dialog button[type=submit]').click(); await page.locator('dialog [role=alert]').waitFor();
    assert.equal(await page.locator('dialog input').first().inputValue(),'New linked task');
    await page.evaluate(()=>window.failSave=false);
    await page.locator('dialog button[type=submit]').click(); await page.waitForFunction(()=>window.saved===1);
    const creates=await page.evaluate(()=>window.calls.filter(c=>c.command==='create_linked_todo'));
    assert.equal(creates.length,2);assert.deepEqual(creates[0].args.draft,creates[1].args.draft);
    assert.equal(creates[1].args.draft.note,'Task details');assert.equal(creates[1].args.draft.due_date,null);
    assert.equal(await page.locator('.note-editor > textarea').inputValue(),'Keep this content');
    await add.click(); await page.locator('.note-add-menu button').last().click(); await page.locator('dialog[open]').waitFor();
    await page.keyboard.press('Escape');await page.waitForFunction(()=>window.cancelled===1);
    assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==='create_linked_todo').length),2);
    await page.locator('.note-task-links button[aria-expanded]').click();
    assert.equal(await page.locator('.note-task-links li').count(),5);
    assert.ok(await page.locator('.note-task-links').evaluate(el=>el.scrollWidth<=el.clientWidth),'links must wrap');
    if(size.width===320)await page.screenshot({path:resolve(output,lang+'-'+theme+'-list.png')});
    await add.click(); await page.locator('.note-add-menu button').last().click();
    if(size.width===320)await page.screenshot({path:resolve(output,lang+'-'+theme+'-dialog.png')});
    count++;
  }
  const dst=await browser.newPage({timezoneId:'America/New_York'});
  dst.on('pageerror',e=>errors.push(e.message));
  await dst.goto(server.resolvedUrls.local[0]+'__links?lang=en-US&theme=light');
  await dst.waitForFunction(()=>window.ready);
  await dst.locator('.attachment-trigger').click();
  await dst.locator('.note-add-menu button').last().click();
  await dst.getByLabel('Task title',{exact:true}).fill('DST validation');
  await dst.locator('input[type=datetime-local]').fill('2027-03-14T02:30');
  await dst.locator('dialog button[type=submit]').click();
  await dst.locator('dialog [role=alert]').waitFor();
  assert.equal(await dst.evaluate(()=>window.calls.filter(c=>c.command==='create_linked_todo').length),0);
  await dst.locator('input[type=datetime-local]').fill('2027-03-14T03:30');
  await dst.locator('dialog button[type=submit]').click();
  await dst.waitForFunction(()=>window.saved===1);
  const saved=await dst.evaluate(()=>window.calls.find(c=>c.command==='create_linked_todo').args.draft);
  assert.equal(saved.reminder_at,Date.parse('2027-03-14T07:30:00Z'));
  assert.deepEqual(errors,[]);console.log('Linked UI combinations passed: '+count+' plus DST validation; screenshots: '+output);
}finally{await browser?.close();await server.close();}
