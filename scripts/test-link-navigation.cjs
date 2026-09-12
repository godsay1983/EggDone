const assert = require('node:assert/strict');
const fs = require('node:fs');
const ts = require('typescript');
const source = fs.readFileSync(require('node:path').join(__dirname, '../src/lib/components/TodoPanel.svelte'), 'utf8');
const script = source.slice(source.indexOf('<script lang="ts">') + 18, source.indexOf('</script>'));
const ast = ts.createSourceFile('panel.ts', script, ts.ScriptTarget.Latest, true);
const names = ['openRelatedContent', 'backFromLinkedContent'];
const methods = ast.statements.filter(n => ts.isFunctionDeclaration(n) && names.includes(n.name?.text));
assert.equal(methods.length, names.length);
const body = `let selectedNoteUuid='source', linkedTodoUuid=null, linkHistory=[], linkedTaskEditing=false;
 let noteNavigationBusy=false,noteAttachmentBusy=false,linkNavigating=false,linkManager=null,linkedRequest=null,linkNotice='';
 const $notes={items:[{uuid:'source'},{uuid:'target-note'}],error:null};
 const $todos={items:[{uuid:'target-task'}],error:null};
 ${methods.map(n=>n.getText(ast)).join('\n')}
 return {openRelatedContent,backFromLinkedContent, state:()=>({selectedNoteUuid,linkedTodoUuid,linkHistory,linkNavigating}),
 setEditing:v=>linkedTaskEditing=v, setAttachmentBusy:v=>noteAttachmentBusy=v, noteError:v=>$notes.error=v};`;
const compiled = ts.transpileModule(body,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
function fixture(){
 const events=[]; const deps={
  save:async()=>{events.push('save');}, resolve:async()=>{events.push('resolve');return {link:{todo_uuid:'target-task',note_uuid:'target-note'}};},
  load:async()=>events.push('load'), attachments:async()=>events.push('attachments')
 };
 const h=new Function('flushAllNoteChanges','linkReader','notes','todos','refreshNoteAttachments',compiled)(
  ()=>deps.save(),{resolve:(...a)=>deps.resolve(...a)},{load:()=>deps.load()},{refresh:()=>deps.load()},()=>deps.attachments());
 return {h,deps,events};
}
(async()=>{
 const item={link:{uuid:'link',todo_uuid:'source-task',note_uuid:'source'}};
 const {h,deps,events}=fixture();
 await h.openRelatedContent(item,'note');
 assert.deepEqual(events,['save','resolve','load']);assert.equal(h.state().linkedTodoUuid,'target-task');
 await h.openRelatedContent(item,'todo');assert.equal(h.state().selectedNoteUuid,'target-note');
 assert.equal(h.state().linkHistory.length,2);
 deps.save=async()=>{throw Error('save failed');};
 assert.equal(await h.backFromLinkedContent(),false);assert.equal(h.state().selectedNoteUuid,'target-note');
 deps.save=async()=>{};assert.equal(await h.backFromLinkedContent(),true);assert.equal(h.state().linkedTodoUuid,'target-task');
 h.setEditing(true);assert.equal(await h.backFromLinkedContent(),false);h.setEditing(false);
 assert.equal(await h.backFromLinkedContent(),true);assert.equal(h.state().selectedNoteUuid,'source');
 for (const fault of ['save','resolve','load','attachments']) {
   const f=fixture();f.deps[fault]=async()=>{throw Error(fault);};
   await assert.rejects(f.h.openRelatedContent(item,'todo'),new RegExp(fault));
   assert.equal(f.h.state().selectedNoteUuid,'source');assert.equal(f.h.state().linkHistory.length,0);assert.equal(f.h.state().linkNavigating,false);
 }
 const f=fixture();let release;f.deps.save=()=>new Promise(r=>{release=r;});
 const pending=f.h.openRelatedContent(item,'note');
 await assert.rejects(f.h.openRelatedContent(item,'note'),/BUSY/);release();await pending;
 const readFailure=fixture();readFailure.h.noteError('failed');
 await assert.rejects(readFailure.h.openRelatedContent(item,'todo'),/READ_FAILED/);
 assert.equal(readFailure.h.state().selectedNoteUuid,'source');
 // These production navigation methods must not change persistent filters or the main list route.
 for(const method of methods) assert.doesNotMatch(method.getText(ast),/setListView|focusTodoByUuid|writePreference|setSelectedGroup|searchQuery\s*=/);
 console.log('Production linked navigation: nested return, failures, editing/duplicate guards and route preservation passed');
})().catch(e=>{console.error(e);process.exitCode=1;});
