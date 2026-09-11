const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../src/lib/components/TodoPanel.svelte'), 'utf8');
const script = source.slice(source.indexOf('<script lang="ts">') + 18, source.indexOf('</script>'));
const ast = ts.createSourceFile('panel.ts', script, ts.ScriptTarget.Latest, true);
const fn = ast.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === 'setListView');
assert.ok(fn);
const code = ts.transpileModule(`
let listView='all',taskViewBeforeNotes='all',smartView='overdue',searchQuery='task query',showSearch=true;
const searchContexts={tasks:{query:'',visible:false},notes:{query:'',visible:false}};
let selectedQuadrant='important',selectedAgendaDate='2026-09-12',agendaWeekStartAt=123,agendaWeekVersion=0;
let agendaDatePickerOpen=false,summaryMenuOpen=false,selectedTodoId=1;
const clearSmartView=()=>{smartView=null}; const clearBatchSelection=()=>{},cancelDrag=()=>{};
const startOfAgendaWeek=()=>0,LAST_LIST_VIEW_KEY='last',localStorage={setItem(){}};
${fn.getText(ast)}
return { setListView, query(q,visible=true){searchQuery=q;showSearch=visible},
  state(){return {listView,smartView,searchQuery,showSearch,selectedAgendaDate,selectedQuadrant}},
  initialView(view){listView=view;taskViewBeforeNotes=view;} };
`, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
(async () => {
  const panel = new Function('closeNoteEditor', code)(async () => true);
  await panel.setListView('notes');
  assert.equal(panel.state().searchQuery, ''); assert.equal(panel.state().showSearch, false);
  assert.equal(panel.state().smartView, 'overdue');
  panel.query('note query'); await panel.setListView('all');
  assert.equal(panel.state().searchQuery, 'task query'); assert.equal(panel.state().showSearch, true);
  assert.equal(panel.state().smartView, 'overdue');
  await panel.setListView('notes'); assert.equal(panel.state().searchQuery, 'note query');
  await panel.setListView('today'); assert.equal(panel.state().smartView, null);
  for (const view of ['calendar','quadrants']) {
    const p = new Function('closeNoteEditor', code)(async () => true); p.initialView(view);
    await p.setListView('notes'); await p.setListView(view);
    assert.equal(p.state().listView, view);
    if (view === 'calendar') assert.equal(p.state().selectedAgendaDate, '2026-09-12');
    else assert.equal(p.state().selectedQuadrant, 'important');
  }
  const failed = new Function('closeNoteEditor', code)(async () => false);
  await failed.setListView('notes'); failed.query('unsaved');
  const before = failed.state(); assert.equal(await failed.setListView('all'), false);
  assert.deepEqual(failed.state(), before);
  let finish;
  const delayed = new Function('closeNoteEditor', code)(() => new Promise(r => { finish = r; }));
  await delayed.setListView('notes'); const pending = delayed.setListView('all');
  assert.equal(delayed.state().listView, 'notes'); finish(true); await pending;
  assert.equal(delayed.state().searchQuery, 'task query');
  assert.match(source, /scrollPositions=\{listScrollPositions\}/);
  assert.match(source, /use:preserveScroll=\{\{ positions: listScrollPositions, key: 'tasks'/);
  console.log('Production navigation: separate searches, visibility, smart/calendar/quadrant context, failed/delayed save guards and scroll wiring passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
