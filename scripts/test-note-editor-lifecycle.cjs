const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../src/lib/components/TodoPanel.svelte'), 'utf8');
const script = source.slice(source.indexOf('<script lang="ts">') + '<script lang="ts">'.length,
  source.indexOf('</script>'));
const ast = ts.createSourceFile('TodoPanel.ts', script, ts.ScriptTarget.Latest, true);
const names = ['hasNoteDraftContent', 'persistNoteDraft', 'createNoteFromDraft', 'flushAllNoteChanges',
  'discardNoteDraft', 'closeNoteEditor', 'attachmentDraftTitle', 'ensureNoteForAttachment'];
const functions = ast.statements.filter(n => ts.isFunctionDeclaration(n) && names.includes(n.name?.text));
assert.equal(functions.length, names.length);
const harness = `
  let noteDraft = { uuid: 'draft', title: 'title', content: 'body', color: 'default', pinned: false };
  let noteDraftCreated = null, noteDraftCreatePromise = null, noteDraftSaveTimer = null;
  let selectedNoteUuid = null, selectedNote = null, noteNavigationBusy = false, noteAttachmentBusy = false;
  let linkedRequest = null, linkManager = null, linkNotice = '';
  ${functions.map(n => n.getText(ast)).join('\n')}
  return {
    persistNoteDraft, flushAllNoteChanges, closeNoteEditor, ensureNoteForAttachment,
    edit(patch) { noteDraft = { ...noteDraft, ...patch }; },
    busy(value) { noteAttachmentBusy = value; },
    linking(value) { linkedRequest = value ? { uuid: 'source' } : null; },
    state() { return { noteDraft, noteDraftCreated, selectedNoteUuid, noteNavigationBusy }; }
  };
`;
const compiled = ts.transpileModule(harness, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS }
}).outputText;
function fixture() {
  let persisted = null, pending = null, adds = 0;
  const notes = {
    hasPendingSave: () => pending !== null,
    clearSaveError() {},
    async add(title, content, color) {
      adds++; persisted = { uuid: 'persisted', title, content, color, pinned: false }; return persisted;
    },
    scheduleUpdate(_note, title, content) { pending = { title, content }; },
    async flushPending() {
      if (!pending) return null;
      persisted = { ...persisted, ...pending }; pending = null; return persisted;
    },
    async setColor(_note, color) { persisted = { ...persisted, color }; return persisted; },
    async setPinned(_note, pinned) { persisted = { ...persisted, pinned }; return persisted; }
  };
  const h = new Function('notes', compiled)(notes);
  return { h, notes, saved: () => persisted, adds: () => adds };
}
let count = 0;
async function test(name, run) { await run(); count++; console.log('PASS ' + name); }
(async () => {
  await test('linked task confirmation blocks leaving without writing a note', async () => {
    const { h, adds } = fixture();
    h.linking(true);
    assert.equal(await h.closeNoteEditor(), false);
    assert.equal(adds(), 0);
    assert.equal(h.state().noteDraft.content, 'body');
    h.linking(false);
    assert.equal(await h.closeNoteEditor(), true);
  });
  await test('empty draft and appearance-only edits do not create a record', async () => {
    const { h, adds } = fixture();
    h.edit({ title: '', content: ' ', pinned: true });
    assert.equal(await h.closeNoteEditor(), true);
    assert.equal(adds(), 0); assert.equal(h.state().noteDraft, null);
  });
  await test('attachment-only draft gets a valid identity and filename title', async () => {
    const { h, adds, saved } = fixture();
    h.edit({ title: '', content: '' });
    const n = await h.ensureNoteForAttachment({ name: 'report.pdf' }, 'Attachment');
    assert.equal(n.uuid, 'persisted'); assert.equal(saved().title, 'report');
    assert.equal(await h.closeNoteEditor(), true); assert.equal(adds(), 1);
  });
  await test('create failure blocks closing and retains input', async () => {
    const { h, notes } = fixture();
    notes.add = async () => { throw Error('create failed'); };
    assert.equal(await h.closeNoteEditor(), false);
    assert.equal(h.state().noteDraft.content, 'body'); assert.equal(h.state().noteNavigationBusy, false);
  });
  await test('follow-up failure retries one UUID without creating a second note', async () => {
    const { h, notes, adds, saved } = fixture();
    const add = notes.add, flush = notes.flushPending;
    notes.add = async (...args) => { const n = await add(...args); h.edit({ title: 'latest' }); return n; };
    notes.flushPending = async () => { throw Error('update failed'); };
    assert.equal(await h.closeNoteEditor(), false);
    assert.equal(h.state().noteDraftCreated.uuid, 'persisted');
    notes.flushPending = flush;
    assert.equal(await h.closeNoteEditor(), true);
    assert.equal(adds(), 1); assert.equal(saved().title, 'latest');
  });
  await test('pin failure preserves draft choices for an explicit retry', async () => {
    const { h, notes, adds, saved } = fixture();
    h.edit({ pinned: true });
    const pin = notes.setPinned;
    notes.setPinned = async () => { throw Error('pin failed'); };
    assert.equal(await h.closeNoteEditor(), false);
    assert.equal(h.state().noteDraft.pinned, true);
    notes.setPinned = pin;
    await h.flushAllNoteChanges();
    assert.equal(adds(), 1); assert.equal(saved().pinned, true);
  });
  await test('reverting input after failure replaces the failed queued snapshot', async () => {
    const { h, notes, saved } = fixture();
    const add = notes.add, flush = notes.flushPending;
    notes.add = async (...args) => { const n = await add(...args); h.edit({ title: 'failed snapshot' }); return n; };
    notes.flushPending = async () => { throw Error('failed'); };
    assert.equal(await h.closeNoteEditor(), false);
    h.edit({ title: 'title' }); notes.flushPending = flush;
    assert.equal(await h.closeNoteEditor(), true); assert.equal(saved().title, 'title');
  });
  await test('partial metadata success is retained when a later field fails', async () => {
    const { h, notes, saved, adds } = fixture();
    const add = notes.add, pin = notes.setPinned;
    notes.add = async (...args) => { const n = await add(...args); h.edit({ color: 'blue', pinned: true }); return n; };
    notes.setPinned = async () => { throw Error('failed'); };
    assert.equal(await h.closeNoteEditor(), false); assert.equal(saved().color, 'blue');
    h.edit({ color: 'default', pinned: false }); notes.setPinned = pin;
    assert.equal(await h.closeNoteEditor(), true); assert.equal(saved().color, 'default');
    assert.equal(adds(), 1);
  });
  await test('typing during a metadata save is drained before closing', async () => {
    const { h, notes, saved } = fixture();
    h.edit({ pinned: true });
    const pin = notes.setPinned;
    notes.setPinned = async (...args) => { h.edit({ content: 'newest' }); return pin(...args); };
    assert.equal(await h.closeNoteEditor(), true); assert.equal(saved().content, 'newest');
  });
  await test('concurrent closes wait for one create and reject duplicate navigation', async () => {
    const { h, notes, adds } = fixture();
    const add = notes.add;
    let finish;
    notes.add = async (...args) => { await new Promise(resolve => { finish = resolve; }); return add(...args); };
    const first = h.closeNoteEditor();
    assert.equal(await h.closeNoteEditor(), false);
    assert.notEqual(h.state().noteDraft, null);
    finish(); assert.equal(await first, true); assert.equal(adds(), 1);
  });
  await test('attachment activity keeps the editor open', async () => {
    const { h, adds } = fixture(); h.busy(true);
    assert.equal(await h.closeNoteEditor(), false); assert.equal(adds(), 0);
  });
  assert.match(source, /view !== 'notes' && !\(await closeNoteEditor\(\)\)/);
  assert.match(source, /selectedNoteUuid === note.uuid \|\| !\(await closeNoteEditor\(\)\)/);
  console.log(`${count} production editor lifecycle scenarios passed; not native-window acceptance.`);
})().catch(error => { console.error(error); process.exitCode = 1; });
