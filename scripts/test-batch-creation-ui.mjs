// Production dialog and session; isolated creation/refresh adapters never access user data.
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
const output=resolve(tmpdir(),'eggdone-batch-ui-'+Date.now());mkdirSync(output,{recursive:true});
const refresh=`export async function refreshAfterBatch(){window.refreshes++;if(window.failRefresh)throw Error('refresh failed');}`;
const html=`<!doctype html><html><head><meta charset="utf-8"></head><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/TaskBatchDialog.svelte';
import {BatchCreationSession} from '/src/lib/utils/batchCreationSession.ts';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang')||'en-US');document.documentElement.dataset.theme=p.get('theme')||'light';
document.documentElement.style.zoom=p.get('scale')||'1';window.calls=[];window.refreshes=0;window.failRefresh=false;window.failSave='';window.loseReply=false;window.receipts=new Map();
window.pastes=0;Object.defineProperty(navigator,'clipboard',{value:{readText:async()=>{window.pastes++;if(window.denyPaste)throw Error('denied');return 'Pasted task';}},configurable:true});
const session=new BatchCreationSession({create:async r=>{
  window.calls.push(structuredClone(r));if(window.failSave)throw Error(window.failSave);
  if(window.receipts.has(r.operation_uuid))return structuredClone(window.receipts.get(r.operation_uuid));
  const result={operation_uuid:r.operation_uuid,task_uuids:r.items.map(i=>i.uuid),created_at:100};window.receipts.set(r.operation_uuid,result);
  if(window.loseReply){window.loseReply=false;throw Error('lost reply');}return result;
}},()=>crypto.randomUUID());
let panel;window.openPanel=()=>{panel=mount(Dialog,{target:document.body,props:{session,groups:[],onClose:()=>void unmount(panel)}});};window.openPanel();
</script></body></html>`;
const server=await createServer({root,configFile:false,resolve:{alias:[{find:'$lib/stores/taskBatchStore',replacement:'virtual:batch-refresh'},{find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser']},
 plugins:[svelte(),{name:'batch-ui',resolveId:id=>id==='virtual:batch-refresh'?'\0batch-refresh':null,load:id=>id==='\0batch-refresh'?refresh:null,
 configureServer(server){server.middlewares.use('/__batch',async(_req,res)=>{res.setHeader('Content-Type','text/html; charset=utf-8');res.end(await server.transformIndexHtml('/__batch',html));});}}],
 optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try{
 await server.listen();browser=await chromium.launch({headless:true,channel:'msedge'});
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(10000);
 const go=async(q='')=>page.goto(server.resolvedUrls.local[0]+'__batch?'+q);
 const button=name=>page.getByRole('button',{name,exact:true});
 let count=0;
 for(const lang of ['en-US','zh-CN'])for(const theme of ['light','dark'])for(const size of [{width:320,height:430},{width:480,height:720},{width:1100,height:800}])for(const scale of [1,1.5]){
  await page.setViewportSize(size);await go('lang='+lang+'&theme='+theme+'&scale='+scale);
  await page.locator('#batch-text').fill('- milk\n\n- milk\n3. report');assert.equal(await page.evaluate(()=>window.pastes),0);
  const assertInsetFocus=async field=>{
   await field.focus();
   const style=await field.evaluate(e=>{const s=getComputedStyle(e);return {focused:e.matches(':focus-visible'),width:s.outlineWidth,offset:s.outlineOffset,color:s.outlineColor,border:s.borderLeftColor};});
   assert.equal(style.focused,true);
   const width=parseFloat(style.width),offset=parseFloat(style.offset);
   assert.ok(width>0&&width<=1.01);assert.ok(offset<=-width+0.01,'focus ring stays inside the border at fractional zoom');
   assert.equal(style.color,style.border);
  };
  await assertInsetFocus(page.locator('#batch-text'));
  if(size.width===480&&scale===1)await page.screenshot({path:resolve(output,lang+'-'+theme+'-input-focus.png')});
  try { await button(lang==='en-US'?'Preview':'预览').click(); }
  catch(e){await page.screenshot({path:resolve(output,'failed.png')});console.error({lang,theme,size,scale,output});throw e;}
  await page.locator('.rows li').first().waitFor();
  assert.equal(await page.locator('.rows li').count(),3);assert.equal(await page.evaluate(()=>window.calls.length),0);
  await assertInsetFocus(page.locator('.rows textarea').first());
  await assertInsetFocus(page.locator('#batch-group'));
  assert.ok(await page.locator('dialog').evaluate(e=>e.scrollWidth<=e.clientWidth),'no horizontal overflow');
  for(const b of await page.locator('dialog footer button').all()){
   const r=await b.boundingBox();assert.ok(r.x>=-1&&r.y>=0&&r.x+r.width<=size.width+1&&r.y+r.height<=size.height+1,JSON.stringify({lang,theme,size,scale,r}));
  }
  if(size.width===480&&scale===1)await page.screenshot({path:resolve(output,lang+'-'+theme+'.png')});
  await page.locator('.rows textarea').nth(1).fill('Edited task');await page.locator('.rows input[type=checkbox]').nth(2).uncheck();
  await page.evaluate(()=>window.loseReply=true);
  await button(lang==='en-US'?'Create 2 tasks':'创建 2 项').click();await page.locator('[role=alert]').waitFor();
  assert.ok(await page.locator('.rows textarea').first().isDisabled());
  await button(lang==='en-US'?'Retry same request':'原样重试').click();await page.locator('[role=status]').waitFor();
  const calls=await page.evaluate(()=>window.calls);assert.equal(calls.length,2);assert.deepEqual(calls[0],calls[1]);
  assert.deepEqual(calls[0].items.map(i=>i.title),['milk','Edited task']);count++;
 }
 await page.setViewportSize({width:480,height:720});
 await go();await button('Paste').click();assert.equal(await page.locator('#batch-text').inputValue(),'Pasted task');
 await button('Cancel').click();assert.equal(await page.evaluate(()=>window.calls.length),0);
 await page.evaluate(()=>window.openPanel());assert.equal(await page.locator('#batch-text').inputValue(),'Pasted task');
 await page.locator('#batch-text').fill('- \nvalid');await button('Preview').click();assert.ok(await button('Create 2 tasks').isDisabled());
 await page.locator('.rows input[type=checkbox]').first().uncheck();assert.ok(await button('Create 1 tasks').isEnabled());
 await button('Back to text').click();await button('Preview').click();assert.equal(await page.locator('.rows input[type=checkbox]').first().isChecked(),false);
 await page.evaluate(()=>window.failRefresh=true);await button('Create 1 tasks').click();await page.getByText('Tasks were created, but the list could not refresh.',{exact:false}).waitFor();
 await page.evaluate(()=>window.failRefresh=false);await button('Refresh list').click();await page.waitForFunction(()=>window.refreshes===2);assert.equal(await page.evaluate(()=>window.calls.length),1);
 await go();await page.locator('#batch-text').fill('task');await button('Preview').click();await page.evaluate(()=>window.loseReply=true);
 await button('Create 1 tasks').click();await page.locator('[role=alert]').waitFor();await button('Cancel').click();await page.evaluate(()=>window.openPanel());
 await button('Retry same request').click();await page.locator('[role=status]').waitFor();assert.equal(await page.evaluate(()=>window.receipts.size),1);
 await go();await page.locator('#batch-text').fill(Array(51).fill('a').join('\n'));await button('Preview').click();await page.getByText('Use at most 50 non-empty lines per batch.').waitFor();
 await page.locator('#batch-text').fill(Array(50).fill('a').join('\n'));await button('Preview').click();assert.equal(await page.locator('.rows li').count(),50);
 assert.equal(await page.evaluate(()=>window.calls.length),0);await button('Deselect all').click();assert.ok(await button('Create 0 tasks').isDisabled());
 assert.deepEqual(errors,[]);console.log(count+' batch UI matrix cases plus paste/cancel/reopen/invalid/refresh/limits passed. Screenshots: '+output);
}finally{await browser?.close();await server.close();}
