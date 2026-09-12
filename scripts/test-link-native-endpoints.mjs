// Native endpoint races and read-error recovery. Never run against production data.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const output = resolve(tmpdir(), `eggdone-native-endpoints-${Date.now()}`);
mkdirSync(output, { recursive: true });
const browser = await chromium.connectOverCDP('http://127.0.0.1:9228');
const page = browser.contexts().flatMap(c => c.pages()).find(p => p.url() === 'http://tauri.localhost/');
const invoke = (command, args) => page.evaluate(({command, args}) => window.__TAURI_INTERNALS__.invoke(command, args), {command, args});
const report = { cases: [], pageErrors: [] };
const prefix = `QA-native-endpoints-${Date.now()}`;
let db, cdp, readFault;
async function openSource(title) {
  await page.reload();
  await page.getByRole('button', {name: '便签', exact: true}).click();
  await page.getByText(title, {exact: true}).click();
}
async function manage() {
  await page.getByRole('button', {name: '添加', exact: true}).click();
  await page.getByRole('button', {name: '管理关联', exact: true}).click();
  await page.locator('dialog input[type=search]').waitFor();
}
const close = () => page.locator('dialog').getByRole('button', {name: '关闭', exact: true}).click();
const row = title => page.locator('dialog li').filter({has: page.getByText(title, {exact: true})});
try {
  assert.ok(page);
  page.setDefaultTimeout(10000);
  assert.equal(await invoke('plugin:app|identifier'), 'com.eggdone.l4d2test');
  assert.ok(process.env.APPDATA);
  db = new DatabaseSync(resolve(process.env.APPDATA, 'com.eggdone.l4d2test/eggdone.sqlite3'));
  cdp = await page.context().newCDPSession(page);
  page.on('pageerror', error => report.pageErrors.push(String(error)));
  const source = await invoke('create_note', {title: prefix, content: 'Endpoint source stays intact.', color: 'default'});
  const todoUuid = randomUUID();
  const original = await invoke('create_linked_todo', {draft: {
    todo_uuid: todoUuid, note_uuid: source.uuid, title: `${prefix}-task`, note: '',
    group_uuid: null, due_date: null, due_at: null, reminder_at: null, priority: 0,
  }});
  const pair = {todoUuid, noteUuid: source.uuid};
  const task = (await invoke('list_todos')).find(t => t.uuid === todoUuid);
  assert.ok(task);
  report.sourceUuid = source.uuid;
  report.todoUuid = todoUuid;

  await invoke('set_todo_completed', {id: task.id, completed: true});
  await openSource(prefix);
  await manage();
  await row(task.title).getByText('已完成', {exact: true}).waitFor();
  await row(task.title).getByRole('button', {name: '打开', exact: true}).click();
  await page.getByRole('button', {name: '返回来源', exact: true}).first().click();
  assert.equal(await page.getByPlaceholder('便签标题').inputValue(), prefix);
  report.cases.push('Completed native task opens and returns to the unchanged note');

  await manage();
  await row(task.title).getByRole('button', {name: '打开', exact: true}).waitFor();
  // Model an externally arriving archive for this QA endpoint, without archiving other tasks.
  db.prepare('UPDATE todos SET archived_at=? WHERE uuid=?').run(Date.now(), todoUuid);
  await row(task.title).getByRole('button', {name: '打开', exact: true}).click();
  await page.getByText('此任务已归档，关联仍保留；此处暂不打开归档任务。', {exact: true}).waitFor();
  await row(task.title).getByText('已归档', {exact: true}).waitFor();
  assert.deepEqual(await invoke('get_task_note_link', pair), original);
  await row(task.title).getByRole('button', {name: '解除关联', exact: true}).click();
  await page.locator('dialog').getByRole('button', {name: '确认', exact: true}).click();
  await page.getByText('关联变更已保存到本机。', {exact: true}).waitFor();
  assert.ok((await invoke('get_task_note_link', pair)).deleted_at);
  assert.ok(db.prepare('SELECT archived_at FROM todos WHERE uuid=?').get(todoUuid).archived_at);
  report.cases.push('Stale open rechecks archive; explicit unlink preserves the archived endpoint');
  await close();
  db.prepare('UPDATE todos SET archived_at=NULL WHERE uuid=?').run(todoUuid);
  const active = await invoke('change_task_note_link', {...pair, active: true, expected: await invoke('get_task_note_link', pair)});

  // Corrupt only the newly created QA link record; production reads must expose an error.
  readFault = {uuid: active.uuid, json: db.prepare('SELECT record_json FROM task_note_links WHERE uuid=?').get(active.uuid).record_json};
  db.prepare('UPDATE task_note_links SET record_json=? WHERE uuid=?').run('{invalid', active.uuid);
  await manage();
  await page.getByText('关联或候选内容读取失败，请重试。', {exact: true}).waitFor();
  assert.equal(await page.locator('dialog .candidate:not(:disabled)').count(), 0);
  db.prepare('UPDATE task_note_links SET record_json=? WHERE uuid=?').run(readFault.json, readFault.uuid);
  readFault = undefined;
  await page.locator('dialog').getByRole('button', {name: '重试', exact: true}).click();
  await row(task.title).getByRole('button', {name: '打开', exact: true}).waitFor();
  assert.deepEqual(await invoke('get_task_note_link', pair), active);
  report.cases.push('Real native link read failure is not an empty list; retry restores the unchanged relationship');

  await invoke('delete_todo', {id: task.id, repeatScope: null});
  await row(task.title).getByRole('button', {name: '打开', exact: true}).click();
  await page.getByText('未能打开关联内容。请先保存当前便签后重试，目标内容或关联也可能已不可用。', {exact: true}).waitFor();
  assert.equal((await invoke('list_task_note_links', {scope: 'note', uuid: source.uuid})).length, 0);
  const deletion = await invoke('get_task_note_link', pair);
  assert.ok(deletion.deleted_at);
  await invoke('restore_todo', {id: task.id});
  assert.deepEqual(await invoke('get_task_note_link', pair), deletion);
  await close();
  report.cases.push('Deleted target cannot open from stale manager; restoring it does not resurrect the relationship');

  const candidate = await invoke('create_todo', {title: `${prefix}-candidate`, groupUuid: null});
  await manage();
  await page.locator('dialog').getByRole('button', {name: candidate.title, exact: true}).click();
  await page.locator('dialog').getByRole('button', {name: '确认', exact: true}).waitFor();
  await invoke('delete_todo', {id: candidate.id, repeatScope: null});
  await page.locator('dialog').getByRole('button', {name: '确认', exact: true}).click();
  await page.getByText('操作未完成，内容或关联可能已变化。请刷新后重新选择并确认。', {exact: true}).waitFor();
  assert.equal(await invoke('get_task_note_link', {todoUuid: candidate.uuid, noteUuid: source.uuid}), null);
  assert.equal(await page.locator('dialog').getByRole('button', {name: candidate.title, exact: true}).count(), 0);
  assert.equal(db.prepare('SELECT deleted_at FROM notes WHERE uuid=?').get(source.uuid).deleted_at, null);
  report.cases.push('Candidate deleted after selection is rejected at confirmation, with no link or source mutation');
  await close();

  await invoke('change_task_note_link', {...pair, active: true, expected: deletion});
  const targetNote = await invoke('create_note', {title: `${prefix}-target-note`, content: 'Reverse navigation content.', color: 'default'});
  const reversePair = {todoUuid, noteUuid: targetNote.uuid};
  await invoke('change_task_note_link', {...reversePair, active: true, expected: null});
  await openSource(prefix);
  await manage();
  await row(task.title).getByRole('button', {name: '打开', exact: true}).click();
  const manageFromTask = async () => {
    await page.getByRole('button', {name: `更多: ${task.title}`, exact: true}).click();
    await page.getByRole('menuitem', {name: '关联便签', exact: true}).click();
    await row(targetNote.title).getByRole('button', {name: '打开', exact: true}).waitFor();
  };
  await manageFromTask();
  await row(targetNote.title).getByRole('button', {name: '打开', exact: true}).click();
  assert.equal(await page.getByPlaceholder('便签标题').inputValue(), targetNote.title);
  assert.equal(await page.getByPlaceholder('写下想法、资料或临时记录…').inputValue(), targetNote.content);
  await page.getByRole('button', {name: '返回', exact: true}).click();
  await manageFromTask();
  await invoke('delete_note', {uuid: targetNote.uuid});
  await row(targetNote.title).getByRole('button', {name: '打开', exact: true}).click();
  await page.getByText('未能打开关联内容。请先保存当前便签后重试，目标内容或关联也可能已不可用。', {exact: true}).waitFor();
  const noteDeletion = await invoke('get_task_note_link', reversePair);
  assert.ok(noteDeletion.deleted_at);
  await invoke('restore_note', {uuid: targetNote.uuid});
  assert.deepEqual(await invoke('get_task_note_link', reversePair), noteDeletion);
  // Select the accessible modal action; DOM order differs from native top-layer order.
  await page.getByRole('button', {name: '关闭', exact: true}).click();
  await page.getByRole('button', {name: '返回来源', exact: true}).first().click();
  assert.equal(await page.getByPlaceholder('便签标题').inputValue(), prefix);
  report.cases.push('Task-to-note open/return works; deleted note cannot open from stale manager and restore does not reconnect');
  assert.deepEqual(report.pageErrors, []);
  report.passed = true;
} catch (error) {
  report.error = String(error);
  if (cdp) {
    const shot = await cdp.send('Page.captureScreenshot', {format: 'png', fromSurface: true}).catch(() => null);
    if (shot) writeFileSync(resolve(output, 'failure.png'), Buffer.from(shot.data, 'base64'));
  }
  throw error;
} finally {
  if (db) {
    if (readFault) db.prepare('UPDATE task_note_links SET record_json=? WHERE uuid=?').run(readFault.json, readFault.uuid);
    report.readFaultRestored = !readFault || db.prepare('SELECT record_json FROM task_note_links WHERE uuid=?').get(readFault.uuid).record_json === readFault.json;
    db.close();
  }
  writeFileSync(resolve(output, 'report.json'), JSON.stringify(report, null, 2));
  console.log(output, JSON.stringify(report, null, 2));
  await browser.close();
}
