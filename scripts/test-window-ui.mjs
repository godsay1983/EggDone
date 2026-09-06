// Real Svelte settings and controller, with a deterministic native-window substitute.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const native = `
export class LogicalSize { constructor(width,height) { this.width=width; this.height=height; } }
export class PhysicalPosition { constructor(x,y) { this.x=x; this.y=y; } }
window.native = { size:{width:360,height:560}, position:{x:1500,y:450}, zoom:1, calls:[], events:{}, fail:false,
 monitor:{ scaleFactor:1, workArea:{position:{x:0,y:0},size:{width:1920,height:1040}} } };
const n=window.native;
const event=key=>async callback=>{n.events[key]=callback;return ()=>delete n.events[key];};
const win={ innerSize:async()=>n.size, outerSize:async()=>n.size, scaleFactor:async()=>n.monitor.scaleFactor,
 outerPosition:async()=>n.position, setSize:async size=>{n.size={width:size.width*n.monitor.scaleFactor,height:size.height*n.monitor.scaleFactor};n.calls.push('size');},
 setMinSize:async size=>{n.min=size;},setMaxSize:async size=>{n.max=size;},setPosition:async pos=>{n.position=pos;},
 onResized:event('resize'),onScaleChanged:event('scale'),onFocusChanged:event('focus'),onMoved:event('move'),
 startResizeDragging:async direction=>{n.direction=direction;} };
