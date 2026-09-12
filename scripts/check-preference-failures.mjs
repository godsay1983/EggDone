import assert from 'node:assert/strict';
import path from 'node:path';
import { createRequire } from 'node:module';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_PATH||'playwright');
const root=path.resolve(import.meta.dirname,'..');
const harness=`
import {mount} from 'svelte';
import TodoPanel from '/src/lib/components/TodoPanel.svelte';
import FocusWindow from '/src/lib/components/FocusWindow.svelte';
import SettingsPanel from '/src/lib/components/SettingsPanel.svelte';
import {initializeLanguage,setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);
await setLanguageMode(p.get('locale'));
localStorage.setItem('eggdone-focus-duration-minutes','45');
const read=Storage.prototype.getItem,write=Storage.prototype.setItem;
window.failStorage=true;
Storage.prototype.getItem=function(key){if(window.failStorage&&p.get('failure')==='read')throw Error('injected read failure');return read.call(this,key)};
Storage.prototype.setItem=function(key,value){if(window.failStorage&&p.get('failure')==='write')throw Error('injected write failure');return write.call(this,key,value)};
initializeLanguage();
const view=p.get('view');
const props=view==='settings'?{settings:{shortcut:'Alt+Shift+Space',shortcutEnabled:false,noteShortcut:'Alt+Shift+N',noteShortcutEnabled:false,autostartEnabled:false,shortcutError:null,noteShortcutError:null,autostartError:null},defaultListViewMode:'remember',onClose:()=>{},onChange:()=>{},onDefaultListViewChange:()=>{}}:{};
mount(view==='main'?TodoPanel:view==='focus'?FocusWindow:SettingsPanel,{target:document.querySelector('#app'),props});
window.ready=true;
`;
const server=await createServer({root,configFile:false,logLevel:'error',resolve:{alias:{$lib:path.join(root,'src/lib')}},
  plugins:[svelte({configFile:false}),{name:'preference-failure-harness',resolveId:id=>id==='/__prefs.js'?'\0prefs':undefined,
    load:id=>id==='\0prefs'?harness:undefined,configureServer(server){server.middlewares.use((req,res,next)=>{
      if(!req.url?.startsWith('/__prefs.html'))return next();res.setHeader('Content-Type','text/html');
      res.end('<html><head></head><body><div id="app"></div><script type="module" src="/__prefs.js"></script></body></html>');
    });}}],optimizeDeps:{noDiscovery:true,include:[]},server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try{
  await server.listen();browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL});
  for(const locale of ['en-US','zh-CN'])for(const view of ['main','focus','settings']){
    const page=await browser.newPage({viewport:{width:380,height:900}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
    const failure=view==='settings'?'write':'read';
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__prefs.html?view=${view}&failure=${failure}&locale=${locale}`);
    await page.waitForFunction(()=>window.ready);
    if(view==='main')assert.ok(await page.locator('.panel-shell input').count()>0);
    if(view==='focus')await page.locator('.focus-window-shell').waitFor();
    if(view==='settings'){
      await page.locator('select').last().selectOption('today');
      await page.waitForFunction(()=>[...document.querySelectorAll('select')].at(-1).value==='remember');
      const buttons=page.locator('.duration-options').first().locator('button');
      await buttons.first().click();assert.match(await page.locator('.duration-options').first().locator('button.active').innerText(),/45/);
      await page.locator('.preference-status').waitFor();
      await page.evaluate(()=>{window.failStorage=false});await buttons.first().click();
      assert.match(await page.locator('.duration-options').first().locator('button.active').innerText(),/15/);
      assert.equal(await page.locator('.preference-status').count(),0);
    }else await page.locator('.preference-status').waitFor();
    assert.deepEqual(errors,[]);console.log('PASS '+locale+' '+view+' '+failure+' failure');await page.close();
  }
}finally{await browser?.close();await server.close();}
