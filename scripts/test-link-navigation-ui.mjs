// Real components + production navigation methods, isolated IPC and persistence fixtures.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import ts from 'typescript';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(tmpdir(), 'eggdone-link-navigation-' + Date.now()); mkdirSync(output, { recursive: true });
const panel = readFileSync(resolve(root, 'src/lib/components/TodoPanel.svelte'), 'utf8');
const ast = ts.createSourceFile('panel.ts', panel.slice(panel.indexOf('<script lang="ts">') + 18, panel.indexOf('</script>')), ts.ScriptTarget.Latest, true);
const names = ['openRelatedContent', 'backFromLinkedContent'];
const methods = ast.statements.filter(n => ts.isFunctionDeclaration(n) && names.includes(n.name?.text));
assert.equal(methods.length, names.length);
const fixture = `<script lang="ts">
 import {writable} from 'svelte/store';
 import Editor from '$lib/components/NoteEditor.svelte';
 import Workspace from '$lib/components/LinkWorkspace.svelte';
 import Manager from '$lib/components/LinkManagerDialog.svelte';
 import TodoItem from '$lib/components/TodoItem.svelte';
 import {createLinkManager} from '$lib/stores/linkManagerStore';
 import {translator} from '$lib/i18n';
 const noop=async()=>{};
 const makeNote=(uuid,title)=>({uuid,title,content:'Line of note content\\n'.repeat(150),color:'default',pinned:false,deleted_at:null});
 const source=makeNote('source','Source note'), target=makeNote('target-note','Target note');
 const task={id:1,uuid:'target-task',title:'Linked task',note:'Task details',completed:false,pinned:false,priority:0,group_uuid:null,due_date:null,due_at:null,reminder_at:null,repeat_rule:null,repeat_series_uuid:null,archived_at:null,deleted_at:null};
 const image={uuid:'image',note_uuid:'target-note',kind:'image',display_name:'Fixture.png',byte_size:68,transfer_state:'synced',deleted_at:null};
 const imageUrl='data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jY1kAAAAASUVORK5CYII=';
 const notes=Object.assign(writable({items:[source,target],error:null}),{load:noop});
 const todos=Object.assign(writable({items:[task],error:null}),{refresh:noop});
 let selectedNoteUuid='source', linkedTodoUuid=null,linkHistory=[],linkManager=null,linkedRequest=null,linkNotice='';
 let linkNavigating=false,noteNavigationBusy=false,noteAttachmentBusy=false,linkedTaskEditing=false;
 let revision=0;window.refreshLinks=()=>revision++;
 const linkReader=createLinkManager();const scrollPositions=new Map();
 const refreshNoteAttachments=noop;
 async function flushAllNoteChanges(){if(window.failSave)throw Error('save failed');window.saves++;}
 ${methods.map(n => n.getText(ast)).join('\n')}
 $: selectedNote=$notes.items.find(n=>n.uuid===selectedNoteUuid);
 $: linkedTodo=$todos.items.find(n=>n.uuid===linkedTodoUuid);
 $: window.depth=linkHistory.length;
 function change(note,title,content){notes.update(s=>({...s,items:s.items.map(n=>n.uuid===note.uuid?{...n,title,content}:n)}));}
 window.failSave=false;window.saves=0;
</script>
<Workspace active={linkHistory.length>0} busy={linkNavigating||linkedTaskEditing} onBack={backFromLinkedContent}>
 {#if linkedTodo}
  <button class="action-button" disabled={linkedTaskEditing} onclick={backFromLinkedContent}>{$translator('links.backSource')}</button>
  <TodoItem todo={linkedTodo} animationEnabled={false} onToggle={noop} onEdit={noop} onNote={noop} onPin={noop} onPriority={noop} onFocus={noop} onSchedule={noop} onSnooze={noop} onGroupChange={noop} onDelete={noop} onMove={noop} onDragStart={()=>{}} onBatchSelect={()=>{}} dragDisabled={true}
    onManageLinks={()=>linkManager={scope:'todo',uuid:'target-task',title:'Linked task'}} onEditingChange={v=>linkedTaskEditing=v}/>
 {:else if selectedNote}
  {#key selectedNote.uuid}
  <Editor note={selectedNote} linkRevision={revision} {scrollPositions} onChange={change} onDone={backFromLinkedContent}
   attachments={selectedNote.uuid==='target-note'?[image]:[]} attachmentPreviewUrls={{image:imageUrl}}
   onOpenLink={item=>openRelatedContent(item,'note')} onPin={noop} onColor={noop} onDelete={noop}
   onAddImages={noop} onAddFiles={noop} onOpenAttachment={async()=>imageUrl} onOpenFile={noop} onMoveAttachment={noop} onDeleteAttachment={noop} onRetryAttachment={noop}/>
  {/key}
 {/if}
</Workspace>
{#if linkManager}
 <Manager scope="todo" uuid="target-task" title="Linked task" saveSource={flushAllNoteChanges} afterCommit={noop} onClose={()=>linkManager=null} onOpen={item=>openRelatedContent(item,'todo')}/>
{/if}`;
const native = `export const isTauri=()=>false;
export async function invoke(command,args){
 if(command==='list_task_note_links'){
  if(window.linkRemoved)return [];
  const ids=args.scope==='todo'?['source','target-note']:[args.uuid];
  return ids.map(id=>({link:{uuid:'link-'+id,todo_uuid:'target-task',note_uuid:id,deleted_at:null},todo_title:'Linked task',note_title:id==='source'?'Source note':'Target note',todo_state:window.linkState||'active',note_state:'active',is_repeating:false}));
 }
 if(command==='list_notes')return [];
 throw Error('Unexpected IPC '+command);
}`;
const fixtureId = resolve(root, 'src/LinkNavigationFixture.svelte').replaceAll('\\', '/');
const html = `<!doctype html><html><body><script type="module">
import {mount} from 'svelte';import Fixture from '/src/LinkNavigationFixture.svelte';import {setLanguageMode} from '/src/lib/i18n/index.ts';import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang'));document.documentElement.dataset.theme=p.get('theme');document.documentElement.style.zoom=p.get('scale')||'1';mount(Fixture,{target:document.body});
</script><style>body{height:100dvh;display:flex;flex-direction:column;padding:8px;box-sizing:border-box}</style></body></html>`;
const server = await createServer({ root, configFile:false, resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'virtual:link-nav-ipc'},{find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser']},
 plugins:[{name:'link-navigation-fixture',enforce:'pre',resolveId(id){if(id==='virtual:link-nav-ipc')return '\0link-nav-ipc';if(id==='/src/LinkNavigationFixture.svelte')return fixtureId;},load(id){if(id===fixtureId)return fixture;if(id==='\0link-nav-ipc')return native;},
 configureServer(s){s.middlewares.use('/__navigation',async(_req,res)=>{res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/__navigation',html));});}},svelte()],
 optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},server:{host:'127.0.0.1',port:5192,watch:{ignored:['**/src-tauri/**']}} });
let browser;
try {
 await server.listen();browser=await chromium.launch({headless:true,channel:'msedge'});const page=await browser.newPage();page.setDefaultTimeout(15000);const errors=[];page.on('pageerror',e=>{errors.push(e.message);console.error(e.message);});let count=0;
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const size of [{width:320,height:430},{width:480,height:720},{width:1000,height:760}])for(const scale of [1,1.5]){
  await page.setViewportSize(size);await page.goto(server.resolvedUrls.local[0]+'__navigation?lang='+lang+'&theme='+theme+'&scale='+scale);
  await page.locator('.note-editor textarea').waitFor();await page.locator('.note-editor textarea').fill('Saved source\n'+'long text\n'.repeat(150));
  await page.locator('.note-editor textarea').evaluate(el=>{el.scrollTop=300;el.dispatchEvent(new Event('scroll'));});
  await page.locator('.note-task-links > button').click();
  const open=()=>page.getByRole('button',{name:lang==='zh-CN'?'打开':'Open',exact:true});
  await page.evaluate(()=>window.failSave=true);await open().click();await page.getByRole('alert').waitFor();assert.equal(await page.evaluate(()=>window.depth),0);
  await page.evaluate(()=>window.failSave=false);await open().click();await page.waitForFunction(()=>window.depth===1);
  await page.locator('.todo-item').dblclick();const back=()=>page.getByRole('button',{name:lang==='zh-CN'?'返回来源':'Back to source',exact:true});
  assert.equal(await back().isDisabled(),true);await page.locator('.todo-item input').press('Escape');await page.waitForFunction(()=>!document.querySelector('.todo-item.editing'));
  await page.locator('.more-button').click();await page.getByRole('menuitem',{name:lang==='zh-CN'?'关联便签':'Linked notes',exact:true}).click();
  await open().last().click();await page.waitForFunction(()=>window.depth===2);
  assert.equal(await page.locator('.note-editor > input:not([type=file])').inputValue(),'Target note');
  assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'workspace overflow');
  await page.getByRole('button',{name:lang==='zh-CN'?'添加':'Add',exact:true}).click();await page.keyboard.press('Escape');assert.equal(await page.evaluate(()=>window.depth),2);
  await page.locator('.note-attachment-summary-heading').click();await page.locator('.note-attachment-preview').click();
  await page.locator('.note-image-viewer img').waitFor();await page.keyboard.press('Escape');
  assert.equal(await page.locator('.note-image-viewer').count(),0);assert.equal(await page.locator('.note-attachment-manager').count(),1);assert.equal(await page.evaluate(()=>window.depth),2);
  await page.keyboard.press('Escape');assert.equal(await page.locator('.note-attachment-manager').count(),0);assert.equal(await page.evaluate(()=>window.depth),2);
  const lastAction=page.locator('.note-editor footer button').last();await lastAction.scrollIntoViewIfNeeded();
  const bounds=await lastAction.boundingBox();assert.ok(bounds&&bounds.y>=0&&bounds.y+bounds.height<=size.height,'editor footer must remain reachable');
  const swatches=await page.locator('.note-color-picker button').evaluateAll(nodes=>nodes.map(el=>({color:getComputedStyle(el).backgroundColor,width:el.getBoundingClientRect().width})));
  assert.equal(swatches.length,5);assert.equal(new Set(swatches.map(s=>s.color)).size,5);assert.ok(swatches.every(s=>s.color!=='rgba(0, 0, 0, 0)'&&s.width>=19),'color controls must be visible');
  await page.screenshot({path:resolve(output,lang+'-'+theme+'-'+size.width+'-'+scale+'.png')});
  await page.getByRole('button',{name:lang==='zh-CN'?'返回':'Back',exact:true}).click();await page.waitForFunction(()=>window.depth===1);
  await back().click();await page.waitForFunction(()=>window.depth===0);
  assert.ok((await page.locator('.note-editor textarea').inputValue()).startsWith('Saved source'));
  assert.ok(await page.locator('.note-editor textarea').evaluate(el=>el.scrollTop>200),'source editor scroll lost');
  await page.locator('.note-task-links > button').click();
  await page.evaluate(()=>window.linkState='archived');await open().click();await page.getByRole('alert').waitFor();assert.equal(await page.evaluate(()=>window.depth),0);
  for(const state of ['deleted','missing']){
   await page.evaluate(state=>{window.linkState=state;window.refreshLinks();},state);await page.waitForFunction(()=>document.querySelector('.note-task-links li button')?.disabled);
   assert.equal(await open().isDisabled(),true);
  }
  await page.evaluate(()=>{window.linkState='completed';window.refreshLinks();});await page.waitForFunction(()=>!document.querySelector('.note-task-links li button')?.disabled);
  await open().click();await page.waitForFunction(()=>window.depth===1);await back().click();await page.waitForFunction(()=>window.depth===0);
  await page.locator('.note-task-links > button').click();await page.evaluate(()=>window.linkRemoved=true);
  await open().click();await page.getByRole('alert').waitFor();assert.equal(await page.evaluate(()=>window.depth),0);
  assert.equal(await page.locator('.note-task-links li').count(),0);
  count++;
 }
 assert.deepEqual(errors,[]);console.log('Linked navigation '+count+' locale/theme/size workflows passed; screenshots: '+output);
} finally {await browser?.close();await server.close();}
