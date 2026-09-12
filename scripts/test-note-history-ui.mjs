// Real Svelte dialog with isolated IPC. Native persistence and sync are tested separately.
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
const ts = require('typescript');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(tmpdir(), 'eggdone-note-history-ui-' + Date.now());
mkdirSync(output, { recursive: true });
const native = `export const isTauri=()=>false;
export async function invoke(command,args){
 if(command==='list_task_note_links')return [];
 if(command==='list_notes'){if(window.failRefresh)throw Error('refresh');return [{...window.current,uuid:'n',color:'default',pinned:false,created_at:1}];}
 if(command==='update_note'){
   if(window.holdSave)await new Promise(resolve=>window.releaseSave=resolve);
   if(window.failSave)throw Error('save failed');
   window.saves.push({title:args.title,content:args.content});
   window.current={...window.current,title:args.title,content:args.content,updated_at:window.current.updated_at+1};
   return {...window.current,uuid:'n',color:'default',pinned:false,created_at:1};
 }
 if(command==='list_note_history'){if(window.failLoad)throw Error('private database path');return window.rows;}
 if(command==='preview_note_history'){
   if(window.failPreview)throw Error('NOTE_HISTORY_NOT_FOUND');
   return {entry:{id:args.id,note_uuid:args.uuid,text:window.old,captured_at:1789190000000},current:window.current};
 }
 if(command==='restore_note_history'){
   window.writes.push(structuredClone(args.expected));
   if(window.hold)await new Promise(resolve=>window.release=resolve);
   if(window.conflict){window.conflict=false;throw Error('NOTE_HISTORY_CONFLICT');}
   if(window.noop)return false;
   window.current=structuredClone(args.expected.entry.text);return true;
 }
 throw Error('Unexpected IPC '+command);
}`;
const html = String.raw`<!doctype html><html><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/NoteHistoryDialog.svelte';
import Harness from '/__history-harness.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang'));
document.documentElement.dataset.theme=p.get('theme');document.documentElement.style.zoom=p.get('scale')||'1';
window.writes=[];window.saves=[];window.closedHistory=false;window.failRefresh=false;window.failLoad=p.has('failLoad');window.hold=false;
window.old={title:'Historical title '.repeat(4),content:'<img src=x onerror=alert(1)>\nHistorical full body\n'.repeat(80),updated_at:1,updated_by:'test'};
window.current={title:'Current title',content:'Current body\n'.repeat(80),updated_at:2,updated_by:'test'};
window.rows=p.has('empty')?[]:Array.from({length:100},(_,i)=>({id:i+1,note_uuid:'n',title:window.old.title,excerpt:'Saved text',captured_at:1789190000000,updated_at:1}));
const instance=p.has('editor')?mount(Harness,{target:document.body}):mount(Dialog,{target:document.body,props:{uuid:'n',beforeRestore:()=>{},
 afterRestore:async changed=>{window.changed=changed;if(window.failRefresh)throw Error('refresh');},
 onRefresh:async()=>{if(window.failRefresh)throw Error('refresh');},
 onClose:refreshFailed=>{window.closedHistory=true;window.closedStale=refreshFailed;void unmount(instance);}}});
</script></body></html>`;
// Compile the real parent methods into a fixture shell, keeping the production editor and save queue.
const panel = readFileSync(resolve(root, 'src/lib/components/TodoPanel.svelte'), 'utf8');
const ast = ts.createSourceFile('panel.ts', panel.slice(panel.indexOf('<script lang="ts">') + 18, panel.indexOf('</script>')), ts.ScriptTarget.Latest, true);
const names = ['openNoteHistory','assertHistoryReady','refreshHistoryEditor','afterHistoryRestore','closeNoteHistory','updateNote','flushAllNoteChanges'];
const methods = ast.statements.filter(n => ts.isFunctionDeclaration(n) && names.includes(n.name?.text));
assert.equal(methods.length,names.length);
const harness = `<script lang="ts">
 import {onMount} from 'svelte';
 import {translator} from '$lib/i18n';
 import {createNoteStore} from '$lib/stores/noteStore';
 import Editor from '$lib/components/NoteEditor.svelte';
 import NoteHistoryDialog from '$lib/components/NoteHistoryDialog.svelte';
 const notes=createNoteStore(undefined,()=>{},600);const scheduleAutoSync=()=>{};
 let historyOpening=false,historyUuid=null,historyEditorRevision=0,historyRefreshed=false,historyOpenError=false;
 let selectedNoteUuid='n',noteDraft=null,noteNavigationBusy=false,noteAttachmentBusy=false,linkNavigating=false,linkedRequest=null,linkManager=null;
 let linkHistory=[],linkNotice='',noteAttachmentError=null,noteDraftSaveTimer=null,noteDraftCreatePromise=null,noteDraftCreated=null;
 const NOTE_DRAFT_UUID='draft',noop=async()=>{};
 const hasNoteDraftContent=()=>false,persistNoteDraft=noop,scheduleNoteDraftSave=()=>{};
 $: selectedNote=$notes.items.find(n=>n.uuid===selectedNoteUuid);
 ${methods.map(n=>n.getText(ast)).join('\n')}
 onMount(()=>{void notes.load();return ()=>{void notes.flushPending();};});
 </script>
 {#if selectedNote}
 {#key selectedNote.uuid + ':' + historyEditorRevision}
 <Editor note={selectedNote} locked={historyOpening || historyUuid !== null} onHistory={()=>void openNoteHistory()}
 saving={$notes.saving} error={historyOpenError ? $translator('history.saveFailed') : noteAttachmentError || $notes.error} onChange={updateNote} onDone={noop}
 onPin={noop} onColor={noop} onDelete={noop} onAddImages={noop} onAddFiles={noop} onOpenAttachment={noop}
 onOpenFile={noop} onMoveAttachment={noop} onDeleteAttachment={noop} onRetryAttachment={noop}/>
 {/key}
 {:else}<p data-testid="note-list">Note list</p>{/if}
 {#if historyUuid}<NoteHistoryDialog uuid={historyUuid} beforeRestore={assertHistoryReady}
 afterRestore={afterHistoryRestore} onRefresh={refreshHistoryEditor} onClose={closeNoteHistory}/>{/if}
 <style>:global(body){padding:12px;display:flex;flex-direction:column;}</style>`;
