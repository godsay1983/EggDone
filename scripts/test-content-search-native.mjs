// Real WebView2 + Rust IPC. Refuse every identifier except the isolated search build.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(tmpdir(), `eggdone-search-native-${Date.now()}`);
mkdirSync(output, { recursive: true });
const report = { output, cases: [], pageErrors: [], profiles: [], passed: false };
const prefix = `QA-search-${Date.now()}`;
const browser = await chromium.connectOverCDP('http://127.0.0.1:9228');
const page = browser.contexts().flatMap(context => context.pages()).find(p => p.url() === 'http://tauri.localhost/');
const invoke = (command, args) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const dialog = () => page.locator('dialog[aria-labelledby="content-search-heading"][open]');
const group = scope => dialog().locator(`[data-scope="${scope}"]`);
let isolated = false, cdp, db, ownsFaultTable = false, ownsFaultTrigger = false;

function clearSaveFault() {
  if (ownsFaultTrigger) { db.exec('DROP TRIGGER qa_search_block_note_update'); ownsFaultTrigger = false; }
  if (ownsFaultTable) { db.exec('DROP TABLE qa_search_block_updates'); ownsFaultTable = false; }
}
function failNoteSave(uuid) {
  assert.equal(db.prepare("SELECT count(*) AS n FROM sqlite_master WHERE name IN ('qa_search_block_note_update','qa_search_block_updates')").get().n, 0);
  db.exec('CREATE TABLE qa_search_block_updates(uuid TEXT PRIMARY KEY)');
  ownsFaultTable = true;
  db.prepare('INSERT INTO qa_search_block_updates(uuid) VALUES (?)').run(uuid);
  db.exec(`CREATE TRIGGER qa_search_block_note_update BEFORE UPDATE ON notes
    WHEN OLD.uuid IN (SELECT uuid FROM qa_search_block_updates)
    BEGIN SELECT RAISE(ABORT, 'QA_SEARCH_SAVE_FAILURE'); END`);
  ownsFaultTrigger = true;
}

async function screenshot(name) {
  const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
  writeFileSync(resolve(output, name), Buffer.from(shot.data, 'base64'));
}
async function search(query, scope = 'all') {
  await dialog().getByRole('searchbox').fill(query);
  await dialog().getByRole('button', { name: '搜索', exact: true }).click();
  await page.waitForFunction(() => {
    const groups = [...document.querySelectorAll('dialog[open] .group')];
    return groups.length > 0 && groups.every(g => g.getAttribute('aria-busy') === 'false');
  });
  await dialog().getByRole('combobox').selectOption(scope);
}
async function returnToSearch(query, scope) {
  const editor = page.locator('.note-editor:visible');
  if (await editor.count()) await editor.getByRole('button', { name: '返回', exact: true }).click();
  else await page.getByRole('button', { name: '返回来源', exact: true }).first().click();
  await dialog().waitFor();
  assert.equal(await dialog().getByRole('searchbox').inputValue(), query);
  assert.equal(await dialog().getByRole('combobox').inputValue(), scope);
}
function sameRows(actual, expected) {
  assert.deepEqual(actual.map(a => a.uuid), expected.map(a => a.uuid));
}

