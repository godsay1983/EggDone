// Production dialog, editor, save queue and parent navigation; isolated IPC, not native acceptance.
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
const output = resolve(tmpdir(), 'eggdone-content-search-ui-' + Date.now()); mkdirSync(output, { recursive: true });
const panel = readFileSync(resolve(root,'src/lib/components/TodoPanel.svelte'),'utf8');
const ast = ts.createSourceFile('panel.ts',panel.slice(panel.indexOf('<script lang="ts">')+18,panel.indexOf('</script>')),ts.ScriptTarget.Latest,true);
const names=['openContentSearch','openSearchResult','backFromLinkedContent','openRelatedContent','updateNote','flushAllNoteChanges','refreshNoteAttachments','loadAttachmentPreviews'];
const methods=ast.statements.filter(n=>ts.isFunctionDeclaration(n)&&names.includes(n.name?.text));assert.equal(methods.length,names.length);
const previewEffect=ast.statements.find(n=>ts.isLabeledStatement(n)&&n.getText(ast).includes('const requestKey = visibleNotePreviewAttachments'));
assert.ok(previewEffect,'Use the production list-preview effect to detect search-triggered file reads');
for(const method of methods) assert.doesNotMatch(method.getText(ast),/setListView|focusTodoByUuid|writePreference|setSelectedGroup|searchQuery\s*=/);
const harness=`<script lang="ts">
 import {onMount} from 'svelte';import {writable} from 'svelte/store';
 import {translator} from '$lib/i18n';import {createNoteStore} from '$lib/stores/noteStore';
 import {contentSearchApi} from '$lib/api/contentSearchApi';
 import {noteAttachmentApi} from '$lib/api/noteAttachmentApi';
 import Editor from '$lib/components/NoteEditor.svelte';import Dialog from '$lib/components/ContentSearchDialog.svelte';
 import Workspace from '$lib/components/LinkWorkspace.svelte';import TodoItem from '$lib/components/TodoItem.svelte';
 const noop=async()=>{};const notes=createNoteStore(undefined,()=>{},600);
 const todos=Object.assign(writable({items:window.taskData,error:null}),{refresh:noop});
 let contentSearchSession=false,contentSearchActive=false,contentSearchOpening=false,searchAttachmentUuid='';
 let selectedNoteUuid='source',linkedTodoUuid=null,linkedTaskEditing=false,linkNavigating=false,linkHistory=[],linkNotice='';
 let historyOpening=false,historyUuid=null,historyOpenError=false,noteNavigationBusy=false,noteAttachmentBusy=false;
 let linkedRequest=null,linkManager=null,noteDraft=null,captureRequest=null,captureLoading=false,summaryMenuOpen=false,noteAttachmentError='';
 let noteDraftSaveTimer=null,noteDraftCreatePromise=null,noteDraftCreated=null,noteAttachmentsByNote={};
 const NOTE_DRAFT_UUID='draft',hasNoteDraftContent=()=>false,persistNoteDraft=noop,scheduleNoteDraftSave=noop;
 const linkReader={resolve:async(_scope,_source,id)=>({link:{uuid:id,todo_uuid:'todo1',note_uuid:'note1'}})};
 const releaseUnusedAttachmentPreviews=()=>{};let noteAttachmentPreviewUrls={};
 let visibleNotePreviewAttachments=[],visibleNotePreviewRequestKey='';
 $: visibleNotePreviewAttachments=Object.values(noteAttachmentsByNote).flat().filter(a=>a.kind==='image');
 ${previewEffect.getText(ast)}
 const noteEditorScrollPositions=new Map();
 ${methods.map(n=>n.getText(ast)).join('\n')}
 $: selectedNote=$notes.items.find(n=>n.uuid===selectedNoteUuid);
 $: linkedTodo=$todos.items.find(n=>n.uuid===linkedTodoUuid);
 $: window.navigation={contentSearchSession,contentSearchActive,contentSearchOpening,selectedNoteUuid,linkedTodoUuid,depth:linkHistory.length};
 window.backSource=backFromLinkedContent;window.openRelated=()=>openRelatedContent({link:{uuid:'l',todo_uuid:'todo1',note_uuid:'note0'}},'todo');
 onMount(()=>{void notes.load();return()=>{void notes.flushPending();};});
 </script>
 <Workspace active={linkHistory.length>0} busy={linkNavigating||linkedTaskEditing} onBack={backFromLinkedContent}>
 {#if linkedTodo}
  <button class="action-button" onclick={backFromLinkedContent}>Back to search</button>
  <TodoItem todo={linkedTodo} animationEnabled={false} onToggle={noop} onEdit={noop} onNote={noop} onPin={noop} onPriority={noop} onFocus={noop}
   onSchedule={noop} onSnooze={noop} onGroupChange={noop} onDelete={noop} onMove={noop} onDragStart={()=>{}} onBatchSelect={()=>{}}
   dragDisabled={true} onManageLinks={noop} onEditingChange={v=>linkedTaskEditing=v}/>
 {:else if selectedNote}
 {#key selectedNote.uuid}
 <Editor note={selectedNote} locked={contentSearchOpening||contentSearchActive} onSearch={contentSearchSession?null:()=>void openContentSearch()}
  focusAttachmentUuid={searchAttachmentUuid} scrollPositions={noteEditorScrollPositions} saving={$notes.saving} error={noteAttachmentError||$notes.error} saveFailed={!!$notes.error}
  onClearAttachmentFocus={()=>searchAttachmentUuid=''}
  onRetrySave={()=>flushAllNoteChanges().catch(()=>{})} onChange={updateNote} onDone={backFromLinkedContent} onPin={noop} onColor={noop} onDelete={noop}
  attachments={noteAttachmentsByNote[selectedNote.uuid]||[]} onAddImages={noop} onAddFiles={noop} onOpenAttachment={async()=>{window.fileReads++;return '';}}
  onOpenFile={async()=>{window.fileReads++;}} onMoveAttachment={noop} onDeleteAttachment={noop} onRetryAttachment={noop}/>
 {/key}
 {/if}
 </Workspace>
 {#if contentSearchSession}<Dialog active={contentSearchActive} onOpen={openSearchResult}
 onClose={()=>{contentSearchSession=false;contentSearchActive=false;}}/>{/if}
 <style>:global(body){margin:0;padding:12px;box-sizing:border-box;display:flex;flex-direction:column;height:100vh;}</style>`;
