import assert from 'node:assert/strict';
import path from 'node:path';
import fs from 'node:fs/promises';
import os from 'node:os';
import { createRequire } from 'node:module';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = path.resolve(import.meta.dirname, '..');
const artifacts = await fs.mkdtemp(path.join(os.tmpdir(), 'eggdone-pins-'));
const harness = `
import {mount} from 'svelte';
import TodoPanel from '/src/lib/components/TodoPanel.svelte';
import {initializeLanguage,setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const params=new URLSearchParams(location.search);
await setLanguageMode(params.get('locale'));initializeLanguage();
localStorage.setItem('eggdone-theme',params.get('theme'));
const write=Storage.prototype.setItem,read=Storage.prototype.getItem;
window.failPins='';
Storage.prototype.setItem=function(key,value){if(key==='eggdone-pinned-smart-views'&&window.failPins==='write')throw Error('full');return write.call(this,key,value)};
Storage.prototype.getItem=function(key){if(key==='eggdone-pinned-smart-views'&&window.failPins==='read')throw Error('denied');return read.call(this,key)};
mount(TodoPanel,{target:document.querySelector('#app')});window.ready=true;
`;
const server = await createServer({root, configFile:false, logLevel:'error',
  resolve:{alias:{$lib:path.join(root,'src/lib')}}, plugins:[svelte({configFile:false}), {
    name:'pin-harness', resolveId:id=>id==='/__pins.js'?'\0pins':undefined,
    load:id=>id==='\0pins'?harness:undefined, configureServer(server){
      server.middlewares.use((req,res,next)=>{
        if(!req.url?.startsWith('/__pins.html'))return next();
        res.setHeader('Content-Type','text/html');
        res.end('<html><body><div id="app"></div><script type="module" src="/__pins.js"></script></body></html>');
      });
    }
  }], optimizeDeps:{noDiscovery:true,include:[]}, server:{host:'127.0.0.1',port:0,watch:{ignored:['**/src-tauri/**']}}});
let browser;
try {
  await server.listen(); browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL});
  for(const locale of ['zh-CN','en-US']) for(const theme of ['light','dark']) for(const width of [380,640,1100]) {
    const page=await browser.newPage({viewport:{width,height:900}});
    const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__pins.html?locale=${locale}&theme=${theme}`);
    await page.waitForFunction(()=>window.ready);
    const menu=page.locator('.summary-menu-button');
    await menu.click();
    const pins=page.locator('.smart-pin');
    await pins.first().waitFor(); await pins.first().click(); await pins.nth(4).click();
    assert.equal(await pins.nth(1).isDisabled(),true);
    assert.equal(await page.locator('.pinned-smart-views button').count(),2);
    await menu.click();
    const activeLabel=await page.locator('.pinned-smart-views button').first().innerText();
    await page.locator('.pinned-smart-views button').first().click();
    await page.locator('.smart-filter').waitFor();
    await menu.click(); await pins.first().click();
    assert.match(await page.locator('.smart-filter').innerText(),new RegExp(activeLabel));
    assert.equal(await page.locator('.pinned-smart-views button').count(),1);
    await pins.nth(1).click(); await menu.click();
    await page.reload(); await page.waitForFunction(()=>window.ready);
    assert.equal(await page.locator('.pinned-smart-views button').count(),2);
    await page.evaluate(()=>document.documentElement.style.zoom='1.25');
    const moreBounds=await menu.boundingBox();
    assert.ok(moreBounds && moreBounds.x>=0 && moreBounds.x+moreBounds.width<=width, 'More stays visible at 125%');
    await page.screenshot({path:path.join(artifacts,`${locale}-${theme}-${width}.png`)});
    for(const item of await page.locator('.pinned-smart-views button').all()) {
      assert.equal(await item.evaluate(el=>el.scrollWidth>el.clientWidth+1),false);
    }
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1),false);
    await menu.click();
    await page.evaluate(()=>window.failPins='write');
    await pins.nth(4).click(); await page.locator('.pinned-feedback').waitFor();
    assert.equal(await page.locator('.pinned-smart-views button').count(),2);
    await page.evaluate(()=>window.failPins=''); await page.locator('.pinned-feedback button').click();
    assert.equal(await page.locator('.pinned-smart-views button').count(),1);
    await page.evaluate(()=>window.failPins='read'); await pins.nth(1).click();
    assert.equal(await page.locator('.pinned-smart-views button').count(),1);
    await page.evaluate(()=>window.failPins=''); await page.locator('.pinned-feedback button').click();
    assert.equal(await page.locator('.pinned-smart-views').count(),0);
    assert.deepEqual(errors,[]);
    console.log(`PASS ${locale} ${theme} ${width}: pin limit, unpin selection isolation, restart, 125% layout, read/write retry`);
    await page.close();
  }
  console.log(`Screenshots: ${artifacts}`);
} finally { await browser?.close(); await server.close(); }
