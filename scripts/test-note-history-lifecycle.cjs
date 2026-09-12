const assert = require('node:assert/strict');
const fs = require('node:fs');
const ts = require('typescript');
const path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, '../src/lib/components/TodoPanel.svelte'), 'utf8');
const script = source.slice(source.indexOf('<script lang="ts">') + 18, source.indexOf('</script>'));
const ast = ts.createSourceFile('panel.ts', script, ts.ScriptTarget.Latest, true);
const names = ['openNoteHistory', 'assertHistoryReady', 'refreshHistoryEditor', 'afterHistoryRestore',
  'closeNoteHistory', 'updateNote', 'closeNoteEditor'];
const methods = ast.statements.filter(n => ts.isFunctionDeclaration(n) && names.includes(n.name?.text));
assert.equal(methods.length, names.length);
const body = `
 let historyOpening=false,historyUuid=null,historyEditorRevision=0,historyRefreshed=false,historyOpenError=false;
 let contentSearchActive=false,contentSearchOpening=false,contentSearchSession=false,searchAttachmentUuid='',linkedTodoUuid=null;
 let selectedNote={uuid:'n',title:'current'},selectedNoteUuid='n',noteDraft=null;
 let noteNavigationBusy=false,noteAttachmentBusy=false,linkNavigating=false,linkedRequest=null,linkManager=null;
 let linkHistory=[{noteUuid:'source',todoUuid:null}],linkNotice='',noteAttachmentError=null;
 const $notes={items:[selectedNote],error:null},$translator=k=>k;
 ${methods.map(n => n.getText(ast)).join('\n')}
 return {openNoteHistory,assertHistoryReady,refreshHistoryEditor,afterHistoryRestore,closeNoteHistory,updateNote,closeNoteEditor,
  state:()=>({historyOpening,historyUuid,selectedNoteUuid,historyEditorRevision,linkHistory,historyOpenError,contentSearchActive,searchAttachmentUuid}),
  fromSearch:()=>{contentSearchSession=true;searchAttachmentUuid='asset';},
  failRead:()=>{$notes.error='failed';},clearRead:()=>{$notes.error=null;},
  remove:()=>{$notes.items=[];},switchSource:()=>{selectedNoteUuid='other';}};`;
const compiled = ts.transpileModule(body, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
function fixture() {
 const calls=[];const deps={flush:async()=>calls.push('flush'),pending:false,refresh:async()=>calls.push('refresh')};
 const notes={hasPendingSave:()=>deps.pending,scheduleUpdate:()=>calls.push('save'),refresh:()=>deps.refresh()};
 const h=new Function('notes','flushAllNoteChanges','scheduleAutoSync','NOTE_DRAFT_UUID',compiled)
  (notes,()=>deps.flush(),()=>calls.push('sync'),'draft');
 return {h,deps,calls};
}
(async()=>{
 const f=fixture();let release;f.deps.flush=()=>new Promise(r=>{release=r;});
 const opening=f.h.openNoteHistory();assert.equal(f.h.state().historyOpening,true);
 f.h.updateNote({uuid:'n'},'late','input');assert.equal(await f.h.closeNoteEditor(),false);
 await f.h.openNoteHistory();release();await opening;
 assert.equal(f.h.state().historyUuid,'n');assert.equal(f.calls.includes('save'),false);
 f.deps.pending=true;assert.throws(f.h.assertHistoryReady,/BUSY/);f.deps.pending=false;
 f.h.assertHistoryReady();await f.h.afterHistoryRestore(true);
 assert.deepEqual(f.calls,['sync','refresh']);
 f.h.updateNote({uuid:'n'},'old','late');assert.equal(f.calls.includes('save'),false);
 f.h.closeNoteHistory(false);assert.equal(f.h.state().historyEditorRevision,1);
 assert.equal(f.h.state().selectedNoteUuid,'n');assert.equal(f.h.state().linkHistory.length,1);
 const failed=fixture();failed.deps.flush=async()=>{throw Error('save');};await failed.h.openNoteHistory();
 assert.equal(failed.h.state().historyUuid,null);assert.equal(failed.h.state().selectedNoteUuid,'n');
 assert.equal(failed.h.state().historyOpenError,true);
 const refresh=fixture();await refresh.h.openNoteHistory();refresh.h.failRead();
 await assert.rejects(refresh.h.afterHistoryRestore(true),/REFRESH_FAILED/);
 refresh.h.closeNoteHistory(true);assert.equal(refresh.h.state().selectedNoteUuid,null);
 assert.equal(refresh.calls.includes('save'),false);
 const search=fixture();search.h.fromSearch();await search.h.openNoteHistory();search.h.failRead();
 await assert.rejects(search.h.afterHistoryRestore(true),/REFRESH_FAILED/);
 search.h.closeNoteHistory(true);assert.equal(search.h.state().contentSearchActive,true);
 assert.equal(search.h.state().searchAttachmentUuid,'');assert.equal(search.h.state().linkHistory.length,0);
 const retry=fixture();await retry.h.openNoteHistory();retry.h.failRead();
 await assert.rejects(retry.h.afterHistoryRestore(true));retry.h.clearRead();await retry.h.refreshHistoryEditor();
 retry.h.closeNoteHistory(false);assert.equal(retry.h.state().historyEditorRevision,1);
 assert.equal(retry.calls.filter(c=>c==='sync').length,1);
 const noop=fixture();await noop.h.openNoteHistory();await noop.h.afterHistoryRestore(false);
 assert.equal(noop.calls.includes('sync'),false);noop.h.closeNoteHistory(false);
 assert.equal(noop.h.state().historyEditorRevision,1);
 const switched=fixture();switched.deps.flush=async()=>switched.h.switchSource();await switched.h.openNoteHistory();
 assert.equal(switched.h.state().historyUuid,null);
 const deleted=fixture();await deleted.h.openNoteHistory();deleted.h.remove();
 await assert.rejects(deleted.h.afterHistoryRestore(true),/REFRESH_FAILED/);
 assert.match(source,/locked=\{historyOpening \|\| historyUuid !== null \|\| contentSearchOpening \|\| contentSearchActive\}/);
 assert.match(source,/#key selectedNote.uuid \+ ':' \+ historyEditorRevision/);
 console.log('History editor lifecycle: flush, input freeze, source/pending guards, refresh, no-op, failed-refresh close/retry passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