const native=`export const isTauri=()=>false;
 export async function invoke(command,args){
 if(command==='list_task_note_links')return [];
 if(command==='list_notes')return window.noteData;
 if(command==='list_note_attachments')return window.assets.filter(a=>a.note_uuid===args.noteUuid);
 if(command==='read_note_attachment_preview'){window.fileReads++;return [1,2,3];}
 if(command==='update_note'){
   if(window.holdSave)await new Promise(r=>window.releaseSave=r);
   if(window.failSave)throw Error('private save failure');
   const n=window.noteData.find(n=>n.uuid===args.uuid);Object.assign(n,{title:args.title,content:args.content,updated_at:n.updated_at+1});window.saves++;return structuredClone(n);
 }
 if(command==='search_content'){
   window.calls.push({...args});if(window.failedScope===args.scope)throw Error('private database path');
   const all=window.results[args.scope].filter(i=>(i.title+' '+i.excerpt).toLowerCase().includes(args.query.toLowerCase()));
   return {...args,total:all.length,items:all.slice(args.offset,args.offset+args.limit)};
 }
 if(command==='resolve_search_target'){
   if(window.unavailable)throw Error('SEARCH_UNAVAILABLE');
   const row=window.results[args.scope].find(r=>r.uuid===args.uuid);
   return {...row,content:row.archived?'Archived full body\\n<img src=x onerror=alert(1)>':row.excerpt};
 }
 throw Error('Unexpected IPC '+command);
 }`;
const html=String.raw`<!doctype html><html><body><script type="module">
 import {mount} from 'svelte';import Harness from '/__search-harness.svelte';import {setLanguageMode} from '/src/lib/i18n/index.ts';import '/src/app.css';
 const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang'));document.documentElement.dataset.theme=p.get('theme');document.documentElement.style.zoom=p.get('scale')||'1';
 window.calls=[];window.fileReads=0;window.saves=0;window.failedScope='';window.unavailable=false;window.failSave=false;window.holdSave=false;
 const item=(kind,i)=>({kind,uuid:kind+i,title:'Needle '+kind+' '+i+' 标题'.repeat(8),excerpt:'Needle body <b>plain text</b>',parent_uuid:null,parent_title:null,completed:false,archived:false,updated_at:1,matched_field:'title'});
 window.results={todo:Array.from({length:25},(_,i)=>({...item('todo',i),archived:i===0})),note:Array.from({length:22},(_,i)=>item('note',i)),attachment:[{...item('attachment',0),parent_uuid:'note0',parent_title:'Needle note 0'}]};
 window.taskData=window.results.todo.filter(t=>!t.archived).map((t,i)=>({...t,id:i+1,note:'Task full body',pinned:false,priority:0,group_uuid:null,due_date:null,due_at:null,reminder_at:null,repeat_rule:null,repeat_series_uuid:null,archived_at:null,deleted_at:null}));
 window.noteData=[{uuid:'source',title:'Source note',content:'Source body'},...window.results.note.map(n=>({uuid:n.uuid,title:n.title,content:'Current note body\n'.repeat(30)}))].map(n=>({...n,color:'default',pinned:false,updated_at:1,created_at:1,updated_by:'test',deleted_at:null}));
 window.assets=Array.from({length:15},(_,i)=>({uuid:i===14?'attachment0':'asset'+i,note_uuid:'note0',kind:'file',display_name:'Needle attachment '+i+'.md',mime_type:'text/markdown',byte_size:10,sort_order:i,updated_at:1,deleted_at:null,transfer_state:'remote_only',remote_uploaded:true,local_original_path:null,local_preview_path:null}));
 window.assets.push({...window.assets[0],uuid:'remote-image',kind:'image',display_name:'Remote image.png',mime_type:'image/png',width:1,height:1});
 mount(Harness,{target:document.body});
 </script></body></html>`;