try {
  assert.ok(page, 'Start the isolated native main WebView first');
  page.setDefaultTimeout(15000);
  assert.equal(await invoke('plugin:app|identifier'), 'com.eggdone.searchtest', 'Refuse production data');
  const settings = await invoke('get_sync_settings');
  assert.equal(settings.enabled, false, 'Tests must not connect a sync space');
  assert.equal(settings.credentialsConfigured, false, 'Use a fresh credential-free test application');
  isolated = true;
  report.appVersion = await invoke('plugin:app|version');
  report.userAgent = await page.evaluate(() => navigator.userAgent);
  assert.ok(process.env.APPDATA);
  const databasePath = resolve(process.env.APPDATA, 'com.eggdone.searchtest/eggdone.sqlite3');
  assert.ok(existsSync(databasePath));
  db = new DatabaseSync(databasePath);
  cdp = await page.context().newCDPSession(page);
  page.on('pageerror', error => report.pageErrors.push(String(error)));
  await invoke('plugin:window|set_size', { label: 'main', value: { Logical: { width: 620, height: 760 } } });
  await invoke('plugin:webview|set_webview_zoom', { label: 'main', value: 1 });

  const note = await invoke('create_note', { title: `${prefix}-source`, content: 'Search attachment source.\n'.repeat(30), color: 'default' });
  const other = await invoke('create_note', { title: `${prefix}-other-parent`, content: 'Duplicate filename must not select this parent.', color: 'default' });
  report.noteUuids = [note.uuid, other.uuid];
  await page.reload();
  await page.getByRole('button', { name: '便签', exact: true }).click();
  await page.getByText(note.title, { exact: true }).click();
  const markdown = Buffer.from('# Search fixture\n\nActual local attachment bytes.\n');
  const fileName = `${prefix}-report.md`;
  await page.locator('input[type=file][accept^=".pdf"]').setInputFiles({ name: fileName, mimeType: 'text/markdown', buffer: markdown });
  await page.getByTitle(fileName, { exact: true }).waitFor();
  const png = readFileSync(resolve(root, 'src-tauri/icons/128x128.png'));
  const imageName = `${prefix}-image.png`;
  await page.locator('input[type=file][accept^="image/"]').setInputFiles({ name: imageName, mimeType: 'image/png', buffer: png });
  await page.getByTitle(imageName, { exact: true }).waitFor();
  const assets = await invoke('list_note_attachments', { noteUuid: note.uuid });
  assert.equal(assets.length, 2);
  for (const asset of assets) {
    assert.deepEqual(Buffer.from(await invoke('read_note_attachment_original', { uuid: asset.uuid })), asset.kind === 'file' ? markdown : png);
  }
  const duplicate = await invoke('create_note_file_attachment', { noteUuid: other.uuid, displayName: fileName, bytes: [...markdown] });
  report.attachmentUuids = [...assets.map(a => a.uuid), duplicate.uuid];
  report.cases.push('Real file-input imports persist Markdown/PNG bytes; another note owns a duplicate filename');
  await page.getByRole('button', { name: '完成', exact: true }).click();

  const tasks = [];
  for (let index = 0; index < 23; index++) {
    tasks.push(await invoke('create_todo', { title: `${prefix}-task-${String(index).padStart(2, '0')}`, groupUuid: null }));
  }
  const archived = tasks[0];
  const archivedContent = 'Archived content must remain complete.\n'.repeat(20) + 'END-ARCHIVED';
  await invoke('update_todo_note', { id: archived.id, note: archivedContent });
  // Simulate a received archive for this run's UUID only, without touching other test records.
  db.prepare('UPDATE todos SET archived_at=? WHERE uuid=?').run(Date.now(), archived.uuid);
  report.taskUuids = tasks.map(t => t.uuid);
  await page.reload();
  await page.locator('.summary-menu-button').click();
  await page.getByRole('menuitem', { name: '统一搜索', exact: true }).click();
  await search(prefix, 'todo');
  assert.equal(await group('todo').locator('.result').count(), 20);
  await group('todo').getByRole('button', { name: '下一页', exact: true }).click();
  await group('todo').getByText('第 2 页', { exact: true }).waitFor();
  assert.equal(await group('todo').locator('.result').count(), 3);
  const taskRow = group('todo').locator('.result').filter({ hasNotText: archived.title }).first();
  const taskTitle = await taskRow.locator('strong').innerText();
  await taskRow.scrollIntoViewIfNeeded();
  const taskScroll = await dialog().locator('.content').evaluate(node => node.scrollTop);
  await taskRow.click();
  await page.getByRole('button', { name: '返回来源', exact: true }).first().waitFor();
  await page.locator('.todo-row').getByText(taskTitle, { exact: true }).waitFor();
  await returnToSearch(prefix, 'todo');
  await group('todo').getByText('第 2 页', { exact: true }).waitFor();
  assert.equal(await dialog().locator('.content').evaluate(node => node.scrollTop), taskScroll);
  report.cases.push('Page-two native task opens and returns with query, category, page and scroll preserved');

  await search(archived.title, 'todo');
  await group('todo').locator('.result').click();
  await dialog().getByText('已归档任务 · 只读', { exact: true }).waitFor();
  assert.equal(await dialog().locator('.body').innerText(), archivedContent.trimEnd());
  assert.equal(await dialog().locator('textarea,input').count(), 0);
  assert.ok(db.prepare('SELECT archived_at FROM todos WHERE uuid=?').get(archived.uuid).archived_at);
  await screenshot('archived-readonly.png');
  await dialog().getByRole('button', { name: '返回结果', exact: true }).click();
  assert.equal(await dialog().getByRole('searchbox').inputValue(), archived.title);
  report.cases.push('Archived task shows full read-only content without restoring or mutating it');

  for (const asset of assets) {
    await search(asset.display_name, 'attachment');
    const result = group('attachment').locator('.result').filter({ hasText: note.title });
    assert.equal(await result.count(), 1);
    await result.click();
    const manager = page.locator('.note-attachment-manager');
    await manager.waitFor();
    assert.equal(await page.getByPlaceholder('便签标题').inputValue(), note.title);
    const target = manager.locator('.search-target');
    assert.equal(await target.count(), 1);
    assert.equal(await target.getAttribute('data-attachment-id'), asset.uuid);
    await page.waitForFunction(uuid => document.activeElement?.getAttribute('data-attachment-id') === uuid, asset.uuid);
    assert.equal(await page.locator('.note-image-viewer').count(), 0);
    sameRows(await invoke('list_note_attachments', { noteUuid: note.uuid }), assets);
    await screenshot(`attachment-${asset.kind}.png`);
    if (asset.kind === 'image') {
      await target.locator('.note-attachment-preview').click();
      await page.locator('.note-image-viewer img').waitFor();
      await page.waitForFunction(() => document.querySelector('.note-image-viewer img')?.naturalWidth === 128);
      await screenshot('image-explicit-open.png');
      await page.keyboard.press('Escape');
      await page.locator('.note-image-viewer').waitFor({ state: 'detached' });
      await manager.waitFor();
      assert.equal(await target.getAttribute('data-attachment-id'), asset.uuid);
    }
    await page.keyboard.press('Escape');
    await manager.waitFor({ state: 'detached' });
    assert.equal(await page.getByPlaceholder('便签标题').inputValue(), note.title);
    await returnToSearch(asset.display_name, 'attachment');
    report.cases.push(`${asset.kind}: correct parent and UUID highlighted/focused; Escape closes only manager; return preserves search`);
  }

  await search(fileName, 'attachment');
  const stale = group('attachment').locator('.result').filter({ hasText: other.title });
  await stale.waitFor();
  await invoke('delete_note', { uuid: other.uuid });
  await stale.click();
  await dialog().getByText('内容已删除或不可用，请重新搜索。', { exact: true }).waitFor();
  assert.equal(await page.locator('.note-attachment-manager').count(), 0);
  assert.equal(await dialog().getByRole('searchbox').inputValue(), fileName);
  await search(fileName, 'attachment');
  assert.equal(await group('attachment').locator('.result').count(), 1);
  report.cases.push('Native stale attachment result rejects a deleted parent and a new search excludes it');

  for (const zoom of [1, 1.25, 1.5]) {
    await invoke('plugin:window|set_size', { label: 'main', value: { Logical: { width: Math.ceil(360 * zoom), height: Math.ceil(560 * zoom) } } });
    await invoke('plugin:webview|set_webview_zoom', { label: 'main', value: zoom });
    assert.equal((await cdp.send('Page.getLayoutMetrics')).cssVisualViewport.zoom, zoom);
    report.profiles.push({ zoom, ...await page.evaluate(() => ({ width: innerWidth, height: innerHeight, theme: document.documentElement.dataset.theme, language: document.documentElement.lang })) });
    await dialog().getByRole('combobox').selectOption('all');
    await dialog().getByRole('combobox').selectOption('attachment');
    assert.ok(await dialog().evaluate(node => {
      const r = node.getBoundingClientRect();
      return r.left >= -1 && r.top >= -1 && r.right <= innerWidth + 1 && r.bottom <= innerHeight + 1 && node.scrollWidth <= node.clientWidth + 1;
    }));
    await group('attachment').locator('.result').click();
    await page.locator('.note-attachment-manager .search-target').waitFor();
    await screenshot(`native-zoom-${zoom}.png`);
    await page.keyboard.press('Escape');
    await page.locator('.note-attachment-manager').waitFor({ state: 'detached' });
    await returnToSearch(fileName, 'attachment');
    report.cases.push(`Native zoom ${zoom}: search bounds and attachment round-trip`);
  }
  await dialog().getByRole('button', { name: '关闭', exact: true }).click();
  await page.reload();
  await page.getByRole('button', { name: '便签', exact: true }).click();
  await page.getByText(note.title, { exact: true }).click();
  const savedContent = 'Source is saved before entering native search.';
  await page.locator('.note-editor textarea').fill(savedContent);
  await page.getByRole('button', { name: '统一搜索', exact: true }).click();
  await dialog().waitFor();
  assert.equal(db.prepare('SELECT content FROM notes WHERE uuid=?').get(note.uuid).content, savedContent);
  await dialog().getByRole('button', { name: '关闭', exact: true }).click();
  assert.equal(await page.locator('.note-editor textarea').inputValue(), savedContent);
  report.cases.push('Editor search entry flushes source content through real Rust/SQLite and returns to it');

  failNoteSave(note.uuid);
  const failedDraft = 'Keep this unsaved draft while native SQLite rejects UPDATE.';
  await page.locator('.note-editor textarea').fill(failedDraft);
  await page.getByRole('button', { name: '统一搜索', exact: true }).click();
  await page.getByText('当前内容未保存成功，请先重试保存再搜索。', { exact: true }).waitFor();
  assert.equal(await dialog().count(), 0);
  assert.equal(await page.locator('.note-editor textarea').inputValue(), failedDraft);
  assert.equal(db.prepare('SELECT content FROM notes WHERE uuid=?').get(note.uuid).content, savedContent);
  await screenshot('source-save-failure.png');
  clearSaveFault();
  await page.locator('.note-editor').getByRole('button', { name: '重试', exact: true }).click();
  await page.locator('.note-editor').getByRole('button', { name: '重试', exact: true }).waitFor({ state: 'detached' });
  assert.equal(db.prepare('SELECT content FROM notes WHERE uuid=?').get(note.uuid).content, failedDraft);
  await page.getByRole('button', { name: '统一搜索', exact: true }).click();
  await dialog().waitFor();
  report.cases.push('Real SQLite source save failure blocks search, retains draft, and retry persists it');
  await search(note.title, 'note');
  await group('note').locator('.result').click();
  await page.getByPlaceholder('便签标题').waitFor();
  failNoteSave(note.uuid);
  const failedTarget = 'Target draft must survive a failed return to search.';
  await page.locator('.note-editor textarea').fill(failedTarget);
  await page.locator('.note-editor').getByRole('button', { name: '返回', exact: true }).click();
  await page.locator('.note-editor').getByRole('button', { name: '重试', exact: true }).waitFor();
  assert.equal(await dialog().count(), 0);
  assert.equal(await page.locator('.note-editor textarea').inputValue(), failedTarget);
  assert.equal(db.prepare('SELECT content FROM notes WHERE uuid=?').get(note.uuid).content, failedDraft);
  clearSaveFault();
  await page.locator('.note-editor').getByRole('button', { name: '重试', exact: true }).click();
  await page.locator('.note-editor').getByRole('button', { name: '重试', exact: true }).waitFor({ state: 'detached' });
  await returnToSearch(note.title, 'note');
  assert.equal(db.prepare('SELECT content FROM notes WHERE uuid=?').get(note.uuid).content, failedTarget);
  report.cases.push('Real SQLite target save failure blocks return; retry restores the same search context');
  await dialog().getByRole('button', { name: '关闭', exact: true }).click();
  await page.reload();
  sameRows(await invoke('list_note_attachments', { noteUuid: note.uuid }), assets);
  assert.equal((await invoke('get_sync_settings')).enabled, false);
  assert.equal(db.prepare("SELECT count(*) AS n FROM sqlite_master WHERE name IN ('qa_search_block_note_update','qa_search_block_updates')").get().n, 0);
  assert.equal(report.cases.length, 12);
  assert.deepEqual(report.pageErrors, []);
  report.passed = true;
} catch (error) {
  report.error = String(error);
  if (isolated && cdp) await screenshot('failure.png').catch(() => {});
  throw error;
} finally {
  clearSaveFault();
  db?.close();
  writeFileSync(resolve(output, 'report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
  await browser.close();
}
