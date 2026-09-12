import assert from 'node:assert/strict';
import path from 'node:path';
import { createRequire } from 'node:module';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = path.resolve(import.meta.dirname, '..');
// Production list/editor with isolated in-memory notes; never native storage.
const harness = `
import { mount, unmount } from 'svelte';
import NoteList from '/src/lib/components/NoteList.svelte';
import NoteEditor from '/src/lib/components/NoteEditor.svelte';
import { setLanguageMode } from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p = new URLSearchParams(location.search);
setLanguageMode(p.get('locale'));
document.documentElement.style.zoom = p.get('zoom');
const target = document.querySelector('main');
const positions = new Map();
const items = Array.from({length:40},(_,i)=>({id:i,uuid:'qa-'+i,title:'QA '+i,content:'Navigation scroll fixture',color:'default',pinned:false,updated_at:1789200000000}));
const noop = async()=>{};
let component;
const common = {onPin:noop,onColor:noop,onDelete:noop};
async function list(){ if(component) await unmount(component); component=mount(NoteList,{target,props:{items,scrollPositions:positions,onOpen:edit,...common}}); }
async function edit(note){await unmount(component);component=mount(NoteEditor,{target,props:{note,onChange:()=>{},onDone:async()=>{await list();return true;},...common,onAddImages:noop,onAddFiles:noop,onOpenAttachment:async()=>'',onOpenFile:noop,onMoveAttachment:noop,onDeleteAttachment:noop,onRetryAttachment:noop}});}
await list();
window.ready=true;
`;
const server = await createServer({root,configFile:false,logLevel:'error',resolve:{alias:{$lib:path.join(root,'src/lib')}},
  plugins:[svelte({configFile:false}),{name:'navigation-scroll-harness',resolveId:id=>id==='/__e5c.js'?'\0e5c':undefined,
    load:id=>id==='\0e5c'?harness:undefined,configureServer(server){server.middlewares.use((req,res,next)=>{
      if(!req.url?.startsWith('/__e5c.html'))return next();
      res.setHeader('Content-Type','text/html');
      res.end('<html><head><style>body{height:100dvh!important;padding:8px}main{height:100%;min-height:0;display:flex;flex-direction:column}.note-list{overflow:auto;min-height:0;flex:1}</style></head><body><main></main><script type="module" src="/__e5c.js"></script></body></html>');
    });}}],optimizeDeps:{noDiscovery:true,include:[]},server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try {
  await server.listen();
  browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL});
  for(const width of [280,380,800]) for(const locale of ['zh-CN','en-US']) for(const zoom of [1,1.5]) {
    const page=await browser.newPage({viewport:{width,height:900}});
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__e5c.html?locale=${locale}&zoom=${zoom}`);
    await page.waitForFunction(()=>window.ready);
    await page.locator('.note-list').evaluate(el=>{el.scrollTop=900;});
    await page.waitForTimeout(100);
    const before=await page.locator('.note-list').evaluate(el=>el.scrollTop);
    assert.ok(before>500);
    await page.locator('.note-list').evaluate(el=>{
      const bounds=el.getBoundingClientRect();
      const card=[...el.querySelectorAll('.note-card')].find(c=>{const r=c.getBoundingClientRect();return r.top>=bounds.top&&r.bottom<=bounds.bottom;});
      if(!card)throw Error('No visible fixture');
      card.querySelector('.note-card-body').click();
    });
    await page.getByRole('button',{name:locale==='en-US'?'Done':'完成',exact:true}).click();
    await page.waitForFunction(expected=>Math.abs((document.querySelector('.note-list')?.scrollTop??-1)-expected)<2,before);
    assert.deepEqual(errors,[]);
    console.log(`PASS ${width}px ${locale} zoom=${zoom}: restored ${before}`);
    await page.close();
  }
  console.log('12 browser list/editor round trips passed; not native Tauri acceptance.');
} finally {await browser?.close();await server.close();}