const id=resolve(root,'__search-harness.svelte').replaceAll('\\','/');
const server=await createServer({root,configFile:false,resolve:{alias:[{find:'$lib',replacement:resolve(root,'src/lib')},{find:'@tauri-apps/api/core',replacement:'virtual:search-ipc'}],conditions:['browser']},
 plugins:[svelte(),{name:'search-ui',resolveId:x=>x==='virtual:search-ipc'?'\0search-ipc':x==='/__search-harness.svelte'?id:null,
 load:x=>x==='\0search-ipc'?native:x===id?harness:null,configureServer(s){s.middlewares.use('/__search',async(_req,res)=>{res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/__search',html));});}}],
 optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try {
 await server.listen();browser=await chromium.launch({headless:true,channel:'msedge'});const page=await browser.newPage();const errors=[];
 page.on('pageerror',error=>errors.push(error.message));page.setDefaultTimeout(12000);const url=server.resolvedUrls.local[0]+'__search';
 async function begin(lang='en-US',theme='dark',scale=1){await page.goto(url+`?lang=${lang}&theme=${theme}&scale=${scale}`);
 await page.getByRole('button',{name:lang==='zh-CN'?'统一搜索':'Search everything',exact:true}).click();await page.getByRole('searchbox').fill('needle');await page.getByRole('searchbox').press('Enter');await page.locator('[data-scope="todo"] .result').first().waitFor();}
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const size of [{width:320,height:430},{width:480,height:720},{width:1100,height:800}])for(const scale of [1,1.5]){
  console.log('Checking search',lang,theme,size.width,scale);await page.setViewportSize(size);await begin(lang,theme,scale);
  assert.equal(await page.locator('.result').count(),41);
  assert.ok(await page.locator('dialog[open]').evaluate(d=>{const r=d.getBoundingClientRect();return r.left>=-1&&r.right<=innerWidth+1&&r.top>=-1&&r.bottom<=innerHeight+1&&d.scrollWidth<=d.clientWidth+1;}));
  await page.locator('[data-scope="todo"] .result').first().click();await page.getByText(lang==='zh-CN'?'已归档任务 · 只读':'Archived task · Read only',{exact:true}).waitFor();
  assert.equal(await page.locator('dialog[open] textarea').count(),0);assert.equal(await page.locator('dialog[open] img').count(),0);
  await page.getByRole('button',{name:lang==='zh-CN'?'返回结果':'Back to results',exact:true}).click();
  assert.equal(await page.getByRole('searchbox').inputValue(),'needle');
  await page.screenshot({path:resolve(output,`${lang}-${theme}-${size.width}-${scale}.png`)});
 }
 await page.setViewportSize({width:480,height:720});await begin();
 await page.locator('[data-scope="todo"] .result').nth(5).scrollIntoViewIfNeeded();
 const resultScroll=await page.locator('dialog[open] .content').evaluate(n=>n.scrollTop);assert.ok(resultScroll>0);
 await page.locator('[data-scope="todo"] .result').nth(5).click();await page.getByRole('button',{name:'Back to search',exact:true}).click();
 assert.ok(Math.abs(await page.locator('dialog[open] .content').evaluate(n=>n.scrollTop)-resultScroll)<2);
 await page.locator('[data-scope="todo"]').getByRole('button',{name:'Next',exact:true}).click();assert.equal(await page.locator('[data-scope="todo"] .result').count(),5);
 await page.locator('[data-scope="todo"] .result').first().click();await page.getByRole('button',{name:'Back to search',exact:true}).click();
 assert.equal(await page.locator('[data-scope="todo"] .result').count(),5);assert.equal(await page.getByRole('searchbox').inputValue(),'needle');
 await page.getByRole('combobox',{name:'Result type'}).selectOption('attachment');
 assert.equal(await page.locator('.result').count(),1);
 await page.locator('[data-scope="attachment"] .result').click();await page.locator('.note-attachment-manager .search-target').waitFor();
 assert.equal(await page.getByRole('button',{name:'Search everything',exact:true}).count(),0);
 assert.equal(await page.locator('.search-target').getAttribute('data-attachment-id'),'attachment0');assert.equal(await page.evaluate(()=>window.fileReads),0);
 const box=await page.locator('.search-target').boundingBox();assert.ok(box.y>=0&&box.y<720);await page.screenshot({path:resolve(output,'attachment-target.png')});
 await page.getByRole('button',{name:'Close attachment manager',exact:true}).click();
 await page.evaluate(()=>window.openRelated());await page.waitForFunction(()=>window.navigation.depth===2);
 await page.evaluate(()=>window.backSource());await page.waitForFunction(()=>window.navigation.depth===1);
 assert.equal(await page.locator('.note-attachment-manager').count(),0);
 await page.evaluate(()=>window.backSource());
 assert.equal(await page.getByRole('searchbox').inputValue(),'needle');
 assert.equal(await page.getByRole('combobox',{name:'Result type'}).inputValue(),'attachment');
 await page.getByRole('combobox',{name:'Result type'}).selectOption('all');
 await page.evaluate(()=>window.unavailable=true);await page.locator('[data-scope="note"] .result').first().click();await page.getByText('This content is no longer available. Search again.',{exact:true}).waitFor();
 assert.equal(await page.evaluate(()=>window.navigation.depth),0);
 await page.evaluate(()=>{window.unavailable=false;window.failedScope='note';});await page.getByRole('searchbox').press('Enter');await page.locator('[data-scope="note"]').getByRole('alert').waitFor();
 assert.equal(await page.locator('[data-scope="todo"] .result').count(),20);assert.equal(await page.locator('[data-scope="attachment"] .result').count(),1);
 await page.evaluate(()=>window.failedScope='');await page.locator('[data-scope="note"]').getByRole('button',{name:'Retry',exact:true}).click();await page.locator('[data-scope="note"] .result').first().waitFor();
 await page.getByRole('searchbox').fill('x'.repeat(101));await page.getByRole('searchbox').press('Enter');await page.getByText('Use up to 100 characters without null characters.',{exact:true}).waitFor();
 await page.getByRole('button',{name:'Clear',exact:true}).click();assert.equal(await page.locator('.result').count(),0);
 assert.equal(await page.evaluate(()=>window.fileReads),0);
 await page.getByRole('button',{name:'Close',exact:true}).click();
 // Closing the search session resumes ordinary list-card previews, exactly once.
 await page.waitForFunction(()=>window.fileReads===1);
 // Failed source save cannot open search or lose edits; a held save freezes the source inputs.
 await page.locator('.note-editor > textarea').fill('Edited source pending');await page.evaluate(()=>window.failSave=true);
 await page.getByRole('button',{name:'Search everything',exact:true}).click();await page.getByText('Save the current content before searching. Please retry.',{exact:true}).waitFor();
 assert.equal(await page.getByRole('searchbox').count(),0);assert.equal(await page.locator('.note-editor > textarea').inputValue(),'Edited source pending');
 await page.evaluate(()=>{window.failSave=false;window.holdSave=true;});await page.getByRole('button',{name:'Search everything',exact:true}).click();
 await page.waitForFunction(()=>typeof window.releaseSave==='function');assert.equal(await page.locator('.note-editor').getAttribute('inert'),'');
 await page.evaluate(()=>{window.holdSave=false;window.releaseSave();});await page.getByRole('searchbox').waitFor();assert.ok(await page.evaluate(()=>window.saves)>0);
 await page.getByRole('searchbox').fill('needle');await page.getByRole('searchbox').press('Enter');await page.locator('[data-scope="note"] .result').first().click();
 await page.locator('.note-editor > textarea').fill('Edited target pending');await page.evaluate(()=>window.failSave=true);
 assert.equal(await page.evaluate(()=>window.backSource()),false);assert.equal(await page.locator('.note-editor > textarea').inputValue(),'Edited target pending');
 await page.evaluate(()=>window.failSave=false);assert.equal(await page.evaluate(()=>window.backSource()),true);await page.getByRole('searchbox').waitFor();
 assert.equal(await page.getByRole('searchbox').inputValue(),'needle');assert.equal(await page.evaluate(()=>window.fileReads),1);
 assert.deepEqual(errors,[]);console.log('PASS 24 layout/locale/theme/scale cases and production save, task/note/attachment navigation, paging and failure flows. Evidence:',output);
} finally { if(browser)await browser.close();await server.close(); }