const harnessId=resolve(root,'__history-harness.svelte').replaceAll('\\','/');
const server = await createServer({ root, configFile:false,
 resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'virtual:history-ipc'},{find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser']},
 plugins:[svelte(),{name:'history-ui',resolveId:id=>id==='virtual:history-ipc'?'\0history-ipc':id==='/__history-harness.svelte'?harnessId:null,
  load:id=>id==='\0history-ipc'?native:id===harnessId?harness:null,
  configureServer(s){s.middlewares.use('/__history',async(_req,res)=>{res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/__history',html));});}}],
 optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},
 server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}}
});
let browser,page;
try {
 await server.listen();browser=await chromium.launch({headless:true,channel:'msedge'});page=await browser.newPage();
 const errors=[];page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(15000);
 const url=server.resolvedUrls.local[0]+'__history';let count=0;
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])
  for(const size of [{width:320,height:430},{width:480,height:720},{width:1100,height:800}])for(const scale of [1,1.5]){
   console.log('Checking',lang,theme,size.width,scale);
   await page.setViewportSize(size);await page.goto(url+'?lang='+lang+'&theme='+theme+'&scale='+scale);
   await page.locator('.record').first().waitFor();assert.equal(await page.locator('.record').count(),100);
   const begin=()=>page.getByRole('button',{name:lang==='zh-CN'?'恢复此版本':'Restore this version',exact:true});
   const confirm=()=>page.getByRole('button',{name:lang==='zh-CN'?'确认恢复':'Confirm restore',exact:true});
   await page.locator('.record').first().click();await begin().waitFor();
   assert.equal(await page.locator('.comparison section').count(),2);
   assert.equal(await page.locator('.comparison img').count(),0);
   assert.equal(await page.locator('.comparison .body').last().textContent(),await page.evaluate(()=>window.old.content));
   await begin().click();await confirm().waitFor();await page.keyboard.press('Escape');await begin().waitFor();
   assert.equal(await page.evaluate(()=>window.writes.length),0);
   await page.keyboard.press('Escape');await page.locator('.record').first().waitFor();
   await page.locator('.record').first().click();await begin().click();
   await page.evaluate(()=>window.conflict=true);await confirm().click();await page.locator('.record').first().waitFor();
   assert.equal(await page.evaluate(()=>window.current.title),'Current title');
   await page.locator('.record').first().click();await begin().click();
   assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'horizontal overflow');
   for(const button of await page.locator('footer button').all()){
    const b=await button.boundingBox();assert.ok(b.x>=0&&b.y>=0&&b.x+b.width<=size.width+1&&b.y+b.height<=size.height+1,'footer clipped');
   }
   const panes=await page.locator('.comparison section').evaluateAll(els=>els.map(e=>({x:e.getBoundingClientRect().x,y:e.getBoundingClientRect().y})));
   if(size.width===1100&&scale===1)assert.ok(panes[1].x>panes[0].x,'wide preview must be side by side');
   if(size.width===320)assert.ok(panes[1].y>panes[0].y,'narrow preview must stack');
   if(size.width!==480)await page.screenshot({path:resolve(output,`${lang}-${theme}-${size.width}-${scale}.png`)});
   await page.evaluate(()=>window.hold=true);await confirm().click();await page.waitForFunction(()=>!!window.release);
   assert.equal(await confirm().isDisabled(),true);await page.keyboard.press('Escape');
   assert.equal(await page.evaluate(()=>window.closedHistory),false);
   await page.evaluate(()=>{window.failRefresh=true;window.hold=false;window.release();});
   await page.getByRole('status').filter({hasText:lang==='zh-CN'?'刷新失败':'could not refresh'}).waitFor();
   assert.equal(await page.locator('.record').first().isDisabled(),true);
   await page.evaluate(()=>window.failRefresh=false);
   await page.getByRole('button',{name:lang==='zh-CN'?'刷新列表':'Refresh list',exact:true}).click();
   await page.waitForFunction(()=>!document.querySelector('.record').disabled);
   assert.equal(await page.evaluate(()=>window.writes.length),2);
   await page.keyboard.press('Escape');await page.waitForFunction(()=>window.closedHistory);
   assert.equal(await page.evaluate(()=>window.closedStale),false);count++;
  }
 await page.goto(url+'?lang=en-US&theme=light&failLoad=1');await page.getByRole('alert').waitFor();
 assert.equal((await page.locator('dialog').innerText()).includes('private database path'),false);
 await page.evaluate(()=>window.failLoad=false);await page.getByRole('button',{name:'Refresh list',exact:true}).click();
 await page.locator('.record').first().waitFor();await page.evaluate(()=>window.failPreview=true);
 await page.locator('.record').first().click();await page.getByRole('status').filter({hasText:'version was removed'}).waitFor();
 assert.equal(await page.locator('.comparison').count(),0);
 await page.evaluate(()=>{window.failPreview=false;window.noop=true;});
 await page.locator('.record').first().click();await page.getByRole('button',{name:'Restore this version',exact:true}).click();
 await page.getByRole('button',{name:'Confirm restore',exact:true}).click();
 await page.getByRole('status').filter({hasText:'Nothing changed'}).waitFor();assert.equal(await page.evaluate(()=>window.changed),false);
 await page.evaluate(()=>{window.noop=false;window.failRefresh=true;});
 await page.locator('.record').first().click();await page.getByRole('button',{name:'Restore this version',exact:true}).click();
 await page.getByRole('button',{name:'Confirm restore',exact:true}).click();
 await page.getByRole('status').filter({hasText:'could not refresh'}).waitFor();
 await page.keyboard.press('Escape');await page.waitForFunction(()=>window.closedHistory);
 assert.equal(await page.evaluate(()=>window.closedStale),true);
 await page.goto(url+'?lang=en-US&theme=light&empty=1');await page.getByText('No history yet.',{exact:false}).waitFor();
 assert.equal(await page.locator('.record').count(),0);
 // Real editor + queue + extracted production parent lifecycle, not a second hand-written workflow.
 await page.setViewportSize({width:480,height:720});await page.goto(url+'?lang=en-US&theme=light&editor=1');
 const editor=page.locator('.note-editor'), title=editor.locator('input[maxlength="100"]');
 await title.waitFor();await page.evaluate(()=>window.holdSave=true);await title.fill('Unsaved before history');
 await page.getByRole('button',{name:'Version history',exact:true}).click();
 await page.waitForFunction(()=>!!window.releaseSave);
 assert.equal(await editor.evaluate(el=>el.inert),true);assert.equal(await page.locator('dialog').count(),0);
 await title.evaluate(el=>{el.value='late stale event';el.dispatchEvent(new Event('input',{bubbles:true}));});
 await page.evaluate(()=>{window.holdSave=false;window.releaseSave();});await page.locator('.record').first().waitFor();
 assert.equal(await page.evaluate(()=>window.current.title),'Unsaved before history');
 await page.locator('.record').first().click();await page.getByRole('button',{name:'Restore this version',exact:true}).click();
 await page.getByRole('button',{name:'Confirm restore',exact:true}).click();
 await page.getByRole('status').filter({hasText:'Restored locally'}).waitFor();
 await page.keyboard.press('Escape');await page.waitForFunction(()=>!document.querySelector('dialog'));
 assert.equal(await title.inputValue(),await page.evaluate(()=>window.old.title));
 assert.equal(await editor.locator('textarea').inputValue(),await page.evaluate(()=>window.old.content));
 await page.waitForTimeout(800);assert.equal(await page.evaluate(()=>window.saves.length),1);
 await title.fill('After restore edit');await page.waitForFunction(()=>window.saves.length===2);
 assert.equal(await page.evaluate(()=>window.current.title),'After restore edit');
 await page.getByRole('button',{name:'Version history',exact:true}).click();await page.locator('.record').first().click();
 await page.getByRole('button',{name:'Restore this version',exact:true}).click();await page.evaluate(()=>window.failRefresh=true);
 await page.getByRole('button',{name:'Confirm restore',exact:true}).click();await page.getByRole('status').filter({hasText:'could not refresh'}).waitFor();
 await page.keyboard.press('Escape');await page.getByTestId('note-list').waitFor();assert.equal(await editor.count(),0);
 await page.waitForTimeout(800);assert.equal(await page.evaluate(()=>window.saves.length),2);
 await page.goto(url+'?lang=en-US&theme=dark&editor=1');await title.waitFor();
 await page.evaluate(()=>window.failSave=true);await title.fill('Keep failed draft');
 await page.getByRole('button',{name:'Version history',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('.note-editor')&&!document.querySelector('.note-editor').inert);
 assert.equal(await page.locator('dialog').count(),0);assert.equal(await title.inputValue(),'Keep failed draft');
 await page.evaluate(()=>window.failSave=false);await page.getByRole('button',{name:'Version history',exact:true}).click();
 await page.locator('.record').first().waitFor();assert.equal(await page.evaluate(()=>window.current.title),'Keep failed draft');
 assert.deepEqual(errors,[]);
 console.log(`History UI: ${count} layout/locale/theme/zoom combinations; full text, confirm/cancel, conflict, busy, refresh failure/retry, empty/load/preview failure, no-op; production editor and save-queue restore/retry/stale-input regression passed. Screenshots: ${output}`);
}catch(error){if(page)await page.screenshot({path:resolve(output,'failure.png')});console.error('Evidence:',output);throw error;}
finally{await browser?.close();await server.close();}
