// Run only against the isolated debug build documented in L4d2 runtime evidence.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(tmpdir(), `eggdone-link-native-${Date.now()}`);
mkdirSync(output, { recursive: true });
const browser = await chromium.connectOverCDP('http://127.0.0.1:9228');
const page = browser.contexts().flatMap(c => c.pages()).find(p => p.url() === 'http://tauri.localhost/');
const invoke = (command, args) => page.evaluate(({command, args}) => window.__TAURI_INTERNALS__.invoke(command, args), {command, args});
const title = `QA-native-link-${Date.now()}`;
let noteUuid;
let isolated = false;
const evidence = { title, output, cases: [] };
let cdp;
async function screenshot(name) {
  // Playwright's CSS clip crops WebView2 screenshots at native zoom > 100%.
  const shot = await cdp.send('Page.captureScreenshot', {format: 'png', fromSurface: true});
  writeFileSync(resolve(output, name), Buffer.from(shot.data, 'base64'));
}
try {
  assert.ok(page, 'Native main WebView must already be running');
  page.setDefaultTimeout(10000);
  cdp = await page.context().newCDPSession(page);
  assert.equal(await invoke('plugin:app|identifier'), 'com.eggdone.l4d2test', 'Refuse production data');
  isolated = true;
  await page.reload();
  await page.getByRole('button', { name: '便签', exact: true }).click();
  await page.getByRole('button', { name: /新建便签/ }).click();
  await page.getByPlaceholder('便签标题').fill(title);
  await page.getByPlaceholder('写下想法、资料或临时记录…').fill('Native attachment and navigation check.\n'.repeat(100));
  await page.getByRole('button', { name: '完成', exact: true }).click();
  const notes = await invoke('list_notes');
  const note = notes.find(n => n.title === title);
  assert.ok(note, 'UI draft must be persisted by Rust');
  noteUuid = note.uuid;
  await page.getByText(title, { exact: true }).click();
  const markdown = Buffer.from('# Native attachment\n\nL4d2 real IPC and file bytes.\n');
  await page.locator('input[type=file][accept^=".pdf"]').setInputFiles({ name: 'L4d2-native.md', mimeType: 'text/markdown', buffer: markdown });
  await page.getByTitle('L4d2-native.md', { exact: true }).waitFor();
  const imagePath = resolve(root, 'src-tauri/icons/128x128.png');
  await page.locator('input[type=file][accept^="image/"]').setInputFiles(imagePath);
  await page.getByTitle('128x128.png', { exact: true }).waitFor();
  const assets = await invoke('list_note_attachments', { noteUuid });
  assert.equal(assets.length, 2);
  for (const asset of assets) {
    const bytes = Buffer.from(await invoke('read_note_attachment_original', { uuid: asset.uuid }));
    assert.deepEqual(bytes, asset.kind === 'image' ? readFileSync(imagePath) : markdown);
  }
  evidence.cases.push('UI import: real Markdown and PNG bytes round-trip through native asset storage');
  await page.getByTitle('128x128.png', { exact: true }).click();
  await page.locator('.note-attachment-preview').click();
  await page.locator('.note-image-viewer img').waitFor();
  assert.equal(await page.locator('.note-image-viewer img').evaluate(n => n.naturalWidth), 128);
  await screenshot('native-image.png');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('.note-image-viewer').count(), 0);
  assert.equal(await page.locator('.note-attachment-manager').count(), 1);
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('.note-attachment-manager').count(), 0);
  assert.equal(await page.getByPlaceholder('便签标题').inputValue(), title);
  evidence.cases.push('Escape: image -> manager -> same editor');
  await page.locator('.note-editor textarea').evaluate(n => n.scrollTo(0, 500));
  await page.waitForFunction(() => document.querySelector('.note-editor textarea').scrollTop > 100);
  const scrollTop = await page.locator('.note-editor textarea').evaluate(n => n.scrollTop);
  await page.getByRole('button', { name: '添加', exact: true }).click();
  await page.getByRole('button', { name: '创建关联任务', exact: true }).click();
  await page.getByLabel('任务标题', { exact: true }).fill(`${title}-task`);
  await page.locator('dialog button[type=submit]').click();
  await page.locator('dialog').waitFor({ state: 'detached' });
  const links = await invoke('list_task_note_links', { scope: 'note', uuid: noteUuid });
  assert.equal(links.length, 1);
  assert.equal(links[0].todo_title, `${title}-task`);
  await page.getByRole('button', { name: /关联任务 \(1\)/ }).click();
  await page.locator('.note-task-links').getByRole('button', { name: '打开', exact: true }).click();
  await page.getByRole('button', { name: '返回来源', exact: true }).first().waitFor();
  await screenshot('native-linked-task.png');
  await page.getByRole('button', { name: '返回来源', exact: true }).first().click();
  assert.equal(await page.getByPlaceholder('便签标题').inputValue(), title);
  assert.equal(await page.locator('.note-editor textarea').evaluate(n => n.scrollTop), scrollTop);
  assert.equal((await invoke('list_note_attachments', {noteUuid})).length, 2);
  evidence.cases.push('Create related task in UI, open and return with source text scroll restored, without copying attachments');
  await page.getByRole('button', {name: '添加', exact: true}).click();
  await page.getByRole('button', {name: '管理关联', exact: true}).click();
  await page.getByRole('button', {name: '解除关联', exact: true}).click();
  await page.getByRole('button', {name: '取消', exact: true}).click();
  assert.equal((await invoke('list_task_note_links', {scope: 'note', uuid: noteUuid})).length, 1);
  await page.getByRole('button', {name: '关闭', exact: true}).click();
  evidence.cases.push('Cancel unlink in native manager preserves persisted relationship');
  for (const zoom of [1, 1.25, 1.5]) {
    // Real native window and WebView zoom, not CSS zoom or Playwright emulation.
    await invoke('plugin:window|set_size', {label: 'main', value: {Logical: {width: Math.ceil(360 * zoom), height: Math.ceil(560 * zoom)}}});
    await invoke('plugin:webview|set_webview_zoom', {label: 'main', value: zoom});
    await page.getByRole('button', {name: '添加', exact: true}).click();
    await page.getByRole('button', {name: '创建关联任务', exact: true}).click();
    await page.locator('dialog button[type=submit]').scrollIntoViewIfNeeded();
    const bounds = await page.locator('dialog button[type=submit]').boundingBox();
    const viewport = await page.evaluate(() => ({width: innerWidth, height: innerHeight}));
    assert.ok(bounds && bounds.x >= 0 && bounds.y >= 0 && bounds.x + bounds.width <= viewport.width + 1 && bounds.y + bounds.height <= viewport.height + 1);
    const metrics = await cdp.send('Page.getLayoutMetrics');
    assert.equal(metrics.cssVisualViewport.zoom, zoom);
    await screenshot(`native-zoom-${zoom}.png`);
    await page.getByRole('button', {name: '取消', exact: true}).click();
    assert.equal((await invoke('list_task_note_links', {scope: 'note', uuid: noteUuid})).length, 1);
    evidence.cases.push(`Native zoom ${zoom}: related task dialog footer reachable; cancel creates nothing`);
  }
  await page.getByRole('button', {name: '完成', exact: true}).click();
  await page.reload();
  await page.getByRole('button', {name: '便签', exact: true}).click();
  await page.getByText(title, {exact: true}).click();
  assert.equal((await invoke('list_task_note_links', {scope: 'note', uuid: noteUuid})).length, 1);
  assert.equal((await invoke('list_note_attachments', {noteUuid})).length, 2);
  evidence.cases.push('WebView reload restores native note, attachments and link');
  evidence.passed = true;
} catch (error) {
  evidence.error = String(error);
  if (isolated) await screenshot('failure.png').catch(() => {});
  throw error;
} finally {
  evidence.noteUuid = noteUuid;
  writeFileSync(resolve(output, 'report.json'), JSON.stringify(evidence, null, 2));
  console.log(JSON.stringify(evidence, null, 2));
  await browser.close();
}