export const getCurrentWindow=()=>win;
export const currentMonitor=async()=>n.monitor;
export const primaryMonitor=currentMonitor;
export const isTauri=()=>true;
export const invoke=async command=>{n.calls.push(command);};
export const getCurrentWebview=()=>({setZoom:async zoom=>{if(n.fail){n.fail=false;throw Error('injected');}n.zoom=zoom;}});
`;
const html = `<!doctype html><html><head><meta charset="utf-8"></head><body>
<div class="settings-backdrop"><section class="settings-card"><div id="settings"></div></section></div>
<script type="module">
import { mount } from 'svelte';
import Settings from '/src/lib/components/WindowSettings.svelte';
import Controls from '/src/lib/components/WindowControls.svelte';
import { initializeWindowPreferences,updateWindowPreferences } from '/src/lib/stores/windowPreferences.ts';
import { setLanguageMode } from '/src/lib/i18n/index.ts';
import '/src/app.css';
const params=new URLSearchParams(location.search);
setLanguageMode(params.get('lang'));
document.documentElement.dataset.theme=params.get('theme');
window.updatePrefs=updateWindowPreferences;
window.stopPrefs=await initializeWindowPreferences();
mount(Settings,{target:document.getElementById('settings')});
mount(Controls,{target:document.body});
window.ready=true;
</script></body></html>`;
const server = await createServer({ root, configFile: false,
  resolve: { alias: [{find:/^@tauri-apps\/api\/(window|core|webview)$/,replacement:'virtual:native-window'},
    {find:'$lib',replacement:resolve(root,'src/lib')}], conditions:['browser'] },
  plugins:[svelte(),{name:'window-check', resolveId(id){if(id==='virtual:native-window')return '\0native-window';},
    load(id){if(id==='\0native-window')return native;}, configureServer(server){
      server.middlewares.use('/__window-test',async(_request,response)=>{
        response.setHeader('Content-Type','text/html; charset=utf-8');
        response.end(await server.transformIndexHtml('/__window-test',html));
      });
    }}],
  optimizeDeps:{noDiscovery:true,include:['svelte','svelte/store']},
  server:{host:'127.0.0.1',port:5188,watch:{ignored:['**/src-tauri/**']}},
});
let browser;
try {
  await server.listen();
  browser=await chromium.launch({headless:true,channel:'msedge'});
  const page=await browser.newPage();
  const errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  const base=server.resolvedUrls.local[0]+'__window-test';
  let cases=0;
  for(const lang of ['zh-CN','en-US']) for(const theme of ['light','dark']) for(const width of [320,360,480,800]) {
    await page.setViewportSize({width,height:600});
    await page.goto(base+'?lang='+lang+'&theme='+theme);
    await page.waitForFunction(()=>window.ready);
    assert.equal(await page.locator('.settings-card').evaluate(el=>el.scrollWidth<=el.clientWidth),true);
    const zoomLayout = await page.locator('.window-settings').evaluate(section => {
      const presets = section.querySelector('.language-options').getBoundingClientRect();
      const label = section.querySelector('.window-zoom > span').getBoundingClientRect();
      const select = section.querySelector('select').getBoundingClientRect();
      return { gap: select.top - presets.bottom, centerOffset: Math.abs((label.top + label.height / 2) - (select.top + select.height / 2)),
        width: select.width, aligned: Math.abs(select.right - presets.right) < 1, separated: label.right < select.left };
    });
    assert.ok(zoomLayout.gap >= 11, 'zoom row needs separation from presets');
    assert.ok(zoomLayout.centerOffset < 1 && zoomLayout.aligned && zoomLayout.separated, 'zoom label/control must align without overlap');
    assert.equal(zoomLayout.width, 96);
    for(const button of await page.locator('.language-options button').all()) {
      assert.equal(await button.evaluate(el=>el.scrollWidth<=el.clientWidth),true,'button label must fit');
    }
    await page.locator('.language-options button').nth(1).click();
    await page.waitForFunction(()=>window.native.zoom===1.25);
    assert.equal(await page.locator('select').inputValue(),'1.25');
    await page.locator('select').selectOption('1.5');
    await page.waitForFunction(()=>window.native.zoom===1.5);
    assert.equal(await page.evaluate(()=>window.native.min.width),540);
    await page.keyboard.press('Control+-');
    await page.waitForFunction(()=>window.native.zoom===1.25);
    await page.keyboard.press('Control+0');
    await page.waitForFunction(()=>window.native.zoom===1);
    await page.locator('.language-options button').last().click();
    await page.waitForFunction(()=>window.native.size.width===360);
    if(process.env.WINDOW_SCREENSHOT_DIR && width===360) await page.screenshot({path:resolve(process.env.WINDOW_SCREENSHOT_DIR,lang+'-'+theme+'.png')});
    cases++;
  }
  // Persistence and DPI conversion use logical rather than physical pixels.
  await page.evaluate(async()=>{await window.updatePrefs({width:700,height:760,zoom:1.25});});
  await page.reload(); await page.waitForFunction(()=>window.ready);
  assert.equal(await page.evaluate(()=>window.native.size.width),700);
  assert.equal(await page.evaluate(()=>window.native.zoom),1.25);
  await page.evaluate(()=>{window.native.monitor.scaleFactor=2;window.native.size={width:1400,height:1520};window.native.events.resize({});window.native.events.focus({payload:true});});
  await page.waitForFunction(()=>JSON.parse(localStorage.getItem('eggdone-window-preferences-v1')).width===700);
  await page.waitForTimeout(500);
  assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('eggdone-window-preferences-v1')).width),700);
  // A display change must not leave an oversized window offscreen.
  await page.evaluate(()=>{window.native.monitor={scaleFactor:1,workArea:{position:{x:-800,y:0},size:{width:800,height:600}}};window.native.events.move({});});
  await page.waitForFunction(()=>window.native.max.height===584);
  assert.ok(await page.evaluate(()=>window.native.position.x<0 && window.native.size.height<=584));
  // Failed zoom must not overwrite the previous preference and must report failure.
  await page.evaluate(async()=>{window.native.fail=true;await window.updatePrefs({zoom:1.5});});
  await page.locator('[role="alert"]').waitFor();
  assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('eggdone-window-preferences-v1')).zoom),1.25);
  await page.locator('[data-direction="SouthEast"]').dispatchEvent('pointerdown',{button:0});
  await page.waitForFunction(()=>window.native.direction==='SouthEast');
  await page.evaluate(()=>window.stopPrefs());
  assert.equal(await page.evaluate(()=>Object.keys(window.native.events).length),0);
  assert.deepEqual(errors,[]);
  console.log('Window UI: '+cases+' language/theme/width combinations passed; preset, zoom, keyboard, restore, DPI, display fitting, error rollback, resize IPC, cleanup passed. Native transport is mocked.');
} catch(error) { console.error(error); process.exitCode=1; }
finally { await browser?.close(); await server.close(); }
