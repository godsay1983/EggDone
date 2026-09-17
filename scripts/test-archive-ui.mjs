// Production Svelte UI/store with isolated IPC; not native database or S3 acceptance.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve,dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_PATH||'playwright');
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const output=resolve(tmpdir(),'eggdone-archive-ui-'+Date.now());mkdirSync(output,{recursive:true});
const native=`export const isTauri=()=>false;
export async function invoke(command,args){
 if(command==='list_archived'){
  if(window.failLoad)throw Error('read');
  const rows=window.rows.filter(r=>(r.title+r.content).includes(args.query));
  const offset=args.cursor?Number(args.cursor.uuid)+1:0;
  const items=rows.filter(r=>Number(r.expected.uuid)>=offset).slice(0,50);
  return {items:structuredClone(items),next:rows.length>offset+50?{uuid:items.at(-1).expected.uuid,archived_at:1,query:args.query}:null};
 }
 if(command==='preview_archived')return structuredClone(window.rows.find(r=>r.expected.uuid===args.uuid));
 if(command==='apply_archive_action'){
  window.writes.push(structuredClone(args));
  if(window.conflict)throw Error('ARCHIVE_CONFLICT');
  if(window.hold)await new Promise(r=>window.release=r);
  if(window.receipts[args.operation])return {...window.receipts[args.operation],outcome:'already_applied'};
  window.rows=window.rows.filter(r=>r.expected.uuid!==args.expected.uuid);
  const result={uuid:args.expected.uuid,action:args.action,outcome:'applied',result_version:2,warnings:[]};
  window.receipts[args.operation]=result;
  if(window.loseReply){window.loseReply=false;throw Error('lost reply');}
  return result;
 }
 throw Error('Unexpected IPC '+command);
}`;
const html=String.raw`<!doctype html><html><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/ArchiveDialog.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang'));
document.documentElement.dataset.theme=p.get('theme');document.documentElement.style.zoom=p.get('scale')||'1';
window.writes=[];window.receipts={};window.archiveClosed=false;window.failRefresh=false;window.failLoad=p.has('failLoad');
window.hold=false;window.loseReply=false;window.conflict=false;
window.rows=Array.from({length:p.has('pages')?51:2},(_,i)=>({expected:{uuid:String(i),scope:'scope',fingerprint:'hash'},
 title:i===0?'项目发布前的归档检查 Archived checklist '.repeat(3):'Task '+i,content:'Full body line\n'.repeat(14),
 completed:true,archived_at:1789190000000,updated_at:1,completed_at:1,due_date:'2026-09-17',due_at:null,group_name:'Work',
 checklist_json:JSON.stringify({items:[{uuid:'item',content:'Verify everything',sort_order:1,completed:true,deleted_at:null}]}),links_json:'{"links":[]}'}));
const instance=mount(Dialog,{target:document.body,props:{
 afterCommit:async()=>{if(window.failRefresh)throw Error('refresh');},
 onViewTask:async uuid=>{window.viewed=uuid;},
 onClose:()=>{window.archiveClosed=true;void unmount(instance);}
}});
</script></body></html>`;
const server=await createServer({root,configFile:false,
 resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'virtual:archive-ipc'},{find:'$lib',replacement:resolve(root,'src/lib')}],conditions:['browser']},
 plugins:[svelte(),{name:'archive-ui',resolveId:id=>id==='virtual:archive-ipc'?'\0archive-ipc':null,
 load:id=>id==='\0archive-ipc'?native:null,
 configureServer(s){s.middlewares.use('/__archive',async(_req,res)=>{res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/__archive',html));});}}],
 optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},
 server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}}
});
let browser,page;
try{
 await server.listen();browser=await chromium.launch({headless:true,channel:'msedge'});page=await browser.newPage();
 const errors=[];page.on('pageerror',e=>{errors.push(e.message);console.error(e.message);});page.setDefaultTimeout(15000);
 const url=server.resolvedUrls.local[0]+'__archive';
 let count=0;
 for(const lang of ['zh-CN','en-US'])for(const theme of ['light','dark'])
 for(const size of [{width:320,height:480},{width:480,height:720},{width:1000,height:760}])for(const scale of [1,1.5]){
  await page.setViewportSize(size);await page.goto(url+'?lang='+lang+'&theme='+theme+'&scale='+scale);
  const button=(zh,en)=>page.getByRole('button',{name:lang==='zh-CN'?zh:en,exact:true});
  await page.locator('.record').first().click();
  await button('重新打开','Reopen').click();await page.keyboard.press('Escape');
  assert.equal(await page.evaluate(()=>window.writes.length),0);
  assert.equal(await page.locator('.checklist input').isChecked(),true);
  await button('取消归档','Unarchive').click();
  await page.evaluate(()=>{window.hold=true;window.loseReply=true;});
  await button('确认操作','Confirm action').click();await page.waitForFunction(()=>!!window.release);
  await page.keyboard.press('Escape');assert.equal(await page.evaluate(()=>window.archiveClosed),false);
  assert(await button('确认操作','Confirm action').isDisabled());
  await page.evaluate(()=>{window.hold=false;window.release();});
  await page.getByRole('status').filter({hasText:lang==='zh-CN'?'原样重试':'same action'}).waitFor();
  await button('确认操作','Confirm action').click();await page.locator('.record').first().waitFor();
  assert.equal(await page.evaluate(()=>window.rows.length),1);
  assert.deepEqual(await page.evaluate(()=>window.writes[0]),await page.evaluate(()=>window.writes[1]));
  await button('查看任务','View task').click();assert.equal(await page.evaluate(()=>window.viewed),'0');
  await page.locator('.record').first().click();
  assert(await page.locator('dialog').evaluate(e=>e.scrollWidth<=e.clientWidth),'dialog horizontal overflow');
  for(const b of await page.locator('footer button').all()){
   const box=await b.boundingBox();assert(box.x>=0&&box.y>=0&&box.x+box.width<=size.width+1&&box.y+box.height<=size.height+1,'footer clipped');
  }
  if(size.width===320||size.width===1000)await page.screenshot({path:resolve(output,lang+'-'+theme+'-'+size.width+'-'+scale+'.png')});
  await button('移入回收站','Move to trash').click();await page.evaluate(()=>window.failRefresh=true);
  await button('确认操作','Confirm action').click();await page.waitForFunction(()=>window.rows.length===0);
  await page.getByRole('status').filter({hasText:lang==='zh-CN'?'页面刷新失败':'could not refresh'}).waitFor();
  await page.evaluate(()=>window.failRefresh=false);await button('刷新','Refresh').click();
  assert.equal(await page.evaluate(()=>window.writes.length),3);
  await page.keyboard.press('Escape');await page.waitForFunction(()=>window.archiveClosed);count++;
 }
 await page.setViewportSize({width:480,height:720});
 await page.goto(url+'?lang=en-US&theme=light&failLoad=1&pages=1');
 await page.getByRole('alert').waitFor();assert.equal(await page.locator('.record').count(),0);
 await page.evaluate(()=>window.failLoad=false);await page.getByRole('button',{name:'Refresh',exact:true}).click();
 await page.waitForFunction(()=>document.querySelectorAll('.record').length===50);
 await page.getByRole('button',{name:'Load more',exact:true}).click();
 await page.waitForFunction(()=>document.querySelectorAll('.record').length===51);
 await page.getByRole('textbox').fill('Task 50');await page.getByRole('button',{name:'Search',exact:true}).click();
 await page.waitForFunction(()=>document.querySelectorAll('.record').length===1);
 await page.locator('.record').click();await page.getByRole('button',{name:'Reopen',exact:true}).click();
 await page.evaluate(()=>window.conflict=true);await page.getByRole('button',{name:'Confirm action',exact:true}).click();
 assert(await page.getByRole('button',{name:'Confirm action',exact:true}).isDisabled());
 await page.keyboard.press('Escape');await page.keyboard.press('Escape');
 assert.equal(await page.getByRole('textbox').inputValue(),'Task 50');
 assert.equal(await page.locator('.record').count(),1);
 assert.deepEqual(errors,[]);
 console.log('PASS: '+count+' archive locale/theme/window/zoom combinations, cancel, response-loss retry, busy/back, refresh-only retry, pagination, search, conflict. Screenshots: '+output);
}catch(e){
 if(page)await page.screenshot({path:resolve(output,'failure.png')});
 console.error('Screenshots: '+output);throw e;
}finally{await browser?.close();await server.close();}
