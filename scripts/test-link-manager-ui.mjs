// Real components and stores with isolated IPC; native database CAS is covered separately.
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
const output=resolve(tmpdir(),'eggdone-link-manager-'+Date.now());mkdirSync(output,{recursive:true});
const native=`export const isTauri=()=>false;
export async function invoke(command,args){
  if(command==='list_notes'||command==='list_todos')return ['existing','candidate'].map(uuid=>({uuid,title:uuid==='candidate'?'Candidate with a long title '.repeat(4):'Existing'}));
  if(command==='list_task_note_links'){
    if(window.failLoad)throw Error('read failed');
    return Object.values(window.rows).filter(r=>r.deleted_at===null).map(link=>({link,todo_title:link.uuid,note_title:link.uuid,todo_state:'active',note_state:'active',is_repeating:true}));
  }
  const key=args.todoUuid+':'+args.noteUuid;
  if(command==='get_task_note_link')return structuredClone(window.rows[key]||null);
  if(command==='change_task_note_link'){
    window.calls.push(structuredClone(args));window.events.push('write');
    if(window.forceConflict){window.forceConflict=false;window.rows[key].updated_at++;throw Error('CONFLICT');}
    if(JSON.stringify(window.rows[key]||null)!==JSON.stringify(args.expected))throw Error('CONFLICT');
    const updated_at=(window.rows[key]?.updated_at||1)+1;
    return window.rows[key]={uuid:key,todo_uuid:args.todoUuid,note_uuid:args.noteUuid,created_at:1,updated_at,updated_by:'test',deleted_at:args.active?null:updated_at};
  }
  throw Error('Unexpected IPC '+command);
}`;
const html=`<!doctype html><html><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/LinkManagerDialog.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang'));document.documentElement.dataset.theme=p.get('theme');
const scope=p.get('scope');window.calls=[];window.events=[];window.managerClosed=false;window.rows={};window.failRefresh=false;window.failSource=false;window.failLoad=p.has('failLoad');
for(const target of ['existing','candidate']){
 const todo_uuid=scope==='todo'?'source':target,note_uuid=scope==='todo'?target:'source',key=todo_uuid+':'+note_uuid;
 window.rows[key]={uuid:key,todo_uuid,note_uuid,created_at:1,updated_at:2,updated_by:'test',deleted_at:target==='existing'?null:2};
}
const instance=mount(Dialog,{target:document.body,props:{scope,uuid:'source',title:'Source title',
 saveSource:async()=>{window.events.push('source');if(window.failSource)throw Error('source failed');},
 afterCommit:async()=>{window.events.push('refresh');if(window.failRefresh)throw Error('refresh failed');},
 onClose:()=>{window.managerClosed=true;void unmount(instance);}}});window.ready=true;
</script></body></html>`;
const server=await createServer({root,configFile:false,resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'virtual:manager-ipc'},{find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser']},
plugins:[svelte(),{name:'manager-ui',resolveId:id=>id==='virtual:manager-ipc'?'\0manager-ipc':null,load:id=>id==='\0manager-ipc'?native:null,
configureServer(s){s.middlewares.use('/__manager',async(_req,res)=>{res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/__manager',html));});}}],
optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},server:{host:'127.0.0.1',port:5191,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try{
 await server.listen();browser=await chromium.launch({headless:true,channel:'msedge'});const page=await browser.newPage();const errors=[];
 page.on('pageerror',e=>{errors.push(e.message);console.error('Browser:',e.message);});page.setDefaultTimeout(15000);let count=0;
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])for(const scope of ['todo','note'])for(const size of [{width:320,height:430},{width:480,height:720},{width:1000,height:760}]){
  await page.setViewportSize(size);await page.goto(server.resolvedUrls.local[0]+'__manager?lang='+lang+'&theme='+theme+'&scope='+scope);
  await page.locator('.candidate').waitFor();
  const confirm=()=>page.getByRole('button',{name:lang==='zh-CN'?'确认':'Confirm',exact:true});
  const cancel=()=>page.getByRole('button',{name:lang==='zh-CN'?'取消':'Cancel',exact:true});
  const unlink=()=>page.getByRole('button',{name:lang==='zh-CN'?'解除关联':'Unlink',exact:true});
  await unlink().click();await cancel().click();assert.equal(await page.evaluate(()=>window.calls.length),0);
  await page.locator('input[type=search]').fill('Candidate');await page.locator('.candidate').click();
  await page.evaluate(()=>window.forceConflict=true);await confirm().click();await page.locator('.candidate').waitFor();
  assert.equal(await page.evaluate(()=>window.calls.length),1);
  assert.equal(await page.evaluate(()=>Object.values(window.rows).filter(r=>r.deleted_at===null).length),1);
  await page.locator('.candidate').click();await confirm().click();
  await page.waitForFunction(()=>Object.values(window.rows).filter(r=>r.deleted_at===null).length===2);
  assert.equal(await unlink().count(),2);assert.deepEqual(await page.evaluate(()=>window.events),['source','write','source','write','refresh']);
  await unlink().last().click();await page.evaluate(()=>window.failSource=true);await confirm().click();await page.locator('input[type=search]').waitFor();
  assert.equal(await page.evaluate(()=>window.calls.length),2);
  await page.evaluate(()=>{window.failSource=false;window.failRefresh=true;});
  await unlink().last().click();await confirm().click();await page.waitForFunction(()=>window.calls.length===3);
  await page.locator('.candidate').waitFor();
  assert.equal(await page.evaluate(()=>Object.values(window.rows).filter(r=>r.deleted_at===null).length),1);
  assert.equal(await page.locator('dialog header p').textContent(),'Source title');
  assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'dialog overflow');
  for(const button of await page.locator('footer button').all()){const b=await button.boundingBox();assert.ok(b.x>=0&&b.y>=0&&b.x+b.width<=size.width&&b.y+b.height<=size.height,'footer outside viewport');}
  if(size.width===320)await page.screenshot({path:resolve(output,lang+'-'+theme+'-'+scope+'.png')});
  await page.keyboard.press('Escape');await page.waitForFunction(()=>window.managerClosed);count++;
 }
 await page.goto(server.resolvedUrls.local[0]+'__manager?lang=en-US&theme=light&scope=todo&failLoad=1');
 await page.getByRole('alert').waitFor();assert.equal(await page.locator('.candidate').count(),0);
 await page.evaluate(()=>window.failLoad=false);await page.getByRole('button',{name:'Retry',exact:true}).click();await page.locator('.candidate').waitFor();
 assert.deepEqual(errors,[]);console.log('Link manager '+count+' scope/theme/language/size combinations and read retry passed; screenshots: '+output);
}finally{await browser?.close();await server.close();}
