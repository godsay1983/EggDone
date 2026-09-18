// Production UI and store with isolated IPC, not native database or cloud acceptance.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(tmpdir(), 'eggdone-trash-ui-' + Date.now());
mkdirSync(output, { recursive: true });
const native = `export const isTauri=()=>false;
export async function invoke(command,args){
  if(command==='migration_local_backup'){
    window.backupCalls=(window.backupCalls||[]).concat(args.action);
    if(args.action==='publish'){
      if(args.expected!=='preview-digest')throw Error('MIGRATION_PUBLICATION_CONFIRMATION');
      window.publicationConfirmed=true;
      if(window.publicationFail){window.publicationFail=false;throw Error('MIGRATION_PUBLICATION_NETWORK');}
      window.publicationDone=true;
    }
    if(args.action==='status')return null;
    if(window.backupFail){window.backupFail=false;throw Error('MIGRATION_BACKUP_ASSET');}
    if(window.backupRemoteFail){const code=window.backupRemoteFail;window.backupRemoteFail=null;throw Error(code);}
    if(window.backupChanged)throw Error('MIGRATION_BACKUP_CHANGED');
    if(window.recoveryFail)throw Error('MIGRATION_RECOVERY_FAILED');
    if(args.action==='preparePublication')window.publicationPrepared=true;
    return {operation:'backup-operation',files:2,bytes:1024,verifiedAt:1,current:true,blockers:window.settled?[]:['pending_notes'],
      publication:window.publicationPrepared?{operation:'staging-operation',digest:'preview-digest',total:3,completed:window.publicationDone?3:0,bytes:4096,confirmed:!!window.publicationConfirmed,published:!!window.publicationDone,current:true}:null,
      cloud:['prepareCloud','preparePublication','publish'].includes(args.action)?{operation:'cloud-operation',objects:8,files:3,bytes:4096,verifiedAt:2,current:true}:null};
  }
  if(command==='unfinished_trash_purge')return null;
  if(command==='prepare_trash_purge'){
    if(window.legacy)throw Error('PURGE_MIGRATION_REQUIRED');
    window.purgeTargets=structuredClone(args.selected||window.rows.map(({kind,uuid})=>({kind,uuid})));
    return window.purgePlan={operation_uuid:'test-operation',total:window.purgeTargets.length,attachments:1,bytes:100,
      state:'prepared',pending:window.purgeTargets.length,purged:0,skipped:0,cleanup_pending:0};
  }
  if(command==='run_trash_purge'){
    window.purgeCalls.push(args.operationUuid);
    if(window.purgeFail){window.purgeFail=false;throw Error('PURGE_DATABASE_FAILED');}
    window.rows=window.rows.filter(row=>!window.purgeTargets.some(t=>t.kind===row.kind&&t.uuid===row.uuid));
    return {...window.purgePlan,state:'complete',pending:0,purged:window.purgeTargets.length};
  }
  if(command==='list_trash'){if(window.failLoad)throw Error('read');return structuredClone(window.rows.slice(args.offset,args.offset+args.limit));}
  if(command==='preview_trash')return structuredClone(window.rows.find(r=>r.uuid===args.uuid));
  if(command==='restore_trash'){
    window.writes.push(args.expected);
    if(window.failWrite){window.failWrite=false;throw Error('TRASH_CONFLICT');}
    if(window.hold)await new Promise(resolve=>window.release=resolve);
    window.rows=window.rows.filter(r=>r.uuid!==args.expected.uuid);return;
  }
  throw Error('Unexpected IPC '+command);
}`;
const html = String.raw`<!doctype html><html><body><script type="module">
import {mount,unmount} from 'svelte';
import Dialog from '/src/lib/components/TrashDialog.svelte';
import {setLanguageMode} from '/src/lib/i18n/index.ts';
import '/src/app.css';
const p=new URLSearchParams(location.search);setLanguageMode(p.get('lang'));
document.documentElement.dataset.theme=p.get('theme');document.documentElement.style.zoom=p.get('scale')||'1';
window.writes=[];window.purgeCalls=[];window.trashClosed=false;window.failRefresh=false;window.failLoad=p.has('failLoad');window.hold=false;
window.rows=Array.from({length:p.has('pages')?51:2},(_,i)=>({kind:i%2?'note':'todo',uuid:String(i),
title:i===0?'Long task title '.repeat(7):'Note '+i,content:'Full body line\n'.repeat(35),
deleted_at:1789190000000,updated_at:1789190000000,updated_by:'test',completed:true,repeating:true,
attachments:i%2?[{uuid:'a',name:'Attachment-'.repeat(20)+'.md',updated_at:1,updated_by:'test',deleted_at:1}]:[]}));
const instance=mount(Dialog,{target:document.body,props:{afterCommit:async()=>{if(window.failRefresh)throw Error('refresh');},
onClose:()=>{window.trashClosed=true;void unmount(instance);}}});
</script></body></html>`;
const server = await createServer({ root, configFile: false,
  resolve: { alias: [{ find: '@tauri-apps/api/core', replacement: 'virtual:trash-ipc' }, { find: '$lib', replacement: resolve(root, 'src/lib') }], conditions: ['browser'] },
  plugins: [svelte(), { name: 'trash-ui', resolveId: id => id === 'virtual:trash-ipc' ? '\0trash-ipc' : null,
    load: id => id === '\0trash-ipc' ? native : null,
    configureServer(s) { s.middlewares.use('/__trash', async (_req, res) => { res.setHeader('Content-Type', 'text/html'); res.end(await s.transformIndexHtml('/__trash', html)); }); } }],
  optimizeDeps: { noDiscovery: true, include: ['svelte', 'svelte/store'] },
  server: { host: '127.0.0.1', port: 0, watch: { ignored: ['**/src-tauri/**'] } }
});
let browser, page;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true, channel: 'msedge' });
  page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => { errors.push(error.message); console.error('Browser:', error.message); });
  page.setDefaultTimeout(15000);
  const url = server.resolvedUrls.local[0] + '__trash';
  let count = 0;
  for (const lang of ['zh-CN', 'en-US']) for (const theme of ['light', 'dark'])
    for (const size of [{ width: 320, height: 430 }, { width: 480, height: 720 }, { width: 1000, height: 760 }]) for (const scale of [1, 1.5]) {
      console.log('Checking', lang, theme, size.width, scale);
      await page.setViewportSize(size);
      await page.goto(url + '?lang=' + lang + '&theme=' + theme + '&scale=' + scale);
      await page.locator('.record').first().waitFor();
      assert.equal(await page.locator('footer').count(),0,'ordinary list has no action footer');
      assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'list horizontal overflow');
      await page.screenshot({path:resolve(output,'list-'+lang+'-'+theme+'-'+size.width+'-'+scale+'.png')});
      const restore = () => page.getByRole('button', { name: lang === 'zh-CN' ? '确认恢复' : 'Restore', exact: true });
      await page.locator('.record').first().click();
      await restore().waitFor();
      await page.keyboard.press('Escape');
      assert.equal(await page.evaluate(() => window.writes.length), 0);
      await page.locator('.record').first().click();
      await page.evaluate(() => window.failWrite = true);
      await restore().click(); await page.locator('.record').first().waitFor();
      assert.equal(await page.evaluate(() => window.rows.length), 2);
      await page.locator('.record').first().click();
      await page.evaluate(() => window.hold = true);
      await restore().click();
      await page.waitForFunction(() => !!window.release);
      assert.equal(await restore().isDisabled(), true);
      await page.keyboard.press('Escape');
      assert.equal(await page.evaluate(() => window.trashClosed), false);
      assert.equal(await page.locator('dialog').evaluate(el => el.open), true);
      await page.evaluate(() => { window.hold = false; window.release(); });
      await page.waitForFunction(() => window.rows.length === 1);
      await page.locator('.record').first().waitFor();
      await page.locator('.record').first().click();
      await page.locator('.attachments li').waitFor();
      assert.equal(await page.locator('.attachments li').count(), 1);
      assert.ok(await page.locator('dialog').evaluate(el => el.scrollWidth <= el.clientWidth), 'horizontal overflow');
      for (const button of await page.locator('footer button').all()) {
        const b = await button.boundingBox();
        assert.ok(b.x >= 0 && b.y >= 0 && b.x + b.width <= size.width + 1 && b.y + b.height <= size.height + 1, 'footer clipped');
      }
      if (size.width === 320) await page.screenshot({ path: resolve(output, lang + '-' + theme + '-' + scale + '.png') });
      await page.evaluate(() => window.failRefresh = true);
      await restore().click();
      await page.waitForFunction(() => window.rows.length === 0);
      await page.getByRole('status').filter({ hasText: lang === 'zh-CN' ? '页面刷新' : 'could not refresh' }).waitFor();
      await page.evaluate(() => window.failRefresh = false);
      await page.getByRole('button', { name: lang === 'zh-CN' ? '刷新列表' : 'Refresh list', exact: true }).click();
      assert.equal(await page.evaluate(() => window.writes.length), 3);
      await page.keyboard.press('Escape'); await page.waitForFunction(() => window.trashClosed); count++;
    }
  await page.goto(url + '?lang=en-US&theme=light&failLoad=1&pages=1');
  await page.getByRole('alert').waitFor(); assert.equal(await page.locator('.record').count(), 0);
  await page.evaluate(() => window.failLoad = false);
  await page.getByRole('button', { name: 'Refresh list', exact: true }).click();
  await page.locator('.record').first().waitFor(); assert.equal(await page.locator('.record').count(), 50);
  await page.getByRole('button', { name: 'Load more', exact: true }).click();
  await page.waitForFunction(() => document.querySelectorAll('.record').length === 51);
  for (const lang of ['zh-CN','en-US']) for (const theme of ['light','dark']) {
    await page.setViewportSize({width:480,height:720});
    await page.goto(url+'?lang='+lang+'&theme='+theme+'&scale=1.5');
    await page.locator('.record').first().waitFor();
    const all=lang==='zh-CN'?'清空回收站':'Empty trash';
    await page.evaluate(()=>window.legacy=true);
    await page.getByRole('button',{name:all,exact:true}).click();
    await page.getByRole('status').filter({hasText:lang==='zh-CN'?'同步空间':'sync space'}).waitFor();
    assert.equal(await page.evaluate(()=>window.purgeCalls.length),0);
    await page.getByRole('button',{name:lang==='zh-CN'?'迁移准备':'Migration preparation',exact:true}).click();
    const prepareBackup=page.getByRole('button',{name:lang==='zh-CN'?'准备本机快照':'Prepare local snapshot',exact:true});
    await prepareBackup.waitFor();
    await page.evaluate(()=>window.backupFail=true);await prepareBackup.click();
    await page.getByRole('alert').filter({hasText:lang==='zh-CN'?'附件缺失':'attachment is missing'}).waitFor();
    for(const code of ['MIGRATION_BACKUP_ASSET_CONFIG','MIGRATION_BACKUP_ASSET_DOWNLOAD']){
      await page.evaluate(code=>window.backupRemoteFail=code,code);await prepareBackup.click();
      await page.getByRole('alert').filter({hasText:code.endsWith('CONFIG')?(lang==='zh-CN'?'当前同步配置或凭据不可用':'sync settings or credentials are unavailable'):(lang==='zh-CN'?'未能从当前同步空间下载':'could not be downloaded or verified')}).waitFor();
    }
    await prepareBackup.click();
    await page.getByRole('status').filter({hasText:lang==='zh-CN'?'重新读取并校验':'reopened and verified'}).waitFor();
    await page.evaluate(()=>window.recoveryFail=true);
    await page.getByRole('button',{name:lang==='zh-CN'?'重新校验':'Recheck',exact:true}).click();
    await page.getByRole('alert').filter({hasText:lang==='zh-CN'?'隔离恢复验证未通过':'Isolated recovery or temporary file cleanup failed'}).waitFor();
    assert.equal(await page.getByRole('status').filter({hasText:lang==='zh-CN'?'隔离恢复验证通过':'Isolated recovery passed'}).count(),0);
    await page.evaluate(()=>window.recoveryFail=false);
    await prepareBackup.click();
    await page.getByRole('status').filter({hasText:lang==='zh-CN'?'隔离恢复验证通过':'Isolated recovery passed'}).waitFor();
    const prepareCloud=page.getByRole('button',{name:lang==='zh-CN'?'备份并核对云端':'Back up and verify cloud',exact:true});
    await prepareCloud.click();
    await page.getByRole('status').filter({hasText:lang==='zh-CN'?'本次云端快照校验通过':'Cloud snapshot verified in this operation'}).waitFor();
    await page.evaluate(()=>window.backupRemoteFail='MIGRATION_CLOUD_DOWNLOAD');
    await prepareCloud.click();
    await page.getByRole('alert').filter({hasText:lang==='zh-CN'?'未能读取或校验云端':'could not be read or validated'}).waitFor();
    assert.equal(await page.getByRole('status').filter({hasText:lang==='zh-CN'?'本次云端快照校验通过':'Cloud snapshot verified in this operation'}).count(),0);
    await prepareCloud.click();
    assert.equal(await page.evaluate(()=>window.purgeCalls.length),0,'backup never deletes');
    assert.equal(await page.locator('.publication-agreement').count(),0,'unsettled data cannot publish');
    await page.evaluate(()=>window.settled=true);await prepareCloud.click();
    await page.getByRole('button',{name:lang==='zh-CN'?'准备迁移复制':'Prepare migration copy',exact:true}).click();
    const publish=page.getByRole('button',{name:lang==='zh-CN'?'确认复制':'Confirm copy',exact:true});
    assert.equal(await publish.isDisabled(),true);
    assert.equal(await page.getByRole('status').filter({hasText:lang==='zh-CN'?'隔离恢复验证通过':'Isolated recovery passed'}).count(),0,'publication does not claim a new restore drill');
    await page.locator('.publication-agreement input').check();
    await page.evaluate(()=>window.publicationFail=true);await publish.click();
    await page.getByRole('alert').filter({hasText:lang==='zh-CN'?'迁移复制未完成':'Migration copy did not finish'}).waitFor();
    await publish.click();
    await page.getByText(lang==='zh-CN'?'已记录暂存发布完成':'Staging publication was recorded',{exact:false}).waitFor();
    assert.equal(await page.evaluate(()=>window.backupCalls.filter(a=>a==='publish').length),2);
    assert.equal(await page.evaluate(()=>window.purgeCalls.length),0,'staging publication never purges');
    assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'migration panel overflow');
    await page.screenshot({path:resolve(output,'migration-'+lang+'-'+theme+'.png')});
    await page.setViewportSize({width:320,height:430});
    await prepareBackup.scrollIntoViewIfNeeded();
    assert.ok(await page.locator('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth),'narrow migration panel overflow');
    await page.screenshot({path:resolve(output,'migration-narrow-'+lang+'-'+theme+'.png')});
    await page.setViewportSize({width:480,height:720});
    await page.evaluate(()=>window.backupChanged=true);
    await page.getByRole('button',{name:lang==='zh-CN'?'重新校验':'Recheck',exact:true}).click();
    await page.getByRole('alert').filter({hasText:lang==='zh-CN'?'本机内容已变化':'Local data has changed'}).waitFor();
    await page.keyboard.press('Escape');
    await page.evaluate(()=>window.legacy=false);
    await page.getByRole('button',{name:all,exact:true}).click();
    const execute=page.locator('footer button').last();
    assert.equal(await execute.isDisabled(),true);
    await page.locator('.purge-agreement input').check();
    await page.evaluate(()=>{window.purgeFail=true;window.rows.push({...window.rows[0],uuid:'late'});});
    await execute.click();
    await page.getByRole('status').filter({hasText:lang==='zh-CN'?'未全部完成':'did not finish'}).waitFor();
    assert.equal(await page.evaluate(()=>window.rows.length),3);
    await page.screenshot({path:resolve(output,'purge-'+lang+'-'+theme+'.png')});
    for(const button of await page.locator('footer button').all()) {
      const b=await button.boundingBox(); assert.ok(b.x>=0&&b.y>=0&&b.x+b.width<=481&&b.y+b.height<=721,'purge footer clipped');
    }
    await execute.click();
    await page.waitForFunction(()=>window.rows.length===1);
    assert.deepEqual(await page.evaluate(()=>window.purgeCalls),['test-operation','test-operation']);
    assert.equal(await page.evaluate(()=>window.rows[0].uuid),'late');
    await page.keyboard.press('Escape');
    await page.locator('.record').first().waitFor();
    await page.getByRole('button',{name:lang==='zh-CN'?'批量选择':'Select items',exact:true}).click();
    await page.locator('.selection-check input').check();
    assert.equal(await page.locator('footer button').isDisabled(),false);
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('.selection-check').count(),0);
  }
  assert.deepEqual(errors, []);
  console.log('Trash UI: ' + count + ' locale/theme/window/zoom combinations plus 4 purge and migration preparation/missing-file/stale-preview scenarios passed. Screenshots: ' + output);
} catch (error) {
  if (page) {
    await page.screenshot({ path: resolve(output, 'failure.png') });
    console.error(await page.locator('dialog').evaluate(el => ({ open: el.open, text: el.innerText,
      height: el.getBoundingClientRect().height, content: el.querySelector('.content').getBoundingClientRect().height })));
    console.error('Failure screenshot:', resolve(output, 'failure.png'));
  }
  throw error;
} finally { await browser?.close(); await server.close(); }
